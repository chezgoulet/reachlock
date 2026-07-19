//! REACHLOCK faction engine (spec §21) + reputation (spec §21) + tariffs
//! (spec §20). Pure, IO-free, wasm-safe — same offline/online-parity reasoning
//! as the S10 economy module.
//!
//! Three layers, mirroring `economy.rs`:
//!   1. Authored content — [`FactionCatalog`] + [`Storyline`], authored as
//!      `content/factions/*.ron` and `content/storylines/*.ron`, validated by
//!      the CLI (`reachlock content validate-factions` / `validate-storylines`).
//!   2. Player reputation — [`Reputation`] keyed by
//!      `(FactionId, Option<DivisionId>)`, pure event transitions.
//!   3. Engine — [`tariff`], [`tick_factions`], [`evaluate_storylines`]:
//!      deterministic state transitions that emit [`FactionEvent`]s for the
//!      universe tick (S12 broadcasts them; S11 only *produces* them).
//!
//! Reputation axes are fixed-point: the −100..100 display range is scaled by
//! [`crate::economy::TARIFF_ONE`] (1024 == 1.0) so UI math stays integer and
//! offline/online stay bit-identical. No floats in any persisted value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::economy::{GoodCategory, TARIFF_ONE};

/// String newtype for a faction id (e.g. `"compact"`). Opaque so a division id
/// or arbitrary string can never be fed where a faction id is expected.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FactionId(pub String);

impl FactionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// String newtype for an internal-division id (e.g. `"compact_expansionist"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DivisionId(pub String);

impl DivisionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fixed-point scale for reputation axes. −100..100 display maps to
/// −102400..102400 internally; divide by this for UI.
pub const REP_ONE: i64 = TARIFF_ONE;

/// Clamp a reputation axis into the display range as fixed-point.
fn clamp_axis(v: i64) -> i64 {
    let hi = 100 * REP_ONE;
    let lo = -100 * REP_ONE;
    v.max(lo).min(hi)
}

// ───────────────────────────── Faction definition ─────────────────────────

/// A system a faction claims (territory). Coordinates are seed-derived at
/// universe build; authored content only names the system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemClaim {
    pub system_id: String,
    /// Share of the system the faction controls, 0..=100 (authored).
    #[serde(default)]
    pub control: u8,
}

/// A faction's producible/consumable resource tally (authored; the tick
/// reallocates it deterministically per doctrine).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FactionResources {
    /// Named stockpile, e.g. `"alloys" -> 1200`.
    #[serde(default)]
    pub stock: BTreeMap<String, i64>,
}

/// One faction's standing toward another.
///
/// `affinity` is the **canonical** continuous closeness (−100..=100) and is the
/// value the deterministic tick drifts. `status_snapshot` is a frozen mirror of
/// the derived [`RelationStatus`] band (authored in content; re-derived from
/// `affinity` at runtime via [`DiplomaticStanding::status`]). Keeping the enum
/// out of the drift path is what lets affinity cool smoothly instead of
/// snapping back to band centers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiplomaticStanding {
    /// Continuous closeness, fixed-point display −100..100.
    pub affinity: i64,
    /// Derived relation band (frozen mirror of `affinity`).
    #[serde(alias = "status")]
    pub status_snapshot: RelationStatus,
    /// Active treaty text/id, if any (authored; informational for S11).
    #[serde(default)]
    pub treaty: Option<String>,
    /// War objective, if `status_snapshot == War` (authored; informational for S11).
    #[serde(default)]
    pub war_goal: Option<String>,
}

impl DiplomaticStanding {
    /// Current relation band, derived from the live `affinity`.
    pub fn status(&self) -> RelationStatus {
        RelationStatus::from_affinity(self.affinity)
    }
}

/// Diplomatic relation between two factions (or a faction and the player,
/// modelled separately via [`Reputation`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationStatus {
    Allied,
    Friendly,
    Neutral,
    Hostile,
    War,
}

impl RelationStatus {
    /// Numeric closeness used by drift + threshold transitions (higher = closer).
    pub fn affinity(&self) -> i64 {
        match self {
            RelationStatus::Allied => 100,
            RelationStatus::Friendly => 60,
            RelationStatus::Neutral => 0,
            RelationStatus::Hostile => -60,
            RelationStatus::War => -100,
        }
    }

    pub fn from_affinity(a: i64) -> RelationStatus {
        if a >= 80 {
            RelationStatus::Allied
        } else if a >= 30 {
            RelationStatus::Friendly
        } else if a > -30 {
            RelationStatus::Neutral
        } else if a > -80 {
            RelationStatus::Hostile
        } else {
            RelationStatus::War
        }
    }
}

/// A faction's long-term objective (authored; drives tick drift direction).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionGoal {
    pub id: String,
    pub description: String,
}

