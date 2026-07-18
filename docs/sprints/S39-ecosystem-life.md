# S39 — Ecosystem & Life

**Spec:** New (biome diversity, procedural life) · **Wave 9 (Living Galaxy) ·
Depends on:** S04 (system generator), S05 (item icons), S36 (dilemma generator)

## Outcome

Every planet teems with life. Not just a biome label — a full taxonomy tree
from kingdom to species, each with ecological roles, generated names, and
procedural visual representations. The player's scanner reveals species one
by one; the discovery catalog fills with research data. Events change
ecosystems — invasive species from a derelict ship, extinction from a mining
operation, mutation from a Predecessor artifact. The galaxy feels alive
because it IS alive, and that life changes.

## Context

- Humans are the only known sapient species. But the universe is not empty.
  Every habitable planet has ecosystems — some recognizable, some alien.
  Life is varied, weird, and discoverable. It's a first-class gameplay layer.
- The generator already produces planets with biomes (S04). This sprint adds
  a layer beneath the biome: the ecosystem. A biome is "tropical." An
  ecosystem is the specific web of life on THIS tropical planet.
- Event-driven change means the ecosystem catalog is the BASE state. Trope
  encounters (S40), dilemmas (S36), player actions (mining, colonization),
  and faction events (war, terraforming) modify it. The catalog records
  both the original state and the current state.
- Visual representation reuses the procedural icon pipeline from S05 items.
  Species get generated shapes, colors, and silhouettes — organic variants
  of the item icon system.

## Freeze first

### Ecosystem types (`generator/ecosystem.rs`)

```rust
pub struct Ecosystem {
    pub planet_seed: u64,
    pub biomes: Vec<BiomeEcosystem>,
    pub global_species_count: u32,
    pub endemic_species_count: u32,      // species found nowhere else
    pub ecological_complexity: EcosystemComplexity,
    pub baseline_recorded: bool,         // has the player scanned the baseline?
}

pub struct BiomeEcosystem {
    pub biome: Biome,
    pub species: Vec<Species>,
    pub food_web: FoodWeb,
    pub keystone_species: Vec<String>,   // species IDs critical to the web
}

pub struct Species {
    pub id: String,                      // seed-derived, globally unique
    pub taxonomy: Taxonomy,
    pub common_name: String,             // generated: "Verdant Skimmer"
    pub scientific_name: String,         // generated: "Planctos verdantis"
    pub ecological_role: EcologicalRole,
    pub size_class: SizeClass,
    pub habitat: String,                 // "canopy", "deep ocean", "cave system"
    pub rarity: Rarity,
    pub visual: SpeciesVisual,
    pub discoverable: bool,              // has the player scanned this?
    pub research_value: u32,             // points for cataloging
    pub edibility: Edibility,            // can crew eat it?
    pub medicinal_potential: u8,         // 0-10 — can it be harvested for meds?
    pub danger_level: u8,                // 0-10 — will it try to kill you?
}

pub struct Taxonomy {
    pub kingdom: String,                 // "Animalia", "Phytoid", "Mycoid", "Microbial", "Xenobiotic"
    pub phylum: String,
    pub class: String,
    pub order: String,
    pub family: String,
    pub genus: String,
    pub species: String,
}

pub enum EcologicalRole {
    PrimaryProducer,     // photosynthesizers, chemosynthesizers
    Herbivore,
    Carnivore,
    Omnivore,
    Decomposer,
    Parasite,
    Symbiote,
    FilterFeeder,
    Scavenger,
}

pub enum EcosystemComplexity {
    Barren,        // few species, simple web
    Simple,        // 10-50 species
    Developed,     // 50-200 species
    Rich,          // 200-500 species
    Verdant,       // 500+ species, complex web
}

pub struct FoodWeb {
    pub edges: Vec<(String, String, f64)>, // (predator_id, prey_id, dependency_weight)
}

pub struct SpeciesVisual {
    pub silhouette: u8,                  // procedural shape index
    pub primary_color: ColorRgba8,
    pub secondary_color: ColorRgba8,
    pub body_plan: BodyPlan,
    pub size_hint: String,               // "fist-sized", "human-sized", "building-sized"
}

pub enum BodyPlan {
    Radial,
    Bilateral,
    Asymmetric,
    Colonial,        // many small organisms acting as one
    Amorphous,
    Segmented,
}

pub enum Edibility {
    Toxic,
    Inedible,
    Edible { nutrition_value: u32 },
    Delicacy { nutrition_value: u32, market_value: u32 },
}
```

