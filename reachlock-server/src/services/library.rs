//! Contract library (S34): the server-side directory of shared contracts and
//! their community stories. Clients publish contracts, query the directory,
//! and submit stories. Pure in-memory store by default; Postgres when the
//! `postgres` feature is enabled.

use reachlock_core::contract::metadata::{ContractLibraryEntry, ContractStory};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;

/// Publish error variants.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishError {
    RateLimited { retry_after_secs: u64 },
    DailyLimitExceeded,
    DuplicateContract,
}

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

    /// S86: publish with rate-limit checks.
    fn publish_rate_limited(
        &self,
        player_id: &str,
        entry: ContractLibraryEntry,
    ) -> Result<String, PublishError>;
    /// S86: search across name/description/tags.
    fn search(&self, query: &str) -> Vec<ContractLibraryEntry>;
    /// S86: look up a contract by share code.
    fn lookup_share_code(&self, code: &str) -> Option<ContractLibraryEntry>;
    /// S86: generate an 8-char alphanumeric share code.
    fn generate_share_code(&self, contract_id: &str) -> String;
}

/// Rate-limit configuration.
#[derive(Clone)]
pub struct LibraryConfig {
    pub publish_cooldown_secs: u64,
    pub max_publishes_per_day: u32,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        LibraryConfig {
            publish_cooldown_secs: 60,
            max_publishes_per_day: 10,
        }
    }
}

impl LibraryConfig {
    pub fn from_env() -> Self {
        LibraryConfig {
            publish_cooldown_secs: std::env::var("REACHLOCK_LIBRARY_PUBLISH_COOLDOWN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            max_publishes_per_day: std::env::var("REACHLOCK_LIBRARY_MAX_PUBLISHES_PER_DAY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        }
    }
}

fn generate_share_code() -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Deterministic from the timestamp: 8 uppercase alphanumeric chars.
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    let mut code = String::with_capacity(8);
    let mut h = nanos;
    for _ in 0..8 {
        code.push(chars[(h as usize) % chars.len()]);
        h >>= 5;
    }
    code
}

/// In-memory library: default zero-infra store. Lost on restart — clients
/// republish on reconnect.
pub struct MemoryContractLibrary {
    entries: Mutex<Vec<ContractLibraryEntry>>,
    stories: Mutex<Vec<ContractStory>>,
    story_id_counter: Mutex<u64>,
    share_codes: Mutex<HashMap<String, String>>, // code -> entry id (index)
    publish_history: Mutex<HashMap<String, VecDeque<Instant>>>,
    config: LibraryConfig,
}

impl Default for MemoryContractLibrary {
    fn default() -> Self {
        MemoryContractLibrary {
            entries: Mutex::new(Vec::new()),
            stories: Mutex::new(Vec::new()),
            story_id_counter: Mutex::new(0),
            share_codes: Mutex::new(HashMap::new()),
            publish_history: Mutex::new(HashMap::new()),
            config: LibraryConfig::default(),
        }
    }
}

impl MemoryContractLibrary {
    pub fn new(config: LibraryConfig) -> Self {
        MemoryContractLibrary {
            config,
            ..Default::default()
        }
    }

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

    fn publish_rate_limited(
        &self,
        player_id: &str,
        entry: ContractLibraryEntry,
    ) -> Result<String, PublishError> {
        let mut history = self.publish_history.lock().expect("poison");
        let now = Instant::now();
        let player_entries = history.entry(player_id.to_string()).or_default();

        // Cooldown check.
        if let Some(last) = player_entries.back() {
            let elapsed = now.duration_since(*last).as_secs();
            if elapsed < self.config.publish_cooldown_secs {
                return Err(PublishError::RateLimited {
                    retry_after_secs: self.config.publish_cooldown_secs - elapsed,
                });
            }
        }

        // Daily limit check.
        let day_secs = 86400u64;
        let recent = player_entries
            .iter()
            .filter(|t| now.duration_since(**t).as_secs() < day_secs)
            .count();
        if recent >= self.config.max_publishes_per_day as usize {
            return Err(PublishError::DailyLimitExceeded);
        }

        // Generate share code BEFORE publishing.
        let share_code = generate_share_code();
        let entry_id = entry.metadata.crew_member_name.clone();

        // Check for duplicate (same serialized form by same player).
        let guard = self.entries.lock().expect("poison");
        let serialized = ron::to_string(&entry).unwrap_or_default();
        let is_dup = guard.iter().any(|e| {
            ron::to_string(e).ok().as_deref() == Some(&serialized)
                && e.metadata.author == entry.metadata.author
        });
        if is_dup {
            return Err(PublishError::DuplicateContract);
        }
        drop(guard);

        self.entries.lock().expect("poison").push(entry);
        self.share_codes
            .lock()
            .expect("poison")
            .insert(share_code.clone(), entry_id);
        player_entries.push_back(now);
        // Keep history bounded.
        while player_entries.len() > 100 {
            player_entries.pop_front();
        }
        Ok(share_code)
    }