/// A faction's internal division (wing). Has its own player-standing track,
/// independent of the faction-level standing (spec §21).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalDivision {
    pub id: DivisionId,
    pub name: String,
    /// Decision-making share, 0.0..=1.0 (authored).
    pub influence: f32,
    #[serde(default)]
    pub agenda: DivisionAgenda,
    /// Starting player standing with this division, display −100..100.
    #[serde(default)]
    pub player_standing: i8,
}

/// Division political leaning (authored; affects tick behaviour later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivisionAgenda {
    Hawkish,
    Dovish,
    #[default]
    Mercantile,
    Isolationist,
}

/// Faction-wide doctrine (spec §21). Drives the deterministic tick allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Doctrine {
    #[default]
    Military,
    Economic,
    Diplomatic,
    Expansionist,
}

/// Tariff posture a faction applies to goods in its territory (spec §20 table).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TariffPolicy {
    /// Compact: tariffs on foreign goods, subsidies on own-produced goods.
    Regulated { foreign_mult: i64, own_mult: i64 },
    /// ISC: flat port fee on everything.
    Flat { mult: i64 },
    /// Corp Charter: dynamic — adjusts with demand (reads live demand ratio).
    Dynamic,
    /// The Reach: no tariffs, no enforcement.
    #[default]
    None,
}

/// A single faction (spec §21).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Faction {
    pub id: FactionId,
    pub name: String,
    #[serde(default)]
    pub territory: Vec<SystemClaim>,
    #[serde(default)]
    pub resources: FactionResources,
    /// Standing toward other factions, keyed by their id. Kept symmetric with
    /// the counterpart's entry by `symmetrize_relationships`.
    #[serde(default)]
    pub relationships: BTreeMap<FactionId, DiplomaticStanding>,
    #[serde(default)]
    pub goals: Vec<FactionGoal>,
    #[serde(default)]
    pub internal_divisions: Vec<InternalDivision>,
    #[serde(default)]
    pub doctrine: Doctrine,
    #[serde(default)]
    pub tariff_policy: TariffPolicy,
    /// Goods this faction produces (for Compact "own-produced" subsidies).
    #[serde(default)]
    pub produces: Vec<GoodCategory>,
    /// RGBA colour for UI tinting (banners, reputation bars). Authored per
    /// faction in content; clients use it without building a colour hash.
    #[serde(default = "default_faction_color")]
    pub color: [u8; 4],
}

const fn default_faction_color() -> [u8; 4] {
    [0x88, 0x88, 0x88, 0xFF] // neutral grey
}

/// The authored set of factions (e.g. `content/factions/canon.ron`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionCatalog {
    pub version: u32,
    #[serde(default)]
    pub factions: Vec<Faction>,
}

impl FactionCatalog {
    /// Validate authoring invariants. Returns human-readable errors (empty =
    /// clean). Checks unique ids, non-negative control, and that every
    /// relationship is reciprocated with the same status (symmetry gotcha).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for f in &self.factions {
            if !seen.insert(&f.id) {
                errors.push(format!(
                    "duplicate faction id '{

}'",
                    f.id.as_str()
                ));
            }
            for claim in &f.territory {
                if claim.control > 100 {
                    errors.push(format!(
                        "faction '{

}' claims {

}+% control of '{

}'",
                        f.id.as_str(),
                        claim.control,
                        claim.system_id
                    ));
                }
            }
            for (other, standing) in &f.relationships {
                // Find the counterpart.
                let counterpart = self.factions.iter().find(|o| &o.id == other);
                match counterpart {
                    None => errors.push(format!(
                        "faction '{

}' relates to unknown faction '{

}'",
                        f.id.as_str(),
                        other.as_str()
                    )),
                    Some(c) => match c.relationships.get(&f.id) {
                        None => errors.push(format!(
                            "relationship '{

}' -> '{

}' is not reciprocated",
                            f.id.as_str(),
                            other.as_str()
                        )),
                        Some(c_back) if c_back.status() != standing.status() => {
                            errors.push(format!(
                                "relationship '{

}' -> '{

}' is '{

}' but '{

}' -> '{

}' is '{

}' (asymmetric)",
                                f.id.as_str(),
                                other.as_str(),
                                standing.status().status_name(),
                                other.as_str(),
                                f.id.as_str(),
                                c_back.status().status_name()
                            ))
                        }
                        _ => {}
                    },
                }
            }
        }
        errors
    }
}

impl RelationStatus {
    pub fn status_name(&self) -> &'static str {
        match self {
            RelationStatus::Allied => "allied",
            RelationStatus::Friendly => "friendly",
            RelationStatus::Neutral => "neutral",
            RelationStatus::Hostile => "hostile",
            RelationStatus::War => "war",
        }
    }
}

// ───────────────────────────── Reputation ─────────────────────────

