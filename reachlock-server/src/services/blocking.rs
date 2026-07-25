//! Bridging synchronous service traits to async clients (sqlx, redis, lettre).

/// Drive an async database call to completion from a synchronous context.
///
/// The service traits (`SeedStore`, `PlayerStore`, `SessionStore`, `EmailBackend`,
/// …) are sync
/// because the in-memory implementations are, and they are used behind
/// `Box<dyn …>`. The Postgres implementations need async I/O, so they have to
/// block somewhere.
///
/// `Handle::block_on` alone is not enough: it panics with "Cannot start a
/// runtime from within a runtime" whenever it is called on a Tokio worker
/// thread — which is every axum handler and `AppState::new` itself. That is
/// why the Postgres path panicked on startup and would have panicked again on
/// the first authenticated request. The existing tests missed it because they
/// call the stores from `spawn_blocking`, which is not a worker thread.
///
/// `block_in_place` hands this worker's remaining tasks to a sibling thread
/// first, which makes blocking here legal — but it is only available on the
/// multi-threaded runtime, and panics on a current-thread one (which is what
/// `#[tokio::test]` gives you by default). So dispatch on the flavour, and
/// fall back to a plain `block_on` when we are not on a worker at all.
pub fn block_on_async<F: std::future::Future>(
    handle: &tokio::runtime::Handle,
    fut: F,
) -> F::Output {
    use tokio::runtime::RuntimeFlavor;
    match tokio::runtime::Handle::try_current() {
        // On a multi-threaded worker: yield the worker before blocking.
        Ok(current) if current.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(fut))
        }
        // Current-thread runtime, or no runtime context (a `spawn_blocking`
        // thread): blocking directly is already safe.
        _ => handle.block_on(fut),
    }
}
