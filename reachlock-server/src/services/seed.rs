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

#[derive(Debug, Clone, PartialEq)]
struct SeedEntry {
    seed: Seed,
    diffs: Value,
    discoverer_name: Option<String>,
    discovered_at: Option<i64>,
}

/// Result of a discovery attempt. Whatever the store answers IS canonical —
/// the client re-renders from it (spec §4 discovery flow).
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    pub canonical_seed: Seed,
    pub diffs: Value,
    /// True when the caller's tentative seed won the race.
    pub you_discovered: bool,
    /// The player name that first discovered this system.
    pub discoverer_name: Option<String>,
    /// Unix timestamp of the first discovery.
    pub discovered_at: Option<i64>,
}

pub trait SeedStore: Send + Sync {
    /// First-write-wins: if (universe, system) has no seed, the tentative
    /// seed becomes canonical and `you_discovered` is true. Otherwise the
    /// existing canonical entry is returned untouched.
    fn discover(
        &self,
        universe: UniverseTier,
        system: &SystemId,
        tentative: Seed,
        discoverer: Option<&str>,
    ) -> Discovery;

    /// Merge diffs into an existing entry. Returns false if the system has
    /// never been discovered (nothing to modify).
    fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool;
}

#[derive(Default)]
pub struct MemorySeedStore {
    // BTreeMap for deterministic iteration; the mutex is the atomicity
    // arbiter, playing the role of the Postgres UNIQUE constraint.
    entries: Mutex<BTreeMap<(UniverseTier, String), SeedEntry>>,
}