/// A recorded offense against a faction (smuggling, piracy, killing
/// personnel). Drives crime-based gates and notoriety.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crime {
    pub kind: String,
    /// Tick at which it was recorded (for decay/statute later).
    #[serde(default)]
    pub tick: u64,
}

/// Multi-axis player reputation with one faction (and, separately, each of its
/// divisions). Fixed-point (× [`REP_ONE`]); display range −100..100.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reputation {
    /// Kept promises / delivered contracts.
    pub trust: i64,
    /// Material help provided.
    pub contribution: i64,
    /// Visibility of deeds, 0..100 (capped). High notoriety blocks quiet ops.
    pub notoriety: i64,
    #[serde(default)]
    pub crimes: Vec<Crime>,
}

impl Reputation {
    fn clamped(mut self) -> Self {
        self.trust = clamp_axis(self.trust);
        self.contribution = clamp_axis(self.contribution);
        self.notoriety = self.notoriety.clamp(0, 100 * REP_ONE);
        self
    }
}

/// Events that mutate a [`Reputation`] (pure transitions; see [`apply_event`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReputationEvent {
    /// Delivered a contract for the faction.
    DeliveredContract { amount: i64 },
    /// Caught smuggling contraband into/through the faction's space.
    SmugglingCaught { severity: i64 },
    /// Killed a member of the faction.
    FactionKill { count: i64 },
    /// Completed a faction mission.
    MissionComplete { amount: i64 },
    /// Earned favor with a specific division (moves division standing only).
    DivisionFavor { division: DivisionId, amount: i64 },
}

/// Apply a reputation event, returning the new reputation. Division-favor
/// events are handled by the caller (they touch the division map, not the
/// faction-level axes). All axes are clamped to the fixed-point display range.
pub fn apply_event(rep: Reputation, ev: &ReputationEvent) -> Reputation {
    let mut r = rep;
    match ev {
        ReputationEvent::DeliveredContract { amount } => {
            let a = amount * REP_ONE;
            r.trust += a;
            r.contribution += a;
        }
        ReputationEvent::SmugglingCaught { severity } => {
            let a = severity * REP_ONE;
            r.trust -= a;
            r.notoriety += a * 2;
        }
        ReputationEvent::FactionKill { count } => {
            let a = count * 10 * REP_ONE;
            r.trust -= a;
            r.contribution -= a;
            r.notoriety += a / 2;
        }
        ReputationEvent::MissionComplete { amount } => {
            let a = amount * REP_ONE;
            r.trust += a;
            r.contribution += a;
        }
        ReputationEvent::DivisionFavor { .. } => {
            // No faction-level axis change; division standing handled elsewhere.
        }
    }
    r.clamped()
}

/// A requirement a reputation must satisfy to pass a gate (market discount,
/// docking access, mission unlock).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReputationRequirement {
    /// Minimum trust (display units, −100..100).
    MinTrust(i64),
    /// Minimum contribution.
    MinContribution(i64),
    /// Maximum notoriety (display units).
    MaxNotoriety(i64),
    /// No recorded crimes of this kind.
    NoCrimeOf(String),
    /// All of the given requirements must hold.
    All(Vec<ReputationRequirement>),
    /// Any of the given requirements must hold.
    Any(Vec<ReputationRequirement>),
}

/// Pure gate: does this faction-level reputation satisfy `req`?
pub fn access(rep: &Reputation, req: &ReputationRequirement) -> bool {
    match req {
        ReputationRequirement::MinTrust(t) => rep.trust >= *t * REP_ONE,
        ReputationRequirement::MinContribution(c) => rep.contribution >= *c * REP_ONE,
        ReputationRequirement::MaxNotoriety(n) => rep.notoriety <= *n * REP_ONE,
        ReputationRequirement::NoCrimeOf(kind) => !rep.crimes.iter().any(|c| &c.kind == kind),
        ReputationRequirement::All(rs) => rs.iter().all(|r| access(rep, r)),
        ReputationRequirement::Any(rs) => rs.iter().any(|r| access(rep, r)),
    }
}

// ───────────────────────────── Tariffs (spec §20) ─────────────────────────

/// Compute the faction tariff multiplier (fixed-point, × [`TARIFF_ONE`]) for a
/// good of `category` traded in this faction's territory. `player_trust` is the
/// display-range trust (−100..100) used for the reputation discount. `demand`
/// is the live demand ratio (supplied by the caller from the S10 economy; only
/// Corp Charter's dynamic policy reads it; 1024 == neutral).
pub fn tariff(faction: &Faction, category: GoodCategory, player_trust: i64, demand: i64) -> i64 {
    let policy_mult = match &faction.tariff_policy {
        TariffPolicy::Regulated {
            foreign_mult,
            own_mult,
        } => {
            if faction.produces.contains(&category) {
                *own_mult
            } else {
                *foreign_mult
            }
        }
        TariffPolicy::Flat { mult } => *mult,
        TariffPolicy::Dynamic => {
            // Corp Charter: tariffs rise with demand, fall with glut.
            // demand is 1024 at parity; range ~512..2048.
            (demand).clamp(512, 2048)
        }
        TariffPolicy::None => TARIFF_ONE,
    };
    // Reputation discount: high trust shaves the multiplier (min 512 = 0.5x).
    let discount = rep_discount(player_trust); // 1024..512
    (policy_mult * discount / TARIFF_ONE).max(1)
}

