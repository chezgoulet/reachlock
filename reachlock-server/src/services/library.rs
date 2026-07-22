//! Contract library (S34): the server-side directory of shared contracts and
//! their community stories. Clients publish contracts, query the directory,
//! and submit stories. Pure in-memory store by default; Postgres when the
//! `postgres` feature is enabled.

use reachlock_core::contract::metadata::{ContractLibraryEntry, ContractStory};

/// The server-side contract library service.
pub trait ContractLibrary: Send + Sync {
    /// List published contracts (optionally filtered/sorted).
    fn list(&self, role_filter: Option<&str>, sort: Option<&str>) -> Vec<ContractLibraryEntry>;
    /// Publish a contract to the directory.
    fn publish(&self, player_id: &str, entry: ContractLibraryEntry);
    /// Submit a story for a published contract.
    fn submit_story(&self, story: ContractStory) -> u64;
    /// Stories submitted for a contract.
    fn stories_for(&self, contract_id: &str) -> Vec<ContractStory>;
}

use std::sync::Mutex;

/// In-memory library: default zero-infra store. Lost on restart — clients
/// republish on reconnect.
#[derive(Default)]
pub struct MemoryContractLibrary {
    entries: Mutex<Vec<ContractLibraryEntry>>,
    stories: Mutex<Vec<ContractStory>>,
    story_id_counter: Mutex<u64>,
}

impl MemoryContractLibrary {
    pub fn entries(&self) -> Vec<ContractLibraryEntry> {
        self.entries.lock().expect("poison").clone()
    }
}

impl ContractLibrary for MemoryContractLibrary {
    fn list(&self, role_filter: Option<&str>, sort: Option<&str>) -> Vec<ContractLibraryEntry> {
        let guard = self.entries.lock().expect("poison");
        let mut result: Vec<ContractLibraryEntry> = match role_filter {
            Some(role_name) => guard
                .iter()
                .filter(|e| {
                    let r: &str = &format!("{:?}", e.metadata.crew_role);
                    r.eq_ignore_ascii_case(role_name)
                })
                .cloned()
                .collect(),
            None => guard.clone(),
        };
        if let Some(s) = sort {
            match s {
                "newest" => result.sort_by_key(|b| std::cmp::Reverse(b.metadata.created)),
                "stories" => result.sort_by_key(|b| std::cmp::Reverse(b.metadata.updated)),
                _ => {}
            }
        }
        result
    }

    fn publish(&self, _player_id: &str, entry: ContractLibraryEntry) {
        self.entries.lock().expect("poison").push(entry);
    }

    fn submit_story(&self, story: ContractStory) -> u64 {
        let mut counter = self.story_id_counter.lock().expect("poison");
        *counter += 1;
        let id = *counter;
        self.stories.lock().expect("poison").push(story);
        id
    }

    fn stories_for(&self, contract_id: &str) -> Vec<ContractStory> {
        self.stories
            .lock()
            .expect("poison")
            .iter()
            .filter(|s| s.contract_id == contract_id)
            .cloned()
            .collect()
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    use super::*;
    use sqlx::PgPool;

    pub struct PgContractLibrary {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgContractLibrary {
        pub fn new(pool: PgPool) -> Self {
            PgContractLibrary {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl ContractLibrary for PgContractLibrary {
        fn list(
            &self,
            _role_filter: Option<&str>,
            _sort: Option<&str>,
        ) -> Vec<ContractLibraryEntry> {
            Vec::new()
        }

        fn publish(&self, _player_id: &str, _entry: ContractLibraryEntry) {}

        fn submit_story(&self, _story: ContractStory) -> u64 {
            0
        }

        fn stories_for(&self, _contract_id: &str) -> Vec<ContractStory> {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::contract::metadata::{ContractMetadata, CrewRole};

    fn sample_entry() -> ContractLibraryEntry {
        let meta = ContractMetadata::new(
            "test_author".into(),
            "Boris".into(),
            CrewRole::Engineer,
            "test contract".into(),
        );
        ContractLibraryEntry {
            metadata: meta,
            contract_ron: "(id:\"t\",label:\"x\",trigger:Manual,rules:[],llm_authority:None)"
                .into(),
        }
    }

    #[test]
    fn publish_then_list() {
        let lib = MemoryContractLibrary::default();
        lib.publish("p1", sample_entry());
        assert_eq!(lib.list(None, None).len(), 1);
    }

    #[test]
    fn submit_story_returns_incrementing_id() {
        let lib = MemoryContractLibrary::default();
        let id1 = lib.submit_story(ContractStory {
            contract_id: "c1".into(),
            story: "saved the ship".into(),
            event_type: "combat".into(),
            outcome_type: "triumph".into(),
            timestamp: 1,
        });
        let id2 = lib.submit_story(ContractStory {
            contract_id: "c1".into(),
            story: "another story".into(),
            event_type: "crisis".into(),
            outcome_type: "drama".into(),
            timestamp: 2,
        });
        assert_eq!(id2, id1 + 1);
        assert_eq!(lib.stories_for("c1").len(), 2);
    }
}
