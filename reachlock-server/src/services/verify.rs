//! Verification service (spec §6, adversarial finding #4): validates signed
//! evaluation chains. Stateless beyond the last-known signature per
//! (player, contract) — Redis-backed later, in-memory now.

use std::collections::HashMap;
use std::sync::Mutex;

use reachlock_core::contract::signature::{verify_link, ChainError, SignedEvaluation};

#[derive(Default)]
pub struct VerifyService {
    last: Mutex<HashMap<(String, String), LastLink>>,
}

struct LastLink {
    signature: String,
    tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    Rejected(String),
}

impl VerifyService {
    /// Verify one evaluation for a player. On acceptance the chain head
    /// advances; on rejection it does not (the client must resync).
    pub fn submit(&self, player_id: &str, eval: &SignedEvaluation) -> Verdict {
        let mut last = self.last.lock().expect("verify state poisoned");
        let key = (player_id.to_string(), eval.contract_id.clone());

        let (known_sig, known_tick) = match last.get(&key) {
            Some(link) => (link.signature.clone(), Some(link.tick)),
            None => (String::new(), None),
        };

        if let Some(t) = known_tick {
            if eval.tick <= t {
                return Verdict::Rejected(format!("non_monotonic_tick: {} <= {t}", eval.tick));
            }
        }

        match verify_link(eval, &known_sig) {
            Ok(()) => {
                last.insert(
                    key,
                    LastLink {
                        signature: eval.signature.clone(),
                        tick: eval.tick,
                    },
                );
                Verdict::Accepted
            }
            Err(ChainError::BrokenChain { .. }) => {
                Verdict::Rejected("signature_chain_broken".into())
            }
            Err(ChainError::SignatureMismatch) => Verdict::Rejected("signature_mismatch".into()),
            Err(ChainError::NonMonotonicTick { prev, next }) => {
                Verdict::Rejected(format!("non_monotonic_tick: {next} <= {prev}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::contract::signature::SignatureChain;
    use reachlock_core::contract::types::Action;

    #[test]
    fn honest_chain_accepted() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        for tick in 1..=5 {
            let eval = chain.sign_next("cryo-pilot", tick, &Action::verb("maintain_course"));
            assert_eq!(service.submit("boris", &eval), Verdict::Accepted);
        }
    }

    #[test]
    fn forged_action_rejected_and_chain_head_unmoved() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        let honest = chain.sign_next("guns", 1, &Action::verb("hold_fire"));
        assert_eq!(service.submit("vex", &honest), Verdict::Accepted);

        // The cheat: same chain position, different action, stale signature.
        let mut forged = chain.sign_next("guns", 2, &Action::verb("hold_fire"));
        forged.action = Action::verb("fire_weapons");
        assert!(matches!(
            service.submit("vex", &forged),
            Verdict::Rejected(_)
        ));

        // The honest continuation still verifies: rejection didn't advance.
        let mut chain2 = SignatureChain::default();
        let _ = chain2.sign_next("guns", 1, &Action::verb("hold_fire"));
        let honest2 = chain2.sign_next("guns", 2, &Action::verb("hold_fire"));
        assert_eq!(service.submit("vex", &honest2), Verdict::Accepted);
    }

    #[test]
    fn players_have_independent_chains() {
        let service = VerifyService::default();
        let mut a = SignatureChain::default();
        let mut b = SignatureChain::default();
        assert_eq!(
            service.submit("a", &a.sign_next("c", 1, &Action::verb("x"))),
            Verdict::Accepted
        );
        assert_eq!(
            service.submit("b", &b.sign_next("c", 1, &Action::verb("x"))),
            Verdict::Accepted
        );
    }

    #[test]
    fn replay_rejected() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        let eval = chain.sign_next("c", 1, &Action::verb("x"));
        assert_eq!(service.submit("p", &eval), Verdict::Accepted);
        assert!(matches!(service.submit("p", &eval), Verdict::Rejected(_)));
    }
}