### Ecosystem event model (`generator/ecosystem_events.rs`)

```rust
pub struct EcosystemEvent {
    pub event_type: EcosystemEventType,
    pub affected_biomes: Vec<Biome>,
    pub affected_species: Vec<String>,
    pub magnitude: u8,
    pub description_template: String,
}

pub enum EcosystemEventType {
    Extinction { cause: String },
    InvasiveSpecies { origin: String, introduced_by: String },
    Mutation { cause: String, new_trait: String },
    PopulationBoom { cause: String },
    PopulationCrash { cause: String },
    NewSpecies { parent_species: String, divergence_reason: String },
    EcologicalCollapse { trigger: String },
    Recovery { from: String },
}
```

## Deliverables

### 1. Ecosystem generator (`core/src/generator/ecosystem.rs`)

- [ ] `generate_ecosystem(planet_seed, biomes, planet_params) -> Ecosystem` —
      pure function. Produces a full ecosystem from seed. Species count
      scales with planet habitability, age, and biome diversity. Taxonomic
      tree is deterministic — same seed = same kingdom/phylum/class tree.
- [ ] Species name generation: common names from a template system
      (`"{adjective} {noun}"` — "Verdant Skimmer", "Crimson Burrower").
      Scientific names use a pseudo-Latin syllable generator fed by the
      seed. Deterministic.
- [ ] Food web generation: for each biome, assign ecological roles to species,
      then connect predators to prey with dependency weights. Uses a
      simplified trophic model — producers at the bottom, apex predators
      at the top. Ensures every species has a food source (except primary
      producers).
- [ ] `generate_species_visual(seed, body_plan) -> SpeciesVisual` — produces
      a procedural icon using the same pipeline as S05 item icons but with
      organic shapes (radial symmetry for Radial body plan, bilateral for
      Bilateral, etc.).
- [ ] Determinism: add `ecosystem_generation` to determinism manifest. Test
      with fixed seed + biome set → identical Ecosystem struct.

### 2. Scanner & discovery (`client/src/systems/scanner.rs` expansion)

- [ ] Scanner mode (existing from S09): when aimed at a planet, shows biome
      summary. Now shows ecosystem summary: "Tropical biome. Ecosystem
      complexity: Rich. Species cataloged: 12/347. Keystone species: Verdant
      Skimmer."
- [ ] Active scan: hold scanner on a planet to reveal species one at a time.
      Each scan pulse reveals 1-5 species (depending on scanner quality).
      Species appear in the discovery log with name, visual, role, and a
      generated one-line description.
- [ ] Complete biome bonus: catalog all species in a biome → research points
      bonus. Catalog an entire planet → major bonus + planet gets a
      "surveyed" tag. First player to survey a planet gets a discovery
      credit (seed ledger entry).
- [ ] Discovery log UI: planetary view with a grid of species cards. Each
      card shows the visual, name, taxonomy path, ecological role, and
      whether it's been scanned. Unscanned cards are silhouettes with
      question marks.

### 3. Ecosystem events (`core/src/generator/ecosystem_events.rs`)

- [ ] `apply_ecosystem_event(ecosystem, event) -> Ecosystem` — pure function.
      Takes an ecosystem and an event, returns the modified ecosystem.
      Extinction removes species and updates the food web. Invasive species
      adds a species to a new biome and recalculates food web edges.
      Mutation modifies an existing species' traits.
