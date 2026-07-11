//! Durable record of accepted evaluations (spec §6). The `VerifyService`
//! keeps chain heads in memory for the hot path; this store lets those heads
//! survive a restart so an in-flight chain isn't broken by a server bounce.
//!
//! Only ACCEPTED evaluations are recorded (`verified = true`). A rejected eval
//! never advances the head, so it is never persisted.

use reachlock_core::contract::signature::SignedEvaluation;
use reachlock_core::universe::UniverseTier;

/// The head of one `(player, contract)` chain, as reloaded on boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRecord {
    pub player_id: String,
    pub contract_id: String,
    pub signature: String,
    pub tick: u64,
}

/// Append-only log of accepted evaluations, plus a boot-time "latest head per
/// chain" query. Universe is carried so the Postgres store can resolve the
/// player row; verification itself is per-`(player, contract)` (universe
/// agnostic), matching the in-memory `VerifyService`.
pub trait EvalStore: Send + Sync {
    fn record_accepted(&self, player_id: &str, universe: UniverseTier, eval: &SignedEvaluation);
    fn load_heads(&self) -> Vec<HeadRecord>;
}

use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory eval log. Exists mainly to exercise the reload path without a
/// live Postgres: submit evals, drop the `VerifyService`, rebuild it from
/// `load_heads`, and the chain still verifies (see verify.rs tests).
#[derive(Default)]
pub struct MemoryEvalStore {
    // (player_id, contract_id) -> head. We only ever need the latest link, so
    // last-write-wins on accepted evals is exactly the head.
    heads: Mutex<HashMap<(String, String), (String, u64)>>,
}

impl EvalStore for MemoryEvalStore {
    fn record_accepted(&self, player_id: &str, _universe: UniverseTier, eval: &SignedEvaluation) {
        self.heads.lock().expect("eval store poisoned").insert(
            (player_id.to_string(), eval.contract_id.clone()),
            (eval.signature.clone(), eval.tick),
        );
    }

    fn load_heads(&self) -> Vec<HeadRecord> {
        self.heads
            .lock()
            .expect("eval store poisoned")
            .iter()
            .map(|((player_id, contract_id), (signature, tick))| HeadRecord {
                player_id: player_id.clone(),
                contract_id: contract_id.clone(),
                signature: signature.clone(),
                tick: *tick,
            })
            .collect()
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    //! Postgres-backed eval log. Rows land in `eval_signatures` keyed on
    //! `players(id)` (upserted by username). On boot, the head of each chain
    //! is the row with the greatest tick per `(player, contract)`.

    use super::*;
    use sqlx::PgPool;

    pub struct PgEvalStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgEvalStore {
        pub fn new(pool: PgPool) -> Self {
            PgEvalStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl EvalStore for PgEvalStore {
        fn record_accepted(
            &self,
            player_id: &str,
            _universe: UniverseTier,
            eval: &SignedEvaluation,
        ) {
            let pool = self.pool.clone();
            let username = player_id.to_string();
            let eval = eval.clone();
            // block_on is safe: only ever called from a `spawn_blocking` task
            // (VerifyService::submit is dispatched off the async worker).
            self.runtime.block_on(async move {
                // Upsert the player then insert the accepted signature in one
                // statement so we never pull the UUID into Rust.
                sqlx::query(
                    "WITH p AS (
                         INSERT INTO players (username) VALUES ($1)
                         ON CONFLICT (username) DO UPDATE SET last_login = NOW()
                         RETURNING id
                     )
                     INSERT INTO eval_signatures
                         (player_id, contract_id, tick, action, signature, prev_signature, verified)
                     SELECT p.id, $2, $3, $4, $5, $6, true FROM p",
                )
                .bind(&username)
                .bind(&eval.contract_id)
                .bind(eval.tick as i64)
                .bind(serde_json::to_value(&eval.action).expect("action serializes"))
                .bind(&eval.signature)
                .bind(&eval.prev_signature)
                .execute(&pool)
                .await
                .expect("record accepted eval");
            });
        }

        fn load_heads(&self) -> Vec<HeadRecord> {
            let pool = self.pool.clone();
            self.runtime.block_on(async move {
                let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
                    "SELECT DISTINCT ON (e.player_id, e.contract_id)
                            p.username, e.contract_id, e.signature, e.tick
                     FROM eval_signatures e
                     JOIN players p ON p.id = e.player_id
                     WHERE e.verified = true
                     ORDER BY e.player_id, e.contract_id, e.tick DESC",
                )
                .fetch_all(&pool)
                .await
                .expect("load chain heads");
                rows.into_iter()
                    .map(|(player_id, contract_id, signature, tick)| HeadRecord {
                        player_id,
                        contract_id,
                        signature,
                        tick: tick as u64,
                    })
                    .collect()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::contract::signature::SignatureChain;
    use reachlock_core::contract::types::Action;

    #[test]
    fn records_head_and_reloads_latest() {
        let store = MemoryEvalStore::default();
        let mut chain = SignatureChain::default();
        for tick in 1..=3 {
            let eval = chain.sign_next("cryo-pilot", tick, &Action::verb("hold"));
            store.record_accepted("boris", UniverseTier::FairPlay, &eval);
        }
        let heads = store.load_heads();
        assert_eq!(heads.len(), 1, "one chain, one head");
        assert_eq!(heads[0].player_id, "boris");
        assert_eq!(heads[0].tick, 3, "head is the latest accepted tick");
    }

    #[test]
    fn distinct_chains_have_distinct_heads() {
        let store = MemoryEvalStore::default();
        let mut a = SignatureChain::default();
        let mut b = SignatureChain::default();
        store.record_accepted(
            "boris",
            UniverseTier::Classic,
            &a.sign_next("c1", 1, &Action::verb("x")),
        );
        store.record_accepted(
            "vex",
            UniverseTier::Classic,
            &b.sign_next("c2", 1, &Action::verb("y")),
        );
        assert_eq!(store.load_heads().len(), 2);
    }
}