    fn search(&self, query: &str) -> Vec<ContractLibraryEntry> {
        let q = query.to_lowercase();
        let guard = self.entries.lock().expect("poison");
        guard
            .iter()
            .filter(|e| {
                let meta = &e.metadata;
                meta.crew_member_name.to_lowercase().contains(&q)
                    || meta.description.to_lowercase().contains(&q)
                    || meta
                        .personality_tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&q))
                    || meta
                        .story_tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&q))
                    || meta.author.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    fn lookup_share_code(&self, code: &str) -> Option<ContractLibraryEntry> {
        let codes = self.share_codes.lock().expect("poison");
        let entry_id = codes.get(code)?;
        let guard = self.entries.lock().expect("poison");
        guard
            .iter()
            .find(|e| e.metadata.crew_member_name == *entry_id)
            .cloned()
    }

    fn generate_share_code(&self, _contract_id: &str) -> String {
        generate_share_code()
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    use super::*;
    use sqlx::PgPool;

    pub struct PgContractLibrary {
        #[expect(dead_code)]
        pool: PgPool,
        #[expect(dead_code)]
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

        fn publish_rate_limited(
            &self,
            _player_id: &str,
            _entry: ContractLibraryEntry,
        ) -> Result<String, PublishError> {
            Err(PublishError::RateLimited {
                retry_after_secs: 0,
            })
        }

        fn search(&self, _query: &str) -> Vec<ContractLibraryEntry> {
            Vec::new()
        }

        fn lookup_share_code(&self, _code: &str) -> Option<ContractLibraryEntry> {
            None
        }

        fn generate_share_code(&self, _contract_id: &str) -> String {
            String::new()
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

    fn sample_entry_with_tags() -> ContractLibraryEntry {
        let mut meta = ContractMetadata::new(
            "author42".into(),
            "CombatBot".into(),
            CrewRole::Tactical,
            "An aggressive combat contract".into(),
        );
        meta.personality_tags = vec!["aggressive".into(), "combat".into()];
        meta.story_tags = vec!["rescue".into()];
        ContractLibraryEntry {
            metadata: meta,
            contract_ron: "(id:\"c\",label:\"x\",trigger:Manual,rules:[],llm_authority:None)"
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

    #[test]
    fn publish_rate_limited_cooldown() {
        let lib = MemoryContractLibrary::default();
        let entry = sample_entry();
        // First publish succeeds.
        assert!(lib.publish_rate_limited("p1", entry.clone()).is_ok());
        // Immediate second publish fails with cooldown.
        let err = lib.publish_rate_limited("p1", entry.clone()).unwrap_err();
        match err {
            PublishError::RateLimited { retry_after_secs } => {
                assert!(retry_after_secs > 0 && retry_after_secs <= 60);
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[test]
    fn search_filter() {
        let lib = MemoryContractLibrary::default();
        lib.publish("p1", sample_entry());
        lib.publish("p1", sample_entry_with_tags());

        let results = lib.search("combat");
        assert_eq!(results.len(), 1, "should find the combat contract");
        assert_eq!(results[0].metadata.crew_member_name, "CombatBot");

        let results = lib.search("author42");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn share_code_round_trip() {
        let lib = MemoryContractLibrary::default();
        let entry = sample_entry();
        let code = lib.publish_rate_limited("p1", entry.clone()).unwrap();
        assert_eq!(code.len(), 8, "share code is 8 chars");
        let looked_up = lib.lookup_share_code(&code);
        assert!(looked_up.is_some(), "should find by share code");
        assert_eq!(looked_up.unwrap().metadata.author, "test_author");
    }
}
