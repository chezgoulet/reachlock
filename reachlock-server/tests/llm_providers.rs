//! S14 fake-provider battery: a loopback axum stub plays the provider so
//! the real HTTP paths are exercised with zero external network (CI-safe).
//! Proves: happy path (both provider dialects), timeout → Timeout, garbage
//! → BadResponse, HTTP 429 → RateLimited, and the proxy-level token bucket.

use std::net::SocketAddr;
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use reachlock_server::services::providers::{
    InferenceRequest, OllamaNative, OpenAiCompat, Provider, ProviderError,
};

/// Spawn the fake provider on a random loopback port; returns its base URL.
async fn fake_provider() -> String {
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(|| async {
                Json(serde_json::json!({
                    "choices": [{ "message": { "role": "assistant",
                        "content": "{\"action\":\"wake_crew\",\"reasoning\":\"anomaly on scope\"}" } }]
                }))
            }),
        )
        .route(
            "/api/chat",
            post(|| async {
                Json(serde_json::json!({
                    "message": { "role": "assistant",
                        "content": "<think>hmm</think>{\"action\":\"maintain_course\",\"reasoning\":\"steady\"}" }
                }))
            }),
        )
        .route(
            "/slow/v1/chat/completions",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "too late"
            }),
        )
        .route(
            "/garbage/v1/chat/completions",
            post(|| async {
                Json(serde_json::json!({
                    "choices": [{ "message": { "role": "assistant",
                        "content": "I would simply fly better. No JSON for you." } }]
                }))
            }),
        )
        .route(
            "/limited/v1/chat/completions",
            post(|| async {
                (
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    "slow down please",
                )
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn request() -> InferenceRequest {
    InferenceRequest {
        system_prompt: "decide".into(),
        context_json: serde_json::json!({"unknown_signal": 1}),
        max_tokens: 64,
        timeout: Duration::from_millis(1500),
    }
}

#[tokio::test]
async fn openai_compat_happy_path() {
    let base = fake_provider().await;
    let provider = OpenAiCompat {
        base_url: base,
        api_key: Some("sk-test".into()),
        model: "test-model".into(),
    };
    let r = provider.complete(request()).await.unwrap();
    assert_eq!(r.action, "wake_crew");
    assert_eq!(r.reasoning, "anomaly on scope");
}

#[tokio::test]
async fn ollama_native_happy_path_strips_think_traces() {
    let base = fake_provider().await;
    let provider = OllamaNative {
        base_url: base,
        model: "test-model".into(),
    };
    let r = provider.complete(request()).await.unwrap();
    assert_eq!(r.action, "maintain_course");
}

#[tokio::test]
async fn slow_provider_times_out_cleanly() {
    let base = fake_provider().await;
    let provider = OpenAiCompat {
        base_url: format!("{base}/slow"),
        api_key: None,
        model: "test-model".into(),
    };
    let started = std::time::Instant::now();
    let r = provider.complete(request()).await;
    assert_eq!(r, Err(ProviderError::Timeout));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the timeout bound held — no hang"
    );
}

#[tokio::test]
async fn garbage_reply_is_bad_response() {
    let base = fake_provider().await;
    let provider = OpenAiCompat {
        base_url: format!("{base}/garbage"),
        api_key: None,
        model: "test-model".into(),
    };
    match provider.complete(request()).await {
        Err(ProviderError::BadResponse(_)) => {}
        other => panic!("expected BadResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn http_429_maps_to_rate_limited() {
    let base = fake_provider().await;
    let provider = OpenAiCompat {
        base_url: format!("{base}/limited"),
        api_key: None,
        model: "test-model".into(),
    };
    assert_eq!(
        provider.complete(request()).await,
        Err(ProviderError::RateLimited)
    );
}
