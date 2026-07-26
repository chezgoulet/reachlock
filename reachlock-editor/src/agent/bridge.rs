//! UI bridge (S101 P4).
//!
//! `Box<dyn Editor>` lives on the UI thread and is not `Send` — several tabs
//! hold egui texture handles. The agent loop runs on its own thread because it
//! owns blocking HTTP. So a tool that touches a live tab cannot call the
//! editor; it posts a typed request and blocks for the reply.
//!
//! ```text
//!   agent thread                    UI thread
//!   ------------                    ---------
//!   dispatch(write_document)
//!     └─ send(SessionRequest) ──▶   drain_session_requests()  (every frame)
//!        recv_timeout(...)   ◀───     execute against open_editors, reply
//! ```
//!
//! Two rules keep that from deadlocking, and both are load-bearing:
//!
//! 1. **The UI thread drains the queue every frame, whatever else it is
//!    doing.** If draining were skipped while a modal dialog was up, an agent
//!    blocked on a reply would hang until the author happened to dismiss it —
//!    and the author would be looking at an editor that appears frozen.
//! 2. **The agent side always waits with a timeout.** If the UI thread dies,
//!    or a request is dropped, the tool reports a timeout the model can read
//!    instead of blocking the agent thread forever.

use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use super::tools::ToolOutcome;

/// How long a tool waits for the UI thread before giving up.
///
/// Generous: the UI thread may be mid-frame on a large content tree. Short
/// enough that a genuinely wedged UI surfaces as an error in one turn rather
/// than looking like a hung model.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// A typed operation against the live editor.
///
/// Typed rather than a raw JSON blob so the UI-side executor is an exhaustive
/// match: adding a session tool without teaching the executor about it becomes
/// a compile error instead of a runtime "unknown request".
#[derive(Debug, Clone, PartialEq)]
pub enum SessionOp {
    ListTabs,
    /// Open a content file in the tab that handles its type.
    OpenTab {
        path: String,
    },
    /// The active document as RON, via `Editor::snapshot`.
    ReadDocument,
    /// Replace the active document, via `Editor::restore_snapshot`.
    WriteDocument {
        ron: String,
    },
    /// Structural + cross-reference validation of the active tab.
    Validate,
    /// Write every dirty entry back to its own file.
    SaveAll,
}

/// One request in flight, with the channel its answer goes back on.
pub struct SessionRequest {
    pub op: SessionOp,
    reply: Sender<ToolOutcome>,
}

impl SessionRequest {
    /// Answer the request. Dropping a `SessionRequest` without calling this
    /// leaves the agent waiting for the full timeout, so the executor must
    /// answer every request it takes — including ones it rejects.
    pub fn reply(self, outcome: ToolOutcome) {
        // A send error means the agent gave up (timeout) or went away. Not a
        // failure worth surfacing on the UI thread.
        let _ = self.reply.send(outcome);
    }
}

/// The agent-thread half. Cloneable and `Send`, so it can live in the tool
/// context.
#[derive(Clone)]
pub struct SessionHandle {
    tx: Sender<SessionRequest>,
}

impl SessionHandle {
    /// Post an operation and wait for the UI thread to run it.
    pub fn run(&self, op: SessionOp) -> ToolOutcome {
        let (reply_tx, reply_rx) = channel();
        if self
            .tx
            .send(SessionRequest {
                op,
                reply: reply_tx,
            })
            .is_err()
        {
            return ToolOutcome::error(
                "The editor is no longer running, so this tool cannot reach a tab.",
            );
        }
        match reply_rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => ToolOutcome::error(
                "The editor did not respond within 10 seconds. It may be busy or \
                 waiting on a dialog. Try again, or ask the author to check the window.",
            ),
            Err(RecvTimeoutError::Disconnected) => {
                ToolOutcome::error("The editor closed while this tool was running.")
            }
        }
    }
}

/// The UI-thread half. Lives on `EditorApp` and is drained once per frame.
pub struct SessionQueue {
    rx: Receiver<SessionRequest>,
    /// Kept so [`Self::handle`] can hand out clones after construction.
    #[allow(dead_code)]
    handle: SessionHandle,
}

impl SessionQueue {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        SessionQueue {
            rx,
            handle: SessionHandle { tx },
        }
    }

    /// A handle to give the agent thread. Consumed when the loop is spawned
    /// (P5); until then only the tests build one.
    #[allow(dead_code)]
    pub fn handle(&self) -> SessionHandle {
        self.handle.clone()
    }

    /// Take everything queued right now. Never blocks — this runs inside the
    /// frame callback.
    pub fn drain(&self) -> Vec<SessionRequest> {
        self.rx.try_iter().collect()
    }
}

impl Default for SessionQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips() {
        let queue = SessionQueue::new();
        let handle = queue.handle();
        let agent = std::thread::spawn(move || handle.run(SessionOp::ListTabs));

        // Stand in for the UI thread's per-frame drain.
        let mut served = false;
        while !served {
            for req in queue.drain() {
                assert_eq!(req.op, SessionOp::ListTabs);
                req.reply(ToolOutcome::ok("one tab"));
                served = true;
            }
            std::thread::yield_now();
        }
        let out = agent.join().unwrap();
        assert!(!out.is_error);
        assert_eq!(out.content, "one tab");
    }

    /// If the editor is gone, a tool must say so rather than block the agent
    /// thread until the timeout.
    #[test]
    fn a_dropped_queue_is_reported_immediately() {
        let queue = SessionQueue::new();
        let handle = queue.handle();
        drop(queue);
        let out = handle.run(SessionOp::ListTabs);
        assert!(out.is_error);
        assert!(out.content.contains("no longer running"), "{}", out.content);
    }

    /// The executor answering nothing must not wedge the agent for good. This
    /// is the timeout path, exercised with the real (10s) timeout only in
    /// spirit — the assertion here is that dropping the request without
    /// replying disconnects the channel and returns promptly.
    #[test]
    fn dropping_a_request_without_replying_does_not_hang() {
        let queue = SessionQueue::new();
        let handle = queue.handle();
        let agent = std::thread::spawn(move || handle.run(SessionOp::ReadDocument));

        loop {
            let reqs = queue.drain();
            if !reqs.is_empty() {
                // Drop without replying — the bug this guards against.
                drop(reqs);
                break;
            }
            std::thread::yield_now();
        }
        let out = agent.join().unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("closed"), "{}", out.content);
    }
}
