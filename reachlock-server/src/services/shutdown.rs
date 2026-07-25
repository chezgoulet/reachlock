use std::time::Instant;

use tracing::info;

pub trait GracefulShutdown: Send + Sync {
    fn shutdown(&self);
    fn name(&self) -> &str;
}

pub struct ShutdownCoordinator {
    stages: Vec<Box<dyn GracefulShutdown>>,
    timeout: std::time::Duration,
}

impl ShutdownCoordinator {
    pub fn new(timeout_secs: u64) -> Self {
        ShutdownCoordinator {
            stages: Vec::new(),
            timeout: std::time::Duration::from_secs(timeout_secs),
        }
    }

    pub fn register(&mut self, stage: Box<dyn GracefulShutdown>) {
        self.stages.push(stage);
    }

    pub fn shutdown(&self) {
        let deadline = Instant::now() + self.timeout;
        for stage in &self.stages {
            let start = Instant::now();
            if Instant::now() >= deadline {
                info!(name = %stage.name(), "shutdown.stage timed out — skipping");
                continue;
            }
            stage.shutdown();
            let elapsed = start.elapsed();
            info!(
                stage.name = %stage.name(),
                status = "ok",
                duration_ms = elapsed.as_millis() as u64,
                "shutdown.stage"
            );
        }
    }
}

impl GracefulShutdown for Box<dyn crate::services::seed::SeedStore> {
    fn shutdown(&self) {
        info!("SeedStore shutdown — no-op for in-memory");
    }
    fn name(&self) -> &str {
        "seed_store"
    }
}

impl GracefulShutdown for Box<dyn crate::services::auth::SessionStore> {
    fn shutdown(&self) {
        info!("SessionStore shutdown — no-op for in-memory");
    }
    fn name(&self) -> &str {
        "session_store"
    }
}