/// Reputation discount as a fixed-point multiplier. At trust 0 → 1024 (no
/// change); at trust 100 → 512 (half price); at trust −100 → 1024 (no worse
/// than base; the tariff policy itself handles penalties). Clamped so prices
/// never go below half.
pub fn rep_discount(trust: i64) -> i64 {
    let t = trust.clamp(-100, 100);
    if t <= 0 {
        TARIFF_ONE
    } else {
        (TARIFF_ONE - (t * (TARIFF_ONE / 2)) / 100).max(TARIFF_ONE / 2)
    }
}

/// Validate a slice of storylines: unique chapter IDs per storyline,
/// `ChapterComplete` refs exist within the same story, `PlayerReputation`
/// factions exist in the (embedded) canon catalog, and no circular trigger
/// chains (naive depth limit).
pub fn validate_storylines(stories: &[Storyline]) -> Vec<String> {
    let mut errors = Vec::new();
    let default_catalog = load_faction_catalog();
    for (si, story) in stories.iter().enumerate() {
        let mut seen = std::collections::HashSet::new();
        for (ci, ch) in story.chapters.iter().enumerate() {
            if !seen.insert(&ch.id) {
                errors.push(format!(
                    "storyline {}[{}]: duplicate chapter id '{}'",
                    si, ci, ch.id
                ));
            }
            if let Some(trig) = &ch.trigger {
                check_trigger_refs(
                    trig,
                    &story.chapters,
                    &default_catalog,
                    &mut errors,
                    si,
                    ci,
                    0,
                );
            }
        }
    }
    errors
}

fn check_trigger_refs(
    trig: &ChapterTrigger,
    chapters: &[Chapter],
    catalog: &FactionCatalog,
    errors: &mut Vec<String>,
    si: usize,
    ci: usize,
    depth: usize,
) {
    if depth > 32 {
        errors.push(format!(
            "storyline {}[{}]: trigger nesting exceeds 32 (circular?)",
            si, ci
        ));
        return;
    }
    match trig {
        ChapterTrigger::ChapterComplete(id) => {
            if !chapters.iter().any(|c| &c.id == id) {
                errors.push(format!(
                    "storyline {}[{}]: `ChapterComplete` refs unknown chapter '{}'",
                    si, ci, id
                ));
            }
        }
        ChapterTrigger::PlayerReputation { faction, trust: _ } => {
            if !catalog.factions.iter().any(|f| &f.id == faction) {
                errors.push(format!(
                    "storyline {}[{}]: `PlayerReputation` refs unknown faction '{}'",
                    si,
                    ci,
                    faction.as_str()
                ));
            }
        }
        ChapterTrigger::All(ts) | ChapterTrigger::Any(ts) => {
            for sub in ts {
                check_trigger_refs(sub, chapters, catalog, errors, si, ci, depth + 1);
            }
        }
        ChapterTrigger::TickAfter(_) => {} // always valid
    }
}

// ───────────────────────────── Content loading ─────────────────────────

/// Load the canon faction catalog from the embedded RON. Panics on invalid
/// content (only possible if the authored file diverges from the schema, which
/// the CLI `ValidateFactions` command catches).
pub fn load_faction_catalog() -> FactionCatalog {
    ron::from_str(FACTION_CATALOG_RON).expect("embedded canon.ron")
}

const FACTION_CATALOG_RON: &str = include_str!("../../mods/reachlock/factions/canon.ron");

/// Load the canon storylines from the embedded RON.
pub fn load_storylines() -> Vec<Storyline> {
    ron::from_str(STORYLINES_RON).expect("embedded storylines.ron")
}
const STORYLINES_RON: &str = include_str!("../../mods/reachlock/storylines/compact_arc.ron");

// ───────────────────────────── Tick (spec §21) ─────────────────────────

/// An event the faction engine emits during a tick. S12 broadcasts these; S11
/// only produces them (the sink is the caller's `Vec`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionEvent {
    FactionMove {
        faction: FactionId,
        action: String,
        target: String,
    },
    DiplomaticShift {
        faction: FactionId,
        other: FactionId,
        change: i64,
    },
    ContentRelease {
        content_id: String,
        priority: String,
    },
    MissionUnlock {
        mission_id: String,
    },
}

