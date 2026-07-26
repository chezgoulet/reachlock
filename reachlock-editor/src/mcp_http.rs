//! MCP over HTTP from the running editor (S101 P7).
//!
//! The stdio server in [`crate::mcp`] is headless, so it cannot touch tabs.
//! This one runs inside the editor process and holds a
//! [`SessionHandle`](crate::agent::bridge::SessionHandle), so an external MCP
//! client gets the *full* surface — including `open_tab`, `write_document` and
//! `save_all` against the author's live session.
//!
//! Both transports call [`crate::mcp::handle_request`], so they cannot answer
//! the same method differently.
//!
//! # Why this is off by default and locked down
//!
//! This endpoint can write files. Three deliberate constraints:
//!
//! 1. **Opt-in.** Nothing listens until the author turns it on.
//! 2. **Loopback only.** Bound to `127.0.0.1`, never `0.0.0.0`. A content
//!    editor that writes to disk has no business accepting connections from
//!    the network.
//! 3. **Bearer token.** A random token is minted per enable and must be sent
//!    as `Authorization: Bearer …`. Loopback alone is not access control —
//!    any local process, including a browser tab hitting the port, would
//!    otherwise be able to drive the editor.
//!
//! The transport is hand-rolled HTTP/1.1: a JSON-RPC POST with a
//! `Content-Length` body and a JSON response. That is the whole of MCP's
//! Streamable HTTP transport that request/response needs, and it avoids
//! pulling a server framework into a GUI crate. Streaming (SSE) is not
//! implemented; nothing here is long-running.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;

use crate::agent::bridge::SessionHandle;
use crate::agent::tools::{ToolCtx, ToolRegistry};

/// Refuse a body larger than this. A tool argument is a document, not a file
/// upload, and an unbounded read from a socket is a memory-exhaustion bug.
const MAX_BODY: usize = 8 * 1024 * 1024;

/// How much of a rejected request's body to read and discard before closing.
///
/// Closing a socket with unread data queued makes the peer see a connection
/// reset instead of the response that was already written — so a client gets
/// an I/O error rather than the 401 explaining what was wrong. Draining first
/// avoids that. Bounded, because the whole point of rejecting early is not
/// allocating for an unauthenticated caller.
const MAX_DRAIN: usize = 64 * 1024;

/// A running server. Dropping it stops the listener.
pub struct McpHttpServer {
    pub addr: SocketAddr,
    pub token: String,
    stop: Arc<AtomicBool>,
}

impl McpHttpServer {
    /// Bind loopback and start serving. `port` 0 asks the OS for a free one.
    pub fn start(port: u16, session: SessionHandle) -> Result<Self, String> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .map_err(|e| format!("could not bind 127.0.0.1:{port}: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("could not read the bound address: {e}"))?;
        // Non-blocking so the accept loop can notice `stop` instead of
        // sitting in `accept()` until the next connection arrives.
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("could not configure the listener: {e}"))?;

        let token = mint_token();
        let stop = Arc::new(AtomicBool::new(false));

        let thread_token = token.clone();
        let thread_stop = stop.clone();
        std::thread::spawn(move || {
            let registry = ToolRegistry::new();
            let ctx = ToolCtx::with_session(session);
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _peer)) => {
                        // Serve connections one at a time. Every session tool
                        // funnels through a single UI thread anyway, so
                        // concurrency here would buy nothing and would let two
                        // clients interleave writes to the same tab.
                        let _ = stream.set_nonblocking(false);
                        serve_one(stream, &registry, &ctx, &thread_token);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(McpHttpServer { addr, token, stop })
    }

    /// The line to hand to an MCP client.
    pub fn client_hint(&self) -> String {
        format!(
            "http://{} — Authorization: Bearer {}",
            self.addr, self.token
        )
    }
}

impl Drop for McpHttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A random token, from process and time entropy.
///
/// Not a CSPRNG: the threat model is another local process guessing the token
/// within one editor session, and the crate has no RNG dependency. If this
/// ever needs to resist a determined local attacker, swap in a real one — the
/// call site is this function and nothing else.
fn mint_token() -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};

    fn mix(salt: u64) -> u64 {
        let mut h = DefaultHasher::new();
        salt.hash(&mut h);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .hash(&mut h);
        std::process::id().hash(&mut h);
        // Stack address: ASLR contributes entropy the clock does not.
        let local = 0u8;
        (std::ptr::addr_of!(local) as usize).hash(&mut h);
        h.finish()
    }

    format!("{:016x}{:016x}", mix(0), mix(1))
}

