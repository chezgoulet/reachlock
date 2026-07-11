//! `resolve()` (spec §10, Priority System; spec §5, Override System): the
//! single function that decides, for one object, whether an authored
//! [`ContentFile`] wins over procedural generation.

use crate::universe::tier::UniverseTier;

use super::envelope::ContentFile;
use super::priority::Priority;

/// Identifies which object `resolve` is deciding for, and the context
/// needed to evaluate priority (universe membership, event expiry).
#[derive(Debug, Clone)]
pub struct SeedParams {
    pub object_id: String,
    pub universe: UniverseTier,
    /// Unix seconds "now" — `Priority::Event` candidates past
    /// `expires_at` are skipped (spec §10, Content Lifecycle).
    pub now: u64,
}

/// The resolver's verdict for one object.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// An authored file wins; render it instead of generating.
    Authored(ContentFile),
    /// No override applies; the caller should generate procedurally.
    Procedural,
}

/// Pick the winning content file for `params.object_id` out of the full
/// local override index, or `Resolved::Procedural` if none applies. This
/// function does the filtering so callers don't reimplement the priority
/// table (spec §10, Priority System):
///
/// - Only files whose `id` matches `object_id` and whose `universe` field
///   matches `params.universe` are candidates.
/// - `Priority::Event` candidates past `expires_at` are skipped ("Event
///   content auto-removes").
/// - Among remaining candidates, the highest `Priority` wins —
///   `Authoritative` (100) always beats `Event` (75) beats `Curated` (50).
/// - No candidates ⇒ `Resolved::Procedural` (the spec's default row: "no
///   authored content exists").
pub fn resolve(overrides: &[ContentFile], params: &SeedParams) -> Resolved {
    overrides
        .iter()
        .filter(|c| c.id == params.object_id)
        .filter(|c| c.matches_universe(params.universe))
        .filter(|c| !is_expired(c, params.now))
        .max_by_key(|c| c.priority)
        .cloned()
        .map(Resolved::Authored)
        .unwrap_or(Resolved::Procedural)
}

fn is_expired(content: &ContentFile, now: u64) -> bool {
    content.priority == Priority::Event && content.expires_at.is_some_and(|exp| now >= exp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::envelope::{AssetType, ContentPayload};
    use crate::generator::GeneratedMesh;

    fn stub(id: &str, priority: Priority, universe: &str, expires_at: Option<u64>) -> ContentFile {
        ContentFile {
            id: id.into(),
            display_name: id.into(),
            asset_type: AssetType::Hull,
            seed: 1,
            universe: universe.into(),
            priority,
            expires_at,
            payload: ContentPayload::Hull(GeneratedMesh {
                vertices: vec![],
                indices: vec![],
            }),
        }
    }

    fn params(object_id: &str, now: u64) -> SeedParams {
        SeedParams {
            object_id: object_id.into(),
            universe: UniverseTier::Classic,
            now,
        }
    }

    #[test]
    fn no_candidates_is_procedural() {
        assert_eq!(resolve(&[], &params("loup_garou", 0)), Resolved::Procedural);
    }

    /// "authoritative always wins": even alongside a live event and a
    /// curated candidate for the same object, authoritative renders.
    #[test]
    fn authoritative_always_wins() {
        let overrides = vec![
            stub("loup_garou", Priority::Curated, "all", None),
            stub("loup_garou", Priority::Event, "all", Some(1000)),
            stub("loup_garou", Priority::Authoritative, "all", None),
        ];
        let resolved = resolve(&overrides, &params("loup_garou", 500));
        assert!(matches!(
            resolved,
            Resolved::Authored(c) if c.priority == Priority::Authoritative
        ));
    }

    /// "curated falls back when content missing": a curated override exists
    /// for a *different* object; the query object has no matching content,
    /// so resolution falls through to procedural.
    #[test]
    fn curated_falls_back_when_content_missing() {
        let overrides = vec![stub("sorrow_station", Priority::Curated, "all", None)];
        assert_eq!(
            resolve(&overrides, &params("loup_garou", 0)),
            Resolved::Procedural
        );
    }

    /// "event respects expires_at": active before the deadline...
    #[test]
    fn event_wins_while_active() {
        let overrides = vec![
            stub("loup_garou", Priority::Curated, "all", None),
            stub("loup_garou", Priority::Event, "all", Some(1000)),
        ];
        let resolved = resolve(&overrides, &params("loup_garou", 500));
        assert!(matches!(
            resolved,
            Resolved::Authored(c) if c.priority == Priority::Event
        ));
    }

    /// ...and falls back to the next-highest priority once it expires.
    #[test]
    fn event_falls_back_after_expiry() {
        let overrides = vec![
            stub("loup_garou", Priority::Curated, "all", None),
            stub("loup_garou", Priority::Event, "all", Some(1000)),
        ];
        let resolved = resolve(&overrides, &params("loup_garou", 1000));
        assert!(matches!(
            resolved,
            Resolved::Authored(c) if c.priority == Priority::Curated
        ));
    }

    /// An event with no `expires_at` never expires.
    #[test]
    fn event_without_expiry_never_expires() {
        let overrides = vec![stub("loup_garou", Priority::Event, "all", None)];
        let resolved = resolve(&overrides, &params("loup_garou", u64::MAX));
        assert!(matches!(resolved, Resolved::Authored(_)));
    }

    #[test]
    fn universe_mismatch_is_ignored() {
        let overrides = vec![stub("loup_garou", Priority::Authoritative, "byok", None)];
        assert_eq!(
            resolve(&overrides, &params("loup_garou", 0)),
            Resolved::Procedural
        );
    }

    #[test]
    fn universe_all_matches_every_tier() {
        let overrides = vec![stub("loup_garou", Priority::Curated, "all", None)];
        for tier in UniverseTier::ALL {
            let p = SeedParams {
                object_id: "loup_garou".into(),
                universe: tier,
                now: 0,
            };
            assert!(matches!(resolve(&overrides, &p), Resolved::Authored(_)));
        }
    }
}