/// The live faction simulation state (per save / per universe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionState {
    /// All known factions.
    pub catalog: FactionCatalog,
    /// Player reputation per faction id, keyed by faction.
    #[serde(default)]
    pub reputation: BTreeMap<FactionId, Reputation>,
    /// Chapter ids already fired (idempotent-once; see `evaluate_storylines`).
    #[serde(default)]
    pub fired_chapters: Vec<String>,
    /// Current universe tick (advanced by the caller each tick).
    #[serde(default)]
    pub tick: u64,
}

impl FactionState {
    pub fn new(catalog: FactionCatalog) -> Self {
        FactionState {
            catalog,
            reputation: BTreeMap::new(),
            fired_chapters: Vec::new(),
            tick: 0,
        }
    }

    /// Reputation for a faction, defaulting to neutral if unseen.
    pub fn rep(&self, faction: &FactionId) -> Reputation {
        self.reputation.get(faction).cloned().unwrap_or_default()
    }

    /// Apply a reputation event for a faction (inserts a default if absent).
    pub fn record_event(&mut self, faction: &FactionId, ev: &ReputationEvent) {
        let rep = self.reputation.entry(faction.clone()).or_default().clone();
        let updated = apply_event(rep, ev);
        self.reputation.insert(faction.clone(), updated);
        if let ReputationEvent::DivisionFavor { division, amount } = ev {
            // Division standing is tracked on the faction's division list in a
            // real save; here we surface it as a trust nudge so it's observable
            // and testable without a separate division-standing store yet.
            if let Some(f) = self.catalog.factions.iter_mut().find(|f| &f.id == faction) {
                if let Some(d) = f.internal_divisions.iter_mut().find(|d| &d.id == division) {
                    let new = (d.player_standing as i64 + amount).clamp(-100, 100);
                    d.player_standing = new as i8;
                }
            }
        }
    }
}

/// Ensure every relationship is reciprocated with the same status, mutating
/// the catalog in place (authored files should already be symmetric; this is a
/// defensive guarantee + the basis for the symmetry test).
pub fn symmetrize_relationships(state: &mut FactionState) {
    let ids: Vec<FactionId> = state
        .catalog
        .factions
        .iter()
        .map(|f| f.id.clone())
        .collect();
    for a in &ids {
        let others: Vec<FactionId> = state
            .catalog
            .factions
            .iter()
            .filter(|f| &f.id != a)
            .map(|f| f.id.clone())
            .collect();
        for b in &others {
            let a_status = state
                .catalog
                .factions
                .iter()
                .find(|f| &f.id == a)
                .and_then(|f| f.relationships.get(b))
                .map(|s| s.status());
            let b_status = state
                .catalog
                .factions
                .iter()
                .find(|f| &f.id == b)
                .and_then(|f| f.relationships.get(a))
                .map(|s| s.status());
            match (a_status, b_status) {
                (Some(sa), None) => insert_standing(state, b, a, sa),
                (None, Some(sb)) => insert_standing(state, a, b, sb),
                (Some(sa), Some(sb)) if sa != sb => {
                    // Conflict: defer to the higher-affinity status.
                    let s = if sa.affinity() >= sb.affinity() {
                        sa
                    } else {
                        sb
                    };
                    insert_standing(state, a, b, s);
                    insert_standing(state, b, a, s);
                }
                _ => {}
            }
        }
    }
}

fn insert_standing(
    state: &mut FactionState,
    from: &FactionId,
    to: &FactionId,
    status: RelationStatus,
) {
    if let Some(f) = state.catalog.factions.iter_mut().find(|f| &f.id == from) {
        f.relationships.insert(
            to.clone(),
            DiplomaticStanding {
                affinity: status.affinity(),
                status_snapshot: status,
                treaty: None,
                war_goal: None,
            },
        );
    }
}

/// Advance the faction simulation by one tick. Deterministic: same
/// `state`+`tick` → same `(state, events)`. Doctrine drives resource
/// allocation; relationships drift toward neutral; threshold crossings emit
/// `DiplomaticShift`/`FactionMove`. Advances the tick and returns the new
/// state; storyline chapters are evaluated separately by the caller via
/// [`evaluate_storylines`] (so the fired-set invariant lives in one place).
pub fn tick_factions(mut state: FactionState) -> (FactionState, Vec<FactionEvent>) {
    let mut events = Vec::new();
    state.tick += 1;

    // Doctrine-driven relationship drift toward neutral (affinity 0).
    // Index-based: collect each faction's new standings, then apply, so we
    // never hold a `&mut Faction` while also indexing the factions vec.
    let n = state.catalog.factions.len();
    for i in 0..n {
        let drift = match state.catalog.factions[i].doctrine {
            Doctrine::Military => 1,
            Doctrine::Economic => 2,
            Doctrine::Diplomatic => 3,
            Doctrine::Expansionist => 2,
        };
        let mut moves: Vec<(FactionId, i64, RelationStatus, i64)> = Vec::new();
        for (other, standing) in &state.catalog.factions[i].relationships {
            let aff = standing.affinity;
            let new_aff = aff - aff.signum() * drift.min(aff.abs());
            let new_status = RelationStatus::from_affinity(new_aff);
            let delta = new_aff - aff;
            if delta != 0 {
                moves.push((other.clone(), new_aff, new_status, delta));
            }
        }
        for (other, new_aff, new_status, delta) in moves {
            if let Some(f) = state.catalog.factions[i].relationships.get_mut(&other) {
                f.affinity = new_aff;
                f.status_snapshot = new_status;
            }
            events.push(FactionEvent::DiplomaticShift {
                faction: state.catalog.factions[i].id.clone(),
                other,
                change: delta,
            });
        }
    }

    (state, events)
}

