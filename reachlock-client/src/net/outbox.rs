//! Outbound message queue bridging non-network systems (contract
//! evaluation) to the transport (`systems::network`). Kept as plain data so
//! `systems::contract` never has to know a socket exists — it just queues
//! protocol messages when `NetMode::Online`; `systems::network` drains the
//! queue once actually `Connected`. If the socket isn't up yet, queued
//! messages simply wait for the next successful poll.

use std::collections::VecDeque;

use bevy::prelude::*;
use reachlock_core::network::ClientMessage;

#[derive(Resource, Default)]
pub struct NetOutbox(VecDeque<ClientMessage>);

impl NetOutbox {
    pub fn push(&mut self, msg: ClientMessage) {
        self.0.push_back(msg);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = ClientMessage> + '_ {
        self.0.drain(..)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::seed::types::SystemId;

    #[test]
    fn push_then_drain_preserves_order_and_empties() {
        let mut outbox = NetOutbox::default();
        assert!(outbox.is_empty());
        outbox.push(ClientMessage::PlayerPosition {
            system_id: SystemId("a".into()),
            position: [1, 2, 0],
        });
        outbox.push(ClientMessage::PlayerPosition {
            system_id: SystemId("b".into()),
            position: [3, 4, 0],
        });
        assert_eq!(outbox.len(), 2);
        let drained: Vec<_> = outbox.drain().collect();
        assert_eq!(drained.len(), 2);
        assert!(outbox.is_empty());
    }
}
