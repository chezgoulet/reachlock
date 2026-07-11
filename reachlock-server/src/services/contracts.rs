//! Server-side contract backup (spec §6). When a client sends `contract.sync`
//! we persist the player's contract set so it survives device loss. This is a
//! backup, not an authority: the client remains the source of truth and the
//! server never evaluates these — it only verifies signed chains (see
//! `verify.rs`).

use reachlock_core::contract::types::Contract;

/// Persists the latest `contract.sync` payload per player. `sync` replaces the
/// player's stored set wholesale (last-write-wins), matching the client's own
/// "here is my current contract book" semantics.
pub trait ContractStore: Send + Sync {
    fn sync(&self, player_id: &str, contracts: &[Contract]);
}

use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory backup: the zero-infra default. Lost on restart, which is fine —
/// the client re-syncs on reconnect.
#[derive(Default)]
pub struct MemoryContractStore {
    by_player: Mutex<HashMap<String, Vec<Contract>>>,
}

impl MemoryContractStore {
    /// Test/inspection hook: what we currently hold for a player.
    pub fn get(&self, player_id: &str) -> Vec<Contract> {
        self.by_player
            .lock()
            .expect("contract store poisoned")
            .get(player_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl ContractStore for MemoryContractStore {
    fn sync(&self, player_id: &str, contracts: &[Contract]) {
        self.by_player
            .lock()
            .expect("contract store poisoned")
            .insert(player_id.to_string(), contracts.to_vec());
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    //! Postgres-backed contract backup. Keyed on `players(id)` (upserted by
    //! username): in the online ledger, contract backup is per-player. The
    //! per-character split arrives with the landed/on-board sprints (S06+),
    //! where characters become real entities.

    use super::*;
    use sqlx::types::Uuid;
    use sqlx::PgPool;

    pub struct PgContractStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgContractStore {
        pub fn new(pool: PgPool) -> Self {
            PgContractStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl ContractStore for PgContractStore {
        fn sync(&self, player_id: &str, contracts: &[Contract]) {
            let pool = self.pool.clone();
            let username = player_id.to_string();
            let contracts = contracts.to_vec();
            // block_on is safe here: this trait is only invoked from a
            // `spawn_blocking` task in the WS handler, never on an async
            // worker thread (see handler.rs).
            self.runtime.block_on(async move {
                let mut tx = pool.begin().await.expect("begin contract sync tx");
                let (pid,): (Uuid,) = sqlx::query_as(
                    "INSERT INTO players (username) VALUES ($1)
                     ON CONFLICT (username) DO UPDATE SET last_login = NOW()
                     RETURNING id",
                )
                .bind(&username)
                .fetch_one(&mut *tx)
                .await
                .expect("upsert player");

                // Wholesale replace: the sync payload is the player's full book.
                sqlx::query("DELETE FROM contracts WHERE player_id = $1")
                    .bind(pid)
                    .execute(&mut *tx)
                    .await
                    .expect("clear old contracts");

                for contract in &contracts {
                    sqlx::query(
                        "INSERT INTO contracts (player_id, label, contract)
                         VALUES ($1, $2, $3)",
                    )
                    .bind(pid)
                    .bind(&contract.label)
                    .bind(serde_json::to_value(contract).expect("contract serializes"))
                    .execute(&mut *tx)
                    .await
                    .expect("insert contract");
                }
                tx.commit().await.expect("commit contract sync");
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::contract::types::{Contract, Trigger};

    fn contract(id: &str) -> Contract {
        Contract {
            id: id.into(),
            label: format!("label-{id}"),
            trigger: Trigger::Manual,
            rules: vec![],
            llm_authority: None,
        }
    }

    #[test]
    fn sync_stores_and_replaces() {
        let store = MemoryContractStore::default();
        store.sync("boris", &[contract("a"), contract("b")]);
        assert_eq!(store.get("boris").len(), 2);
        // Last-write-wins: a smaller book replaces the larger one.
        store.sync("boris", &[contract("c")]);
        let held = store.get("boris");
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].id, "c");
    }

    #[test]
    fn players_are_isolated() {
        let store = MemoryContractStore::default();
        store.sync("a", &[contract("x")]);
        assert!(store.get("b").is_empty());
    }
}