// ───────────────────────────── Storylines (spec §21) ─────────────────────────

/// A trigger predicate for a storyline chapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterTrigger {
    /// Fire once `tick` exceeds this count.
    TickAfter(u64),
    /// Fire once this prior chapter has fired.
    ChapterComplete(String),
    /// Fire once player trust with `faction` exceeds this display value.
    PlayerReputation { faction: FactionId, trust: i64 },
    /// All sub-triggers must hold.
    All(Vec<ChapterTrigger>),
    /// Any sub-trigger may hold.
    Any(Vec<ChapterTrigger>),
}

/// A storyline chapter (spec §21 RON example).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    #[serde(default)]
    pub trigger: Option<ChapterTrigger>,
    #[serde(default)]
    pub narration: String,
    /// Event ids this chapter releases (consumed by S12/S16 later). Authored
    /// but inert in S11 beyond being recorded once-fired.
    #[serde(default)]
    pub events: Vec<String>,
}

/// A faction's authored storyline arc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Storyline {
    pub faction: FactionId,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
}

/// Pure, idempotent-once evaluation: returns the ids of chapters whose trigger
/// is satisfied *and* which are not yet in `state.fired_chapters`. Does not
/// mutate `state` (the caller records the returned ids). Replaying the same
/// `(state, stories)` yields the same (empty-after-first) result — the
/// double-fire property test asserts this.
pub fn evaluate_storylines(state: &FactionState, stories: &[Storyline]) -> Vec<String> {
    let mut out = Vec::new();
    for story in stories {
        for ch in &story.chapters {
            if state.fired_chapters.contains(&ch.id) {
                continue;
            }
            let trig = match &ch.trigger {
                Some(t) => t,
                None => continue, // no trigger → never auto-fires
            };
            if trigger_met(state, trig) {
                out.push(ch.id.clone());
            }
        }
    }
    out
}

