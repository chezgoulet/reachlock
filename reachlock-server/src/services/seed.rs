//! Seed ledger (spec §4): atomic first-write-wins discovery.
//!
//! The in-memory store is the default and mirrors the Postgres semantics
//! exactly: the UNIQUE(universe, system_id) arbiter becomes a single
//! mutex-guarded map insert. The `postgres` feature adds the sqlx-backed
//! store using `INSERT … ON CONFLICT DO NOTHING`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use reachlock_core::seed::types::{Seed, SystemId};
use reachlock_core::universe::UniverseTier;
use serde_json::Value;

/// Result of a discovery attempt. Whatever the store answers IS canonical —
/// the client re-renders from it (spec §4 discovery flow).
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    pub canonical_seed: Seed,
    pub diffs: Value,
    /// True when the caller's tentative seed won the race.
    pub you_discovered: bool,
}

pub trait SeedStore: Send + Sync {
    /// First-write-wins: if (universe, system) has no seed, the tentative
    /// seed becomes canonical and `you_discovered` is true. Otherwise the
    /// existing canonical entry is returned untouched.
    fn discover(&self, universe: UniverseTier, system: &SystemId, tentative: Seed) -> Discovery;

    /// Merge diffs into an existing entry. Returns false if the system has
    /// never been discovered (nothing to modify).
    fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool;
}

#[derive(Default)]
pub struct MemorySeedStore {
    // BTreeMap for deterministic iteration; the mutex is the atomicity
    // arbiter, playing the role of the Postgres UNIQUE constraint.
    entries: Mutex<BTreeMap<(UniverseTier, String), (Seed, Value)>>,
}

impl SeedStore for MemorySeedStore {
    fn discover(&self, universe: UniverseTier, system: &SystemId, tentative: Seed) -> Discovery {
        let mut entries = self.entries.lock().expect("seed store poisoned");
        let key = (universe, system.0.clone());
        match entries.get(&key) {
            Some((seed, diffs)) => Discovery {
                canonical_seed: *seed,
                diffs: diffs.clone(),
                you_discovered: false,
            },
            None => {
                entries.insert(key, (tentative, Value::Object(Default::default())));
                Discovery {
                    canonical_seed: tentative,
                    diffs: Value::Object(Default::default()),
                    you_discovered: true,
                }
            }
        }
    }

    fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool {
        let mut entries = self.entries.lock().expect("seed store poisoned");
        let key = (universe, system.0.clone());
        match entries.get_mut(&key) {
            Some((_, existing)) => {
                merge_diffs(existing, diffs);
                true
            }
            None => false,
        }
    }
}

/// Shallow JSON-object merge: incoming keys overwrite existing ones.
/// Deltas are last-write-wins per key (spec §4 — diffs are player
/// modifications recorded as deltas).
fn merge_diffs(existing: &mut Value, incoming: Value) {
    match (existing, incoming) {
        (Value::Object(base), Value::Object(new)) => {
            for (k, v) in new {
                base.insert(k, v);
            }
        }
        (slot, incoming) => *slot = incoming,
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    //! Postgres-backed seed store. The UNIQUE constraint in
    //! `migrations/0001_init.sql` is the atomic arbiter (spec §4).

    use super::*;
    use sqlx::PgPool;

    pub struct PgSeedStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgSeedStore {
        pub fn new(pool: PgPool) -> Self {
            PgSeedStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl SeedStore for PgSeedStore {
        fn discover(
            &self,
            universe: UniverseTier,
            system: &SystemId,
            tentative: Seed,
        ) -> Discovery {
            let pool = self.pool.clone();
            let system = system.0.clone();
            let tier = universe.as_str();
            let seed_value = tentative.value() as i64;
            self.runtime.block_on(async move {
                // First-write-wins: the INSERT either lands (we discovered)
                // or hits the unique index and returns nothing.
                let inserted: Option<(i64,)> = sqlx::query_as(
                    "INSERT INTO seeds (discoverer_id, universe, system_id, seed)
                     VALUES (gen_random_uuid(), $1::universe_tier, $2, $3)
                     ON CONFLICT (universe, system_id, object_key) DO NOTHING
                     RETURNING seed",
                )
                .bind(tier)
                .bind(&system)
                .bind(seed_value)
                .fetch_optional(&pool)
                .await
                .expect("seed insert failed");

                if let Some((seed,)) = inserted {
                    return Discovery {
                        canonical_seed: Seed::new(seed as u64),
                        diffs: Value::Object(Default::default()),
                        you_discovered: true,
                    };
                }
                let (seed, diffs): (i64, Value) = sqlx::query_as(
                    "SELECT seed, diffs FROM seeds
                     WHERE universe = $1::universe_tier AND system_id = $2
                       AND object_key = ''",
                )
                .bind(tier)
                .bind(&system)
                .fetch_one(&pool)
                .await
                .expect("canonical seed lookup failed");
                Discovery {
                    canonical_seed: Seed::new(seed as u64),
                    diffs,
                    you_discovered: false,
                }
            })
        }

        fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool {
            let pool = self.pool.clone();
            let system = system.0.clone();
            let tier = universe.as_str();
            self.runtime.block_on(async move {
                let result = sqlx::query(
                    "UPDATE seeds SET diffs = diffs || $3, modified = NOW()
                     WHERE universe = $1::universe_tier AND system_id = $2
                       AND object_key = ''",
                )
                .bind(tier)
                .bind(&system)
                .bind(diffs)
                .execute(&pool)
                .await
                .expect("seed modify failed");
                result.rows_affected() > 0
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(name: &str) -> SystemId {
        SystemId(name.into())
    }

    #[test]
    fn first_writer_wins() {
        let store = MemorySeedStore::default();
        let a = store.discover(UniverseTier::Classic, &system("s1"), Seed::new(111));
        let b = store.discover(UniverseTier::Classic, &system("s1"), Seed::new(222));
        assert!(a.you_discovered);
        assert!(!b.you_discovered);
        assert_eq!(
            b.canonical_seed,
            Seed::new(111),
            "loser gets the winner's seed"
        );
    }

    #[test]
    fn universes_are_isolated() {
        let store = MemorySeedStore::default();
        store.discover(UniverseTier::Classic, &system("s1"), Seed::new(111));
        let other = store.discover(UniverseTier::Spectrum, &system("s1"), Seed::new(222));
        assert!(
            other.you_discovered,
            "same system, different universe = separate ledger"
        );
    }

    #[test]
    fn concurrent_discovery_has_exactly_one_winner() {
        use std::sync::Arc;
        let store = Arc::new(MemorySeedStore::default());
        let mut handles = Vec::new();
        for i in 0..32u64 {
            let store = store.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .discover(
                        UniverseTier::FairPlay,
                        &system("contested"),
                        Seed::new(1000 + i),
                    )
                    .you_discovered
            }));
        }
        let winners: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum();
        assert_eq!(winners, 1, "the race must have exactly one winner");
    }

    #[test]
    fn modify_merges_and_requires_discovery() {
        let store = MemorySeedStore::default();
        assert!(!store.modify(
            UniverseTier::Classic,
            &system("nowhere"),
            serde_json::json!({"x": 1})
        ));
        store.discover(UniverseTier::Classic, &system("s1"), Seed::new(1));
        assert!(store.modify(
            UniverseTier::Classic,
            &system("s1"),
            serde_json::json!({"station": "destroyed"})
        ));
        let d = store.discover(UniverseTier::Classic, &system("s1"), Seed::new(9));
        assert_eq!(d.diffs["station"], "destroyed");
    }
}