/// Read and discard up to [`MAX_DRAIN`] bytes of a body we are not going to
/// process, so the response is not lost to a connection reset.
fn drain_body(reader: &mut BufReader<TcpStream>, content_length: usize) {
    let mut remaining = content_length.min(MAX_DRAIN);
    let mut scratch = [0u8; 4096];
    while remaining > 0 {
        let want = remaining.min(scratch.len());
        match reader.read(&mut scratch[..want]) {
            Ok(0) | Err(_) => return,
            Ok(n) => remaining -= n,
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

fn serve_one(mut stream: TcpStream, registry: &ToolRegistry, ctx: &ToolCtx, token: &str) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let is_post = request_line.starts_with("POST ");

    let mut content_length = 0usize;
    let mut authorized = false;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => return,
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        // Header names are case-insensitive; values are not. Split the
        // original line and lowercase only the name, so the token is compared
        // exactly as sent.
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "authorization" => {
                authorized = value
                    .strip_prefix("Bearer ")
                    .is_some_and(|t| constant_time_eq(t.trim(), token));
            }
            _ => {}
        }
    }

    if !authorized {
        // The body is drained, not parsed: an unauthenticated caller still
        // must not make the editor allocate megabytes, but closing on a
        // half-sent request loses the 401 to a reset.
        drain_body(&mut reader, content_length);
        respond(
            &mut stream,
            "401 Unauthorized",
            &json!({"error": "missing or invalid bearer token"}).to_string(),
        );
        return;
    }
    if !is_post {
        drain_body(&mut reader, content_length);
        respond(
            &mut stream,
            "405 Method Not Allowed",
            &json!({"error": "this endpoint takes JSON-RPC over POST"}).to_string(),
        );
        return;
    }
    if content_length > MAX_BODY {
        // Deliberately not drained — that is the case this limit exists for.
        respond(
            &mut stream,
            "413 Payload Too Large",
            &json!({"error": format!("body exceeds {MAX_BODY} bytes")}).to_string(),
        );
        return;
    }

    let mut body = vec![0u8; content_length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_slice::<serde_json::Value>(&body) else {
        respond(
            &mut stream,
            "400 Bad Request",
            &json!({"error": "body is not JSON"}).to_string(),
        );
        return;
    };

    match crate::mcp::handle_request(registry, &request, ctx) {
        Some(response) => respond(&mut stream, "200 OK", &response.to_string()),
        // A notification. 202 with no body is what the MCP HTTP transport
        // expects; answering with a JSON-RPC response would be a violation.
        None => {
            let _ = write!(
                stream,
                "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.flush();
        }
    }
}

/// Length-independent comparison, so a wrong token cannot be narrowed down by
/// timing one byte at a time.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::bridge::{SessionOp, SessionQueue};

    /// Drive the server the way a client would.
    fn post(addr: SocketAddr, token: Option<&str>, body: &str) -> (String, String) {
        let mut s = TcpStream::connect(addr).expect("connect");
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        write!(
            s,
            "POST / HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("write");
        s.flush().unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).expect("read");
        let (head, body) = out.split_once("\r\n\r\n").unwrap_or((out.as_str(), ""));
        let status = head.lines().next().unwrap_or_default().to_string();
        (status, body.to_string())
    }

    fn server() -> (McpHttpServer, SessionQueue) {
        let queue = SessionQueue::new();
        let server = McpHttpServer::start(0, queue.handle()).expect("bind loopback");
        (server, queue)
    }

    #[test]
    fn it_binds_loopback_only() {
        let (server, _q) = server();
        assert!(server.addr.ip().is_loopback(), "bound {}", server.addr);
    }

    /// Loopback is not access control: any local process could otherwise
    /// drive the editor's write tools.
    #[test]
    fn an_unauthenticated_request_is_refused() {
        let (server, _q) = server();
        let (status, _) = post(
            server.addr,
            None,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        assert!(status.contains("401"), "{status}");

        let (status, _) = post(
            server.addr,
            Some("not-the-token"),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        assert!(status.contains("401"), "{status}");
    }

    /// The point of this transport: unlike stdio, it advertises the live-tab
    /// tools, because it has a session to run them on.
    #[test]
    fn an_authorized_client_gets_the_session_tools_too() {
        let (server, _q) = server();
        let (status, body) = post(
            server.addr,
            Some(&server.token),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        assert!(status.contains("200"), "{status}");
        let v: serde_json::Value = serde_json::from_str(&body).expect("json body");
        let names: Vec<String> = v["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect();
        assert!(names.iter().any(|n| n == "write_document"), "{names:?}");
        assert!(names.iter().any(|n| n == "query_content"), "{names:?}");
    }

    /// A session tool must actually reach the UI thread over the bridge.
    #[test]
    fn a_session_tool_reaches_the_ui_thread() {
        let (server, queue) = server();
        let addr = server.addr;
        let token = server.token.clone();
        let client = std::thread::spawn(move || {
            post(
                addr,
                Some(&token),
                r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_tabs"}}"#,
            )
        });

        // Stand in for the editor's per-frame drain.
        loop {
            for req in queue.drain() {
                assert_eq!(req.op, SessionOp::ListTabs);
                req.reply(crate::agent::tools::ToolOutcome::ok("two tabs"));
            }
            if client.is_finished() {
                break;
            }
            std::thread::yield_now();
        }
        let (status, body) = client.join().unwrap();
        assert!(status.contains("200"), "{status}");
        assert!(body.contains("two tabs"), "{body}");
    }

    #[test]
    fn a_notification_is_accepted_with_no_body() {
        let (server, _q) = server();
        let (status, body) = post(
            server.addr,
            Some(&server.token),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );
        assert!(status.contains("202"), "{status}");
        assert!(body.is_empty(), "{body}");
    }

    #[test]
    fn a_get_is_rejected_but_only_after_authentication() {
        let (server, _q) = server();
        let mut s = TcpStream::connect(server.addr).unwrap();
        write!(
            s,
            "GET / HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\r\n",
            server.token
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert!(out.starts_with("HTTP/1.1 405"), "{out}");
    }

    #[test]
    fn tokens_differ_between_servers() {
        let (a, _qa) = server();
        let (b, _qb) = server();
        assert_ne!(a.token, b.token);
        assert_eq!(a.token.len(), 32);
    }

    /// A token that repeats would let a stale client keep access after the
    /// author toggled the endpoint off and on.
    #[test]
    fn minted_tokens_do_not_repeat() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            assert!(seen.insert(mint_token()), "mint_token repeated a value");
        }
    }

    #[test]
    fn constant_time_eq_matches_str_equality() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(constant_time_eq("", ""));
    }
}
