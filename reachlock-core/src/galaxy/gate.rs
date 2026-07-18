use serde::{Deserialize, Serialize};

use crate::seed::types::SystemId;

/// Gate status encodes who can use a gate, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// Open for everyone.
    Active,
    /// Sealed by the controlling faction — no transit, no exceptions.
    Blockaded,
    /// Only players with sufficient faction reputation may use it.
    Restricted,
    /// Active combat around the gate — may transit but a damage roll applies.
    Contested,
    /// Gate is physically gone (or never existed). Cannot transit.
    Destroyed,
}

/// One edge in the gate network graph: a directed connection between two
/// charted systems. Gates are one-way so a route `from→to` and `to→from`
/// are separate entries (they can have different statuses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
    pub from: SystemId,
    pub to: SystemId,
    pub status: GateStatus,
    /// Faction that controls this gate, if any (for Blockaded/Restricted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_by: Option<String>,
}

/// The authored gate network graph for charted space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateNetwork {
    pub gates: Vec<Gate>,
}

impl GateNetwork {
    /// All outgoing gates from a charted system.
    pub fn outgoing(&self, from: &SystemId) -> Vec<&Gate> {
        self.gates.iter().filter(|g| &g.from == from).collect()
    }

    /// Find a specific gate by origin and index.
    #[allow(dead_code)]
    pub fn gate_by_index(&self, from: &SystemId, index: usize) -> Option<&Gate> {
        self.outgoing(from).into_iter().nth(index)
    }

    /// Validate the network: no self-loops, every destination is reachable
    /// (has at least one outgoing gate).
    pub fn validate(&self) -> Result<(), String> {
        let sources: std::collections::HashSet<&str> =
            self.gates.iter().map(|g| g.from.0.as_str()).collect();
        for gate in &self.gates {
            if gate.from == gate.to {
                return Err(format!("self-loop gate: {} → {}", gate.from.0, gate.to.0));
            }
            if !sources.contains(gate.to.0.as_str()) {
                return Err(format!(
                    "destination {} has no outgoing gate (dead end)",
                    gate.to.0
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(s: &str) -> SystemId {
        SystemId(s.into())
    }

    fn gate(from: &str, to: &str) -> Gate {
        Gate {
            from: sid(from),
            to: sid(to),
            status: GateStatus::Active,
            controlled_by: None,
        }
    }

    #[test]
    fn validates_no_self_loop() {
        let net = GateNetwork {
            gates: vec![gate("a", "a")],
        };
        assert!(net.validate().unwrap_err().contains("self-loop"));
    }

    #[test]
    fn validates_no_dead_end() {
        // "b" is reachable from "a", but "b" has no outgoing gate.
        let net = GateNetwork {
            gates: vec![gate("a", "b")],
        };
        assert!(net.validate().unwrap_err().contains("dead end"));
    }

    #[test]
    fn valid_network_passes() {
        let net = GateNetwork {
            gates: vec![
                gate("a", "b"),
                gate("b", "a"),
                gate("b", "c"),
                gate("c", "b"),
            ],
        };
        assert!(net.validate().is_ok());
    }

    #[test]
    fn outgoing_returns_correct_gates() {
        let net = GateNetwork {
            gates: vec![gate("a", "b"), gate("a", "c"), gate("b", "a")],
        };
        let out = net.outgoing(&sid("a"));
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|g| g.to == sid("b")));
        assert!(out.iter().any(|g| g.to == sid("c")));
    }

    #[test]
    fn round_trips_through_json() {
        let net = GateNetwork {
            gates: vec![gate("a", "b"), gate("b", "a")],
        };
        let json = serde_json::to_string(&net).unwrap();
        let back: GateNetwork = serde_json::from_str(&json).unwrap();
        assert_eq!(net, back);
    }
}