impl SeedStore for MemorySeedStore {
    fn discover(
        &self,
        universe: UniverseTier,
        system: &SystemId,
        tentative: Seed,
        discoverer: Option<&str>,
    ) -> Discovery {
        let mut entries = self.entries.lock().expect("seed store poisoned");
        let key = (universe, system.0.clone());
        match entries.get(&key) {
            Some(entry) => Discovery {
                canonical_seed: entry.seed,
                diffs: entry.diffs.clone(),
                you_discovered: false,
                discoverer_name: entry.discoverer_name.clone(),
                discovered_at: entry.discovered_at,
            },
            None => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as i64);
                entries.insert(
                    key,
                    SeedEntry {
                        seed: tentative,
                        diffs: Value::Object(Default::default()),
                        discoverer_name: discoverer.map(String::from),
                        discovered_at: now,
                    },
                );
                Discovery {
                    canonical_seed: tentative,
                    diffs: Value::Object(Default::default()),
                    you_discovered: true,
                    discoverer_name: discoverer.map(String::from),
                    discovered_at: now,
                }
            }
        }
    }

    fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool {
        let mut entries = self.entries.lock().expect("seed store poisoned");
        let key = (universe, system.0.clone());
        match entries.get_mut(&key) {
            Some(entry) => {
                merge_diffs(&mut entry.diffs, diffs);
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
            discoverer: Option<&str>,
        ) -> Discovery {
            let pool = self.pool.clone();
            let system = system.0.clone();
            let tier = universe.as_str();
            let seed_value = tentative.value() as i64;
            let discoverer_id = discoverer.map(String::from);
            crate::services::blocking::block_on_async(&self.runtime, async move {
                // First-write-wins: INSERT with discoverer_id on first write.
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as i64);
                let inserted: Option<(i64,)> = sqlx::query_as(
                    "INSERT INTO seeds (discoverer_id, universe, system_id, seed)
                     VALUES ($4, $1::universe_tier, $2, $3)
                     ON CONFLICT (universe, system_id, object_key) DO NOTHING
                     RETURNING seed",
                )
                .bind(tier)
                .bind(&system)
                .bind(seed_value)
                .bind(&discoverer_id)
                .fetch_optional(&pool)
                .await
                .expect("seed insert failed");

                if let Some((seed,)) = inserted {
                    return Discovery {
                        canonical_seed: Seed::new(seed as u64),
                        diffs: Value::Object(Default::default()),
                        you_discovered: true,
                        discoverer_name: discoverer_id,
                        discovered_at: now,
                    };
                }
                let (seed, diffs, existing_discoverer, existing_at): (
                    i64,
                    Value,
                    Option<String>,
                    Option<i64>,
                ) = sqlx::query_as(
                    // Discovery carries epoch seconds, not a timestamp type —
                    // extract it in SQL rather than pulling chrono into the
                    // decode path for a single column.
                    "SELECT seed, diffs, discoverer_id,
                            EXTRACT(EPOCH FROM discovered_at)::bigint
                     FROM seeds
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
                    discoverer_name: existing_discoverer,
                    discovered_at: existing_at,
                }
            })
        }

        fn modify(&self, universe: UniverseTier, system: &SystemId, diffs: Value) -> bool {
            let pool = self.pool.clone();
            let system = system.0.clone();
            let tier = universe.as_str();
            crate::services::blocking::block_on_async(&self.runtime, async move {
                let result = sqlx::query(
                    "UPDATE seeds SET diffs = diffs || $3, modified_at = NOW()
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

/// The one seed-store contract, exercised against ANY implementation. Run it
/// against `MemorySeedStore` (below) and, when `REACHLOCK_TEST_DB` is set,
/// against `PgSeedStore` — the whole point is that both stores obey the same
/// first-write-wins semantics. Every scenario uses distinct system ids so the
/// battery is order-independent on a single shared (possibly clean) store.
#[cfg(test)]
pub fn store_contract_tests(store: &dyn SeedStore) {
    use std::thread;

    let system = |name: &str| SystemId(name.into());

    // 1. First writer wins; the loser converges on the winner's seed.
    let a = store.discover(
        UniverseTier::Classic,
        &system("fww-s1"),
        Seed::new(111),
        Some("alice"),
    );
    let b = store.discover(
        UniverseTier::Classic,
        &system("fww-s1"),
        Seed::new(222),
        Some("bob"),
    );
    assert!(a.you_discovered, "first discoverer wins");
    assert!(!b.you_discovered, "second discoverer loses");
    assert_eq!(
        b.canonical_seed,
        Seed::new(111),
        "loser gets the winner's seed"
    );
    assert_eq!(
        b.discoverer_name.as_deref(),
        Some("alice"),
        "loser sees the winner's name"
    );

    // 2. Same system id in a different universe is a separate ledger.
    store.discover(
        UniverseTier::Classic,
        &system("iso-s1"),
        Seed::new(111),
        None,
    );
    let other = store.discover(
        UniverseTier::Spectrum,
        &system("iso-s1"),
        Seed::new(222),
        None,
    );
    assert!(
        other.you_discovered,
        "same system, different universe = separate ledger"
    );

    // 3. 32-way concurrent race: exactly one winner. Against real Postgres
    //    this exercises the UNIQUE(universe, system_id, object_key) index as
    //    the atomic arbiter, not just the in-memory mutex.
    let winners: usize = thread::scope(|scope| {
        let handles: Vec<_> = (0..32u64)
            .map(|i| {
                scope.spawn(move || {
                    store
                        .discover(
                            UniverseTier::FairPlay,
                            &system("race-contested"),
                            Seed::new(1000 + i),
                            None,
                        )
                        .you_discovered as usize
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(winners, 1, "the race must have exactly one winner");

    // 4. modify merges diffs and requires prior discovery.
    assert!(
        !store.modify(
            UniverseTier::Classic,
            &system("mod-nowhere"),
            serde_json::json!({"x": 1})
        ),
        "cannot modify an undiscovered system"
    );
    store.discover(UniverseTier::Classic, &system("mod-s1"), Seed::new(1), None);
    assert!(store.modify(
        UniverseTier::Classic,
        &system("mod-s1"),
        serde_json::json!({"station": "destroyed"})
    ));
    let d = store.discover(UniverseTier::Classic, &system("mod-s1"), Seed::new(9), None);
    assert_eq!(
        d.diffs["station"], "destroyed",
        "diffs merged and persisted"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_obeys_the_contract() {
        store_contract_tests(&MemorySeedStore::default());
    }
}

/// Live-Postgres battery. Skipped (passes trivially) unless `REACHLOCK_TEST_DB`
/// points at a reachable Postgres — CI's `postgres` job sets it. Runs the
/// shared `store_contract_tests` against a freshly-migrated, truncated DB.
#[cfg(all(test, feature = "postgres"))]
mod pg_tests {
    use super::pg::PgSeedStore;
    use super::store_contract_tests;

    #[tokio::test]
    async fn pg_store_obeys_the_contract() {
        let Ok(url) = std::env::var("REACHLOCK_TEST_DB") else {
            eprintln!("REACHLOCK_TEST_DB unset — skipping live Postgres seed battery");
            return;
        };
        let pool = sqlx::PgPool::connect(&url)
            .await
            .expect("connect REACHLOCK_TEST_DB");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        sqlx::query("TRUNCATE seeds")
            .execute(&pool)
            .await
            .expect("clean seeds");

        // The store uses block_on internally, so run the (sync) battery on a
        // blocking thread — never on this async worker.
        let store = PgSeedStore::new(pool);
        tokio::task::spawn_blocking(move || store_contract_tests(&store))
            .await
            .expect("pg battery task");
    }
}