- [ ] Event triggers: trope encounters (S40) can emit ecosystem events.
      Dilemmas (S36) can emit ecosystem events. Player actions (mining
      operation started, colony founded) can emit ecosystem events. Faction
      events (war, terraforming) can emit ecosystem events. Events are
      applied when the player next visits the planet.
- [ ] Ecological collapse: if a keystone species goes extinct, the food web
      is recalculated. Species dependent on it risk cascading extinction.
      The collapse is logged as a planetary event. The player can witness
      a "before and after."
- [ ] Recovery: ecosystems can recover over time (ticks). A crashed
      population slowly rebounds. New species evolve to fill vacant niches.
      The recovery timeline is tens of ticks — the player can return to a
      changed planet.

### 4. Harvesting & crew interaction

- [ ] Edible species can be harvested for food supplies (crew provisions).
      Medicinal species can be harvested for med bay supplies (healing rate).
      Dangerous species appear as environmental hazards during landed
      exploration (S20). Toxic species damage crew on contact.
- [ ] Crew with science/biology backgrounds (from their soul file) get
      bonuses to scanning speed and research point yield. Their contract
      can trigger "interesting specimen discovered" events — the crew
      member gets excited about a find.
- [ ] Xenobiology as a career track input (S42): discovering species
      contributes to Explorer and Science career progression.

### 5. Authoring overrides

- [ ] Specific species can be authored in `.ron` files. Override any field:
      name, visual, taxonomy placement, ecological role. The generator
      fills the rest from seed.
- [ ] Full ecosystem overrides: an authored planet can have its entire
      ecosystem hand-crafted. Useful for story-critical planets.
- [ ] Event injection: authored ecosystem events that fire at specific
      story beats. "When the player activates the Predecessor artifact on
      Aethon, the equatorial species mutate."

### 6. Metrics collection

- [ ] `ecosystem_discovery_events` table: scans per planet, species
      cataloged, complete biomes, discovery credits.
- [ ] Research questions: what ecosystems are players most drawn to scan?
      Does ecosystem complexity correlate with planet visit duration?
      What species traits generate the most crew reactions?

## Acceptance gates

```
cargo test -p reachlock-core generator::ecosystem::
# generation deterministic, food web validity (no orphan species),
# event application correct, species name uniqueness within biome
cargo test -p reachlock-core determinism::  # ecosystem goldens
make check
```

Manual: fly to a habitable planet → scan → watch species populate the catalog
→ read a species description → find a keystone predator → encounter a trope
(S40) that introduces an invasive species → return to the planet → catalog
now shows the invader + one extinct native species.

## Non-goals

- Live ecosystem simulation with population dynamics every tick. Changes are
  event-driven, not simulated. The tick advances the recovery counter but
  doesn't recalculate predator/prey populations each tick.
- Player terraforming / ecosystem engineering (build your own biosphere).
  Phase 4.
- Full 3D creature rendering. Species have 2D procedural icons in the catalog.
  Future sprint for 3D creature encounters in landed mode.
- Microbiome / microbial layer. Species are macro-scale (visible organisms).
- Cross-planet species migration (except via authored events).

## Gotchas

- Taxonomy tree generation must produce valid tree structures — every species
  belongs to exactly one path from kingdom to species. The generator builds
  the tree top-down (kingdom → phyla → classes → orders → families → genera →
  species), then places species at leaf nodes. No floating species without
  a full ancestry path.
- Scientific name generation uses a syllable table fed by the RNG. The table
  must produce plausible pseudo-Latin names. Test for duplicates within a
  biome. Seed the RNG per genus so species in the same genus have related
  names.
- Food web generation must guarantee no cycles (A eats B which eats A).
  The generator assigns trophic levels, then only allows edges from higher
  to lower levels. Apex predators have the highest level. Primary producers
  have level 0.
- Extinction events can cause cascading extinctions if dependency weights
  are high. The collapse algorithm must terminate (graph traversal with a
  visited set). Document the worst-case scenario: a keystone extinction
  that takes out 40% of the biome's species.