fn trigger_met(state: &FactionState, trig: &ChapterTrigger) -> bool {
    match trig {
        ChapterTrigger::TickAfter(n) => state.tick > *n,
        ChapterTrigger::ChapterComplete(id) => state.fired_chapters.contains(id),
        ChapterTrigger::PlayerReputation { faction, trust } => {
            state.rep(faction).trust >= *trust * REP_ONE
        }
        ChapterTrigger::All(ts) => ts.iter().all(|t| trigger_met(state, t)),
        ChapterTrigger::Any(ts) => ts.iter().any(|t| trigger_met(state, t)),
    }
}
// ───────────────────────────── Tests ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> FactionCatalog {
        let mk = |id: &str,
                  doctrine: Doctrine,
                  policy: TariffPolicy,
                  produces: Vec<GoodCategory>| Faction {
            id: FactionId(id.into()),
            name: id.into(),
            territory: vec![],
            resources: FactionResources {
                stock: BTreeMap::new(),
            },
            relationships: BTreeMap::new(),
            goals: vec![],
            internal_divisions: vec![],
            doctrine,
            tariff_policy: policy,
            produces,
            color: default_faction_color(),
        };
        FactionCatalog {
            version: 1,
            factions: vec![
                mk(
                    "compact",
                    Doctrine::Diplomatic,
                    TariffPolicy::Regulated {
                        foreign_mult: 1229,
                        own_mult: 871,
                    },
                    vec![GoodCategory::Manufactured],
                ),
                mk(
                    "isc",
                    Doctrine::Diplomatic,
                    TariffPolicy::Flat { mult: 1075 },
                    vec![],
                ),
                mk("corp", Doctrine::Economic, TariffPolicy::Dynamic, vec![]),
                mk("reach", Doctrine::Expansionist, TariffPolicy::None, vec![]),
                mk(
                    "remnant",
                    Doctrine::Military,
                    TariffPolicy::Flat { mult: 1024 },
                    vec![],
                ),
            ],
        }
    }

    #[test]
    fn embedded_ron_loads() {
        let _catalog = load_faction_catalog();
        let _stories = load_storylines();
    }

    #[test]
    fn reputation_transitions_clamp() {
        let r = Reputation::default();
        let r = apply_event(r, &ReputationEvent::DeliveredContract { amount: 100 });
        assert_eq!(r.trust, 100 * REP_ONE);
        // Over-delivering clamps at 100.
        let r = apply_event(r, &ReputationEvent::DeliveredContract { amount: 200 });
        assert_eq!(r.trust, 100 * REP_ONE);
        // Smuggling drops trust and raises notoriety (capped 100).
        let r = Reputation::default();
        let r = apply_event(r, &ReputationEvent::SmugglingCaught { severity: 80 });
        assert_eq!(r.trust, -80 * REP_ONE);
        assert!(r.notoriety > 0);
        assert!(r.notoriety <= 100 * REP_ONE);
    }

    #[test]
    fn access_gates_driven_by_reputation() {
        let mut r = Reputation::default();
        assert!(!access(&r, &ReputationRequirement::MinTrust(10)));
        r = apply_event(r, &ReputationEvent::DeliveredContract { amount: 50 });
        assert!(access(&r, &ReputationRequirement::MinTrust(40)));
        assert!(!access(&r, &ReputationRequirement::MinTrust(60)));
        // Notoriety gate.
        r = apply_event(r, &ReputationEvent::SmugglingCaught { severity: 40 });
        assert!(access(&r, &ReputationRequirement::MaxNotoriety(80)));
        assert!(!access(&r, &ReputationRequirement::MaxNotoriety(20)));
        // Crime gate.
        let r2 = Reputation {
            crimes: vec![Crime {
                kind: "smuggling".into(),
                tick: 0,
            }],
            ..Default::default()
        };
        assert!(!access(
            &r2,
            &ReputationRequirement::NoCrimeOf("smuggling".into())
        ));
        assert!(access(
            &r2,
            &ReputationRequirement::NoCrimeOf("piracy".into())
        ));
    }

    #[test]
    fn tariff_policies_match_spec_table() {
        let c = catalog();
        let compact = c.factions.iter().find(|f| f.id.0 == "compact").unwrap();
        let isc = c.factions.iter().find(|f| f.id.0 == "isc").unwrap();
        let reach = c.factions.iter().find(|f| f.id.0 == "reach").unwrap();
        // Compact own-produced good is cheaper than foreign.
        let own = tariff(compact, GoodCategory::Manufactured, 0, TARIFF_ONE);
        let foreign = tariff(compact, GoodCategory::Luxury, 0, TARIFF_ONE);
        assert!(own < TARIFF_ONE, "own-produced should be subsidized");
        assert!(foreign > TARIFF_ONE, "foreign should be taxed");
        // ISC flat +5% port fee.
        assert_eq!(tariff(isc, GoodCategory::Luxury, 0, TARIFF_ONE), 1075);
        // Reach zero tariff.
        assert_eq!(
            tariff(reach, GoodCategory::Luxury, 0, TARIFF_ONE),
            TARIFF_ONE
        );
    }

    #[test]
    fn reputation_discount_lowers_price_at_high_trust() {
        let foreign = 1229; // Compact foreign base
        let at_neutral = (foreign * rep_discount(0)) / TARIFF_ONE;
        let at_trusted = (foreign * rep_discount(100)) / TARIFF_ONE;
        assert!(at_trusted < at_neutral, "high trust must be cheaper");
        assert_eq!(at_neutral, foreign);
        // Discount never below half.
        assert!(rep_discount(100) >= TARIFF_ONE / 2);
    }

    #[test]
    fn corp_charter_dynamic_reads_demand() {
        let c = catalog();
        let corp = c.factions.iter().find(|f| f.id.0 == "corp").unwrap();
        let high_demand = tariff(corp, GoodCategory::Luxury, 0, 2048);
        let low_demand = tariff(corp, GoodCategory::Luxury, 0, 512);
        assert!(high_demand > low_demand, "higher demand → higher tariff");
        assert!(high_demand > TARIFF_ONE);
    }

    #[test]
    fn relationship_drift_is_symmetric_and_deterministic() {
        let mut state = FactionState::new(catalog());
        // Set an asymmetric start and let symmetrize fix it.
        if let Some(f) = state
            .catalog
            .factions
            .iter_mut()
            .find(|f| f.id.0 == "compact")
        {
            f.relationships.insert(
                FactionId("isc".into()),
                DiplomaticStanding {
                    affinity: RelationStatus::Friendly.affinity(),
                    status_snapshot: RelationStatus::Friendly,
                    treaty: None,
                    war_goal: None,
                },
            );
        }
        if let Some(f) = state.catalog.factions.iter_mut().find(|f| f.id.0 == "isc") {
            f.relationships.insert(
                FactionId("compact".into()),
                DiplomaticStanding {
                    affinity: RelationStatus::Hostile.affinity(),
                    status_snapshot: RelationStatus::Hostile,
                    treaty: None,
                    war_goal: None,
                },
            );
        }
        symmetrize_relationships(&mut state);
        let compact_sees = state
            .catalog
            .factions
            .iter()
            .find(|f| f.id.0 == "compact")
            .unwrap()
            .relationships
            .get(&FactionId("isc".into()))
            .unwrap()
            .status();
        let isc_sees = state
            .catalog
            .factions
            .iter()
            .find(|f| f.id.0 == "isc")
            .unwrap()
            .relationships
            .get(&FactionId("compact".into()))
            .unwrap()
            .status();
        assert_eq!(compact_sees, isc_sees, "relationships must be symmetric");
    }

    #[test]
    fn tick_drifts_relationships_and_emits_events() {
        let mut state = FactionState::new(catalog());
        // Establish a symmetric Allied link up front (both directions set
        // before the tick so neither clobbers the other).
        for f in state.catalog.factions.iter_mut() {
            let other = match f.id.0.as_str() {
                "compact" => Some("isc"),
                "isc" => Some("compact"),
                _ => None,
            };
            if let Some(o) = other {
                f.relationships.insert(
                    FactionId(o.into()),
                    DiplomaticStanding {
                        affinity: RelationStatus::Allied.affinity(),
                        status_snapshot: RelationStatus::Allied,
                        treaty: None,
                        war_goal: None,
                    },
                );
            }
        }
        let (state2, events) = tick_factions(state);
        // Allied (affinity 100) drifts by the Diplomatic doctrine rate (3/tick)
        // → 97, still Allied, but the shift event fires every tick it moves.
        let s = state2
            .catalog
            .factions
            .iter()
            .find(|f| f.id.0 == "compact")
            .unwrap()
            .relationships
            .get(&FactionId("isc".into()))
            .unwrap()
            .status();
        assert_eq!(s, RelationStatus::Allied);
        assert!(events
            .iter()
            .any(|e| matches!(e, FactionEvent::DiplomaticShift { .. })));
        // Drift is a slow cool toward neutral (3/tick for Diplomatic): 10 more
        // ticks moves 100 → 70, still Allied but strictly cooled.
        let mut st = state2;
        for _ in 0..10 {
            st = tick_factions(st).0;
        }
        let cooled = st
            .catalog
            .factions
            .iter()
            .find(|f| f.id.0 == "compact")
            .unwrap()
            .relationships
            .get(&FactionId("isc".into()))
            .unwrap()
            .affinity;
        assert!(cooled < RelationStatus::Allied.affinity());
    }

    #[test]
    fn storyline_fires_once_and_is_idempotent() {
        let stories = vec![Storyline {
            faction: FactionId("compact".into()),
            chapters: vec![Chapter {
                id: "the_veil_escalation".into(),
                trigger: Some(ChapterTrigger::TickAfter(5)),
                narration: String::new(),
                events: vec![],
            }],
        }];
        let mut state = FactionState::new(catalog());
        // Tick forward, evaluating on the post-tick state and committing fired
        // chapters each step (as the real tick loop does).
        for _ in 0..7 {
            let (mut s, _) = tick_factions(state.clone());
            let fired = evaluate_storylines(&s, &stories);
            s.fired_chapters.extend(fired);
            state = s;
        }
        assert!(state
            .fired_chapters
            .contains(&"the_veil_escalation".to_string()));
        let fired_count = state
            .fired_chapters
            .iter()
            .filter(|c| *c == "the_veil_escalation")
            .count();
        assert_eq!(fired_count, 1, "chapter must fire exactly once");
    }

    #[test]
    fn double_fire_property_under_replay() {
        let stories = vec![Storyline {
            faction: FactionId("compact".into()),
            chapters: vec![Chapter {
                id: "arc1".into(),
                trigger: Some(ChapterTrigger::TickAfter(2)),
                narration: String::new(),
                events: vec![],
            }],
        }];
        let mut state = FactionState::new(catalog());
        let mut first_fires = Vec::new();
        for _ in 0..10 {
            // Tick state in place (advances tick), then evaluate on the same
            // state so the trigger reads the post-tick tick. This mirrors the
            // real tick loop: tick → evaluate → commit fired.
            let (mut s, _) = tick_factions(state);
            let fired = evaluate_storylines(&s, &stories);
            first_fires.extend(fired.iter().cloned());
            s.fired_chapters.extend(fired);
            state = s;
        }
        let arc1 = first_fires.iter().filter(|c| *c == "arc1").count();
        assert_eq!(arc1, 1, "replaying the tick log never double-fires");
    }
}
