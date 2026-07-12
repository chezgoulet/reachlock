//! Dual-target WebSocket transport (S02 deliverable). One API for native
//! and wasm32 via `ewebsock` (no async runtime — wasm32 has no tokio, iron
//! rule #5). Bevy systems only ever call [`WsTransport::poll`], which is
//! non-blocking on both targets: native reads a channel fed by a background
//! OS thread, wasm reads a channel fed by browser `WebSocket` callbacks.

use reachlock_core::network::{ClientMessage, ServerMessage};

/// One event surfaced from the socket this frame.
#[derive(Debug)]
pub enum TransportEvent {
    Opened,
    Message(ServerMessage),
    /// A frame arrived that isn't valid protocol JSON, or was binary/unknown
    /// — the transport itself never panics on garbage from the wire.
    Unparseable(String),
    Error(String),
    Closed,
}

/// Thin wrapper over an `ewebsock` connection.
pub struct WsTransport {
    sender: ewebsock::WsSender,
    receiver: ewebsock::WsReceiver,
}

impl WsTransport {
    /// Opens a connection. Returns immediately on both targets — the actual
    /// handshake happens on a background thread (native) or the browser's
    /// event loop (wasm); watch for [`TransportEvent::Opened`] via
    /// [`Self::poll`].
    pub fn connect(url: &str) -> Result<Self, String> {
        let (sender, receiver) = ewebsock::connect(url, ewebsock::Options::default())?;
        Ok(WsTransport { sender, receiver })
    }

    /// Queues a protocol message for sending. Never blocks.
    pub fn send(&mut self, msg: &ClientMessage) {
        let text = serde_json::to_string(msg).expect("ClientMessage serializes");
        self.sender.send(ewebsock::WsMessage::Text(text));
    }

    /// Drains every event currently buffered. Call once per frame from a
    /// Bevy system; never blocks regardless of target.
    pub fn poll(&mut self) -> Vec<TransportEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.receiver.try_recv() {
            events.push(match event {
                ewebsock::WsEvent::Opened => TransportEvent::Opened,
                ewebsock::WsEvent::Message(ewebsock::WsMessage::Text(text)) => {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(msg) => TransportEvent::Message(msg),
                        Err(e) => {
                            TransportEvent::Unparseable(format!("unparseable frame: {e}"))
                        }
                    }
                }
                ewebsock::WsEvent::Message(_) => {
                    TransportEvent::Unparseable("non-text frame ignored".into())
                }
                ewebsock::WsEvent::Error(e) => TransportEvent::Error(e),
                ewebsock::WsEvent::Closed => TransportEvent::Closed,
            });
        }
        events
    }

    pub fn close(&mut self) {
        self.sender.close();
    }
}

/// Builds the legacy `?player=&universe=` handshake URL (spec §8, S02
/// non-goal: token auth ships in S03 — the server already accepts both, see
/// `reachlock-server/src/ws/session.rs`).
pub fn handshake_url(base: &str, player: &str, universe: reachlock_core::universe::UniverseTier) -> String {
    format!(
        "{base}?player={}&universe={}",
        percent_encode(player),
        universe.as_str()
    )
}

/// Minimal query-value escaping — good enough for player ids, which we
/// either generate ourselves (`pilot-<pid>`) or take verbatim from
/// `REACHLOCK_PLAYER`. Not a general percent-encoder.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::universe::UniverseTier;

    #[test]
    fn handshake_url_shape() {
        let url = handshake_url("ws://127.0.0.1:40711/ws", "boris", UniverseTier::FairPlay);
        assert_eq!(
            url,
            "ws://127.0.0.1:40711/ws?player=boris&universe=fair_play"
        );
    }

    #[test]
    fn percent_encode_escapes_reserved_chars() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
        assert_eq!(percent_encode("pilot-123"), "pilot-123");
    }
}
