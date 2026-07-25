//! Signed evaluation hash chain (spec §6, adversarial finding #4).
//!
//! Online mode: every rule evaluation is hashed over
//! `(contract_id, tick, action, previous_signature)` and sent to the server,
//! which verifies the chain against the canonical contract. A modified
//! client that invents actions breaks the chain and the action is rejected.
//! Offline mode never calls any of this.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::types::Action;

/// Hex SHA-256, 64 chars — fits the ledger's VARCHAR(128).
pub type Signature = String;

/// One signed step in the chain, as transmitted to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEvaluation {
    pub contract_id: String,
    pub tick: u64,
    pub action: Action,
    pub signature: Signature,
    /// Empty string for the first link of a chain.
    #[serde(default)]
    pub prev_signature: Signature,
}

/// Canonical byte encoding of the signed tuple. `Action` params are a
/// BTreeMap, so serde_json emits keys in sorted order — the encoding is
/// deterministic without a custom canonicalizer.
fn canonical_bytes(contract_id: &str, tick: u64, action: &Action, prev: &str) -> Vec<u8> {
    let action_json =
        serde_json::to_vec(action).expect("Action serialization is infallible by construction");
    let mut bytes = Vec::with_capacity(contract_id.len() + 8 + action_json.len() + prev.len() + 3);
    bytes.extend_from_slice(contract_id.as_bytes());
    bytes.push(0x1F);
    bytes.extend_from_slice(&tick.to_le_bytes());
    bytes.push(0x1F);
    bytes.extend_from_slice(&action_json);
    bytes.push(0x1F);
    bytes.extend_from_slice(prev.as_bytes());
    bytes
}

pub fn sign(contract_id: &str, tick: u64, action: &Action, prev_signature: &str) -> Signature {
    let digest = Sha256::digest(canonical_bytes(contract_id, tick, action, prev_signature));
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

/// Verify a single link given the last known signature for this
/// (character, contract) chain.
pub fn verify_link(eval: &SignedEvaluation, last_known: &str) -> Result<(), ChainError> {
    if eval.prev_signature != last_known {
        return Err(ChainError::BrokenChain {
            expected_prev: last_known.to_string(),
            claimed_prev: eval.prev_signature.clone(),
        });
    }
    let expected = sign(
        &eval.contract_id,
        eval.tick,
        &eval.action,
        &eval.prev_signature,
    );
    if eval.signature != expected {
        return Err(ChainError::SignatureMismatch);
    }
    Ok(())
}

/// Verify a full chain from genesis (empty prev). Ticks must strictly
/// increase — replays and reorderings both break verification.
pub fn verify_chain(evals: &[SignedEvaluation]) -> Result<(), ChainError> {
    let mut last_sig = String::new();
    let mut last_tick: Option<u64> = None;
    for eval in evals {
        if let Some(t) = last_tick {
            if eval.tick <= t {
                return Err(ChainError::NonMonotonicTick {
                    prev: t,
                    next: eval.tick,
                });
            }
        }
        verify_link(eval, &last_sig)?;
        last_sig = eval.signature.clone();
        last_tick = Some(eval.tick);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    BrokenChain {
        expected_prev: String,
        claimed_prev: String,
    },
    SignatureMismatch,
    NonMonotonicTick {
        prev: u64,
        next: u64,
    },
}

/// Client-side helper: owns the running chain state for one contract.
#[derive(Debug, Clone, Default)]
pub struct SignatureChain {
    last: Signature,
}

impl SignatureChain {
    pub fn sign_next(&mut self, contract_id: &str, tick: u64, action: &Action) -> SignedEvaluation {
        let signature = sign(contract_id, tick, action, &self.last);
        let eval = SignedEvaluation {
            contract_id: contract_id.to_string(),
            tick,
            action: action.clone(),
            signature: signature.clone(),
            prev_signature: std::mem::take(&mut self.last),
        };
        self.last = signature;
        eval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_of(n: u64) -> Vec<SignedEvaluation> {
        let mut chain = SignatureChain::default();
        (0..n)
            .map(|tick| {
                chain.sign_next(
                    "cryo-pilot",
                    tick * 10 + 1,
                    &Action::verb("maintain_course"),
                )
            })
            .collect()
    }

    #[test]
    fn valid_chain_verifies() {
        assert_eq!(verify_chain(&chain_of(5)), Ok(()));
    }

    #[test]
    fn tampered_action_rejected() {
        let mut evals = chain_of(3);
        evals[1].action = Action::verb("fire_weapons"); // the cheating vector
        assert!(matches!(
            verify_chain(&evals),
            Err(ChainError::SignatureMismatch)
        ));
    }

    #[test]
    fn dropped_link_rejected() {
        let mut evals = chain_of(3);
        evals.remove(1);
        assert!(matches!(
            verify_chain(&evals),
            Err(ChainError::BrokenChain { .. })
        ));
    }

    #[test]
    fn replayed_tick_rejected() {
        let mut evals = chain_of(2);
        let dup = evals[1].clone();
        evals.push(dup);
        assert!(matches!(
            verify_chain(&evals),
            Err(ChainError::NonMonotonicTick { .. })
        ));
    }

    #[test]
    fn deterministic_signature() {
        let a = sign("c", 7, &Action::verb("wake_crew"), "");
        let b = sign("c", 7, &Action::verb("wake_crew"), "");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
