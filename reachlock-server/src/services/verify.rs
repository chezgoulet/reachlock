//! Verification service (spec §6, adversarial finding #4): validates signed
//! evaluation chains. The chain head per (player, contract) is held in memory
//! for the hot path and, when an `EvalStore` is attached, mirrored to durable
//! storage so a restart doesn't break an in-flight chain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reachlock_core::contract::signature::{verify_link, ChainError, SignedEvaluation};
use reachlock_core::universe::UniverseTier;

use super::eval::{EvalStore, HeadRecord};

#[derive(Default)]
pub struct VerifyService {
    last: Mutex<HashMap<(String, String), LastLink>>,
    /// Durable backing (Postgres) for accepted evals. `None` = memory-only
    /// (the zero-infra default); heads then live only for this process.
    store: Option<Arc<dyn EvalStore>>,
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
    /// Build a service backed by `store`, seeded with chain heads already
    /// reloaded from it. The caller loads `heads` off the async worker (see
    /// `AppState::connect`) so this constructor stays non-blocking.
    pub fn with_heads(store: Option<Arc<dyn EvalStore>>, heads: Vec<HeadRecord>) -> Self {
        let mut last = HashMap::new();
        for h in heads {
            last.insert(
                (h.player_id, h.contract_id),
                LastLink {
                    signature: h.signature,
                    tick: h.tick,
                },
            );
        }
        VerifyService {
            last: Mutex::new(last),
            store,
        }
    }

    /// Verify one evaluation for a player. On acceptance the chain head
    /// advances (and is persisted if a store is attached); on rejection it
    /// does not (the client must resync).
    ///
    /// NOTE: when backed by a Postgres `EvalStore`, this performs a blocking
    /// DB write, so it must be dispatched via `spawn_blocking` from the async
    /// WS handler — never called directly on an async worker thread.
    pub fn submit(
        &self,
        player_id: &str,
        universe: UniverseTier,
        eval: &SignedEvaluation,
    ) -> Verdict {
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
                // Persist only accepted evals; drop the lock first so a slow
                // DB write never blocks other players' verification.
                drop(last);
                if let Some(store) = &self.store {
                    store.record_accepted(player_id, universe, eval);
                }
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
    use crate::services::eval::MemoryEvalStore;
    use reachlock_core::contract::signature::SignatureChain;
    use reachlock_core::contract::types::Action;

    const U: UniverseTier = UniverseTier::Classic;

    #[test]
    fn honest_chain_accepted() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        for tick in 1..=5 {
            let eval = chain.sign_next("cryo-pilot", tick, &Action::verb("maintain_course"));
            assert_eq!(service.submit("boris", U, &eval), Verdict::Accepted);
        }
    }

    #[test]
    fn forged_action_rejected_and_chain_head_unmoved() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        let honest = chain.sign_next("guns", 1, &Action::verb("hold_fire"));
        assert_eq!(service.submit("vex", U, &honest), Verdict::Accepted);

        // The cheat: same chain position, different action, stale signature.
        let mut forged = chain.sign_next("guns", 2, &Action::verb("hold_fire"));
        forged.action = Action::verb("fire_weapons");
        assert!(matches!(
            service.submit("vex", U, &forged),
            Verdict::Rejected(_)
        ));

        // The honest continuation still verifies: rejection didn't advance.
        let mut chain2 = SignatureChain::default();
        let _ = chain2.sign_next("guns", 1, &Action::verb("hold_fire"));
        let honest2 = chain2.sign_next("guns", 2, &Action::verb("hold_fire"));
        assert_eq!(service.submit("vex", U, &honest2), Verdict::Accepted);
    }

    #[test]
    fn players_have_independent_chains() {
        let service = VerifyService::default();
        let mut a = SignatureChain::default();
        let mut b = SignatureChain::default();
        assert_eq!(
            service.submit("a", U, &a.sign_next("c", 1, &Action::verb("x"))),
            Verdict::Accepted
        );
        assert_eq!(
            service.submit("b", U, &b.sign_next("c", 1, &Action::verb("x"))),
            Verdict::Accepted
        );
    }

    #[test]
    fn replay_rejected() {
        let service = VerifyService::default();
        let mut chain = SignatureChain::default();
        let eval = chain.sign_next("c", 1, &Action::verb("x"));
        assert_eq!(service.submit("p", U, &eval), Verdict::Accepted);
        assert!(matches!(
            service.submit("p", U, &eval),
            Verdict::Rejected(_)
        ));
    }

    #[test]
    fn heads_survive_a_restart() {
        // The acceptance gate, in memory: submit a partial chain, "restart"
        // (drop the service, keep the store), and the next link still verifies.
        let store = Arc::new(MemoryEvalStore::default());
        let mut chain = SignatureChain::default();

        let before = VerifyService::with_heads(Some(store.clone()), Vec::new());
        for tick in 1..=3 {
            let eval = chain.sign_next("cryo-pilot", tick, &Action::verb("maintain_course"));
            assert_eq!(before.submit("boris", U, &eval), Verdict::Accepted);
        }
        drop(before); // server bounce

        // Rebuild from the store, exactly as AppState::connect does on boot.
        let after = VerifyService::with_heads(Some(store.clone()), store.load_heads());
        let next = chain.sign_next("cryo-pilot", 4, &Action::verb("maintain_course"));
        assert_eq!(
            after.submit("boris", U, &next),
            Verdict::Accepted,
            "the chain continues across a restart"
        );

        // And a replay of an old link is still rejected after reload.
        let mut replay_chain = SignatureChain::default();
        let stale = replay_chain.sign_next("cryo-pilot", 1, &Action::verb("maintain_course"));
        assert!(matches!(
            after.submit("boris", U, &stale),
            Verdict::Rejected(_)
        ));
    }
}
