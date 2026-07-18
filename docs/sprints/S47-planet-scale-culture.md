# S47 — Planet Scale & Culture

**Spec:** New (planetary culture, city scale) · **Wave 9 (Living Galaxy) ·
Depends on:** S04 (system generator), S11 (faction engine)

## Outcome

Planets are not dots on a system map — they're worlds. Each planet has scale:
diameter, gravity, atmosphere, climate, seasons, multiple biomes, resource
distribution. Where humans settle, cities grow — placed by the generator at
resource and biome intersections, sized to population, districted by purpose.
And from the planet's conditions, the culture emerges: language drift, customs,
social structure, architecture, clothing, attitude toward outsiders. A mining
colony on a high-gravity toxic world settled by Compact loyalists 80 years ago
is DIFFERENT from a trade hub on a temperate garden world in ISC space settled
by refugees 20 years ago. The generator produces coherent, distinct cultures
that feel intentional — not just random parameter combinations. Authorable
at any level: override a planet's culture entirely, or just change the
greeting ritual.

## Context

- S04 generates systems with planets that have biomes and threat levels.
  This sprint replaces the "planet is a biome label" model with a full
  planetary generation pipeline.
- S39 generates ecosystems per biome. Culture and cities exist alongside
  ecosystems — they're separate but interacting layers. A planet's
  ecosystem shapes its culture (a world with dangerous predators breeds
  cautious people). A planet's culture shapes its ecosystem (a industrial
  world pollutes its biomes).
- The settlement model: humans arrived at different times through different
  waves. "First Wave" colonies are 100+ years old, deeply established.
  "Frontier" settlements are <10 years old, barely hanging on. This
  settlement age affects culture deeply.
- "Same model as everything else: generator driven but can be overridden
  with authored content at any level." A specific planet can have a fully
  authored culture. Or just an authored architectural style. Or just an
  authored greeting. The generator fills everything else.

## Freeze first

### Planetary parameters (`generator/planet_extended.rs`)

```rust
pub struct PlanetExtended {
    pub planet_id: String,              // seed-derived
    pub name: String,
    pub physical: PlanetPhysical,
    pub climate: PlanetClimate,
    pub habitability: Habitability,
    pub resources: PlanetResources,
    pub settlements: Vec<Settlement>,
    pub culture: PlanetCulture,
    pub ecosystem_id: String,           // links to S39 ecosystem
}

pub struct PlanetPhysical {
    pub diameter_km: u32,
    pub gravity_g: Fixed,               // fraction of Earth gravity
    pub atmosphere: AtmosphereType,
    pub atmosphere_density: Fixed,      // fraction of Earth density
    pub surface_water_pct: u8,
    pub tectonic_activity: u8,          // 0-10
    pub magnetic_field: u8,             // 0-10 — radiation protection
}

pub enum AtmosphereType {
    None,
    Trace,
    Thin,
    Standard,
    Dense,
    Toxic,
    Corrosive,
    Exotic,          // methane, ammonia, etc.
}

pub struct PlanetClimate {
    pub temperature_range: (i32, i32),  // Celsius
    pub seasons: SeasonIntensity,
    pub weather_patterns: Vec<WeatherPattern>,
    pub climate_zones: u8,              // number of distinct climate bands
}

pub enum SeasonIntensity {
    None,            // no axial tilt
    Mild,
    Moderate,
    Extreme,
    Chaotic,         // unpredictable — tidally influenced
}

pub struct WeatherPattern {
    pub pattern_type: WeatherType,
    pub frequency: u8,                  // 0-10
    pub severity: u8,                   // 0-10
}

pub enum WeatherType {
    Rain, Storm, Hurricane, Tornado,
    DustStorm, SandStorm,
    Blizzard, IceStorm,
    AcidRain, MethaneFog,
    SolarFlare, RadiationSurge,
    ElectromagneticStorm,
}

pub struct Habitability {
    pub human_compatible: bool,         // can walk outside without a suit?
    pub requires_suit: bool,
    pub requires_dome: bool,
    pub terraformable: bool,
    pub habitability_index: u8,         // 0-100 — aggregate score
    pub hazards: Vec<Hazard>,
}

pub enum Hazard {
    ToxicAtmosphere,
    ExtremeTemperature,
    HighGravity,
    LowGravity,
    Radiation,
    AggressiveFauna,       // from S39
    GeologicalInstability,
    CorrosiveEnvironment,
    ParasiticMicrobes,
}

pub struct PlanetResources {
    pub mineral_richness: u8,
    pub mineral_types: Vec<String>,     // good IDs from S44
    pub organic_richness: u8,
    pub energy_potential: u8,           // solar, geothermal, wind
    pub rare_element_presence: u8,      // Predecessor materials
    pub resource_map: ResourceMap,
}

pub struct ResourceMap {
    pub deposits: Vec<ResourceDeposit>,
}

pub struct ResourceDeposit {
    pub good_id: String,
    pub location: (Fixed, Fixed),       // planetary coordinates
    pub richness: u8,
    pub accessibility: u8,              // how easy to extract
}
```

### Settlement and culture (`generator/culture.rs`)

```rust
pub struct Settlement {
    pub settlement_id: String,
    pub name: String,                    // generated: "Verdant Landing", "Ferrite Point"
    pub settlement_type: SettlementType,
    pub population: u64,
    pub founded_tick: u64,              // how old is this settlement?
    pub founding_faction: String,       // who settled it
    pub founding_wave: SettlementWave,
    pub districts: Vec<District>,
    pub starport_size: u8,
    pub economic_focus: EconomicFocus,
}

pub enum SettlementType {
    Outpost,         // <1000 people
    Colony,          // 1000-10000
    Settlement,      // 10000-100000
    City,            // 100000-1M
    Metropolis,      // 1M-10M
    Megacity,        // 10M+
}

pub enum SettlementWave {
    FirstWave,       // 100+ years — original colonization
    SecondWave,      // 50-100 years — expansion era
    ThirdWave,       // 20-50 years — post-Compact formation
    RecentColony,    // 5-20 years
    FrontierOutpost, // <5 years
}

pub struct District {
    pub district_type: DistrictType,
    pub name: String,                    // "The Warrens", "Starport District"
    pub notable_locations: Vec<String>,  // station IDs, unique NPC locations
}

pub enum DistrictType {
    Starport,
    Industrial,
    Residential,
    Commercial,
    Administrative,
    Research,
    Military,
    Undercity,
    Agricultural,
}

pub enum EconomicFocus {
    Mining,
    Manufacturing,
    Agriculture,
    Trade,
    Research,
    Military,
    Tourism,
    Refugee,
    Mixed,
}

pub struct PlanetCulture {
    pub cultural_id: String,
    pub language: LanguageProfile,
    pub customs: Vec<Custom>,
    pub social_structure: SocialStructure,
    pub architecture: ArchitecturalStyle,
    pub clothing: ClothingStyle,
    pub attitude_toward_outsiders: OutsiderAttitude,
    pub faction_allegiance: FactionAllegiance,
    pub dominant_values: Vec<CulturalValue>,
    pub cultural_quirk: String,          // one distinctive trait — "they never make eye contact"
}

pub struct LanguageProfile {
    pub base_language: String,           // "Compact Standard", "ISC Common"
    pub drift_intensity: u8,             // 0-10 — how different from the base
    pub accent_name: String,             // "Verdant Drawl", "Ferrite Creak"
    pub unique_terms: Vec<String>,       // local slang, loanwords from ecosystem
    pub greeting: String,                // "May the rock hold you"
    pub farewell: String,                // "Walk in pressure"
}

pub struct Custom {
    pub custom_type: CustomType,
    pub description: String,
    pub trigger: String,                 // when does this custom manifest?
}

pub enum CustomType {
    Greeting,            // how people greet each other
    Farewell,
    GiftGiving,          // what gifts mean, what's appropriate
    Dining,              // food customs, communal eating
    Bargaining,          // how trade is conducted
    Conflict,            // how disputes are resolved
    Mourning,            // death and loss rituals
    Celebration,         // holidays, festivals
    Taboo,               // things you never do
}

pub enum SocialStructure {
    Egalitarian,
    Hierarchical { castes: Vec<String> },
    Meritocratic,
    Corporate,
    Military,
    Religious,
    Communal,
    Individualistic,
}

pub struct ArchitecturalStyle {
    pub style_name: String,              // "Brutalist Dome", "Organic Spire"
    pub materials: Vec<String>,          // "ferrocrete", "woven carbon"
    pub dominant_shape: String,          // "dome", "tower", "sprawl", "subterranean"
    pub color_palette: ColorScheme,
    pub adapted_to: Vec<String>,         // "high gravity" → thick, low buildings
}

pub struct ClothingStyle {
    pub style_name: String,
    pub primary_material: String,
    pub dominant_colors: ColorScheme,
    pub practicality_level: u8,         // 0 = ceremonial, 10 = purely functional
    pub adapted_to: Vec<String>,        // "toxic atmosphere" → always wear sealed suits
}

pub struct ColorScheme {
    pub primary: ColorRgba8,
    pub secondary: ColorRgba8,
    pub accent: ColorRgba8,
    pub preference: ColorPreference,     // warm, cool, earth, bold, muted
}

pub enum ColorPreference {
    Warm, Cool, Earth, Bold, Muted, Monochrome,
}

pub enum OutsiderAttitude {
    Welcoming,
    Curious,
    Indifferent,
    Suspicious,
    Hostile,
    Xenophilic,       // loves outsiders, fetishizes foreignness
    Isolationist,
}

pub enum FactionAllegiance {
    Loyal { faction_id: String, intensity: u8 },
    NominallyAligned { faction_id: String },
    Independent,
    Contested { factions: Vec<String> },
    Lawless,
}

pub enum CulturalValue {
    Honor, Community, Innovation, Tradition,
    Wealth, Knowledge, Strength, Compassion,
    Independence, Order, Freedom, Piety,
    Survival, Beauty, Efficiency, Family,
}
```

## Deliverables

### 1. Planetary generator (`core/src/generator/planet_extended.rs`)

- [ ] `generate_planet_extended(seed, system_params, faction_map) -> PlanetExtended` —
      pure function. Generates full planetary parameters from seed.
      Physical parameters are derived from the seed and system type
      (inner planets = smaller, rocky, hot; outer planets = larger,
      gas-rich, cold). Habitability score determines whether humans
      can survive without technology.
- [ ] Settlement generation: populated planets get 1-N settlements.
      Capital worlds get 1 megacity + satellite cities. Frontier worlds
      get 1-3 outposts. Settlement placement is deterministic from seed +
      resource locations — cities grow where resources are.
- [ ] District generation: each settlement has districts proportional to
      its size and economic focus. A mining colony has industrial +
      residential. A capital has all district types. Districts contain
      stations (docking points) and notable NPC locations.
- [ ] Population scaling: population is derived from settlement age,
      habitability, and economic focus. A 100-year-old mining colony on
      a moderately habitable world = ~500K people. A 5-year-old outpost
      on a hostile world = ~200 people.
- [ ] Determinism: add to determinism manifest. Same seed = same planet
      at every level of detail.

### 2. Culture generator (`core/src/generator/culture.rs`)

- [ ] `generate_culture(planet_seed, planet_params, settlements, faction_map) -> PlanetCulture` —
      derives culture from planetary conditions + settlement history +
      faction influence. The causal chain is explicit and testable:
      - Atmosphere requires suits → clothing is purely functional,
        architectural style is enclosed
      - Low gravity → architecture is tall and spindly, population
        develops lower bone density (cultural adaptation)
      - Isolationist faction rule → outsiders are suspicious
      - First Wave settlement, 100+ years → deep local traditions,
        language heavily drifted
      - Dangerous ecosystem (S39) → survival is a core cultural value,
        customs around safety rituals
- [ ] Cultural coherence: the generator ensures cultural attributes are
      consistent. A "warm and welcoming" attitude toward outsiders
      combined with "isolationist" social structure is contradictory —
      the generator resolves by picking the dominant influence (faction
      allegiance trumps ecosystem pressure).
- [ ] Language generation: base language + drift intensity + ecosystem
      loanwords produces the accent name and unique terms. Greeting and
      farewell are generated from cultural values. "May the rock hold you"
      comes from a mining culture on a geologically unstable world.
- [ ] Visual differentiation: architectural style produces a palette and
      shape vocabulary. The city visual in Landed mode uses these
      parameters. The spaceport landing screen shows architecture in the
      background. NPC clothing uses clothing style colors and materials.

### 3. City rendering (`client/src/systems/city_view.rs`)

- [ ] When docked at a city station, the background/environment reflects
      the city's architectural style. Not a full 3D city — a stylized
      skybox/canvas generated from architectural parameters. "Brutalist
      domes under a red sky." "Organic spires reaching through methane
      fog." "Subterranean tunnels lit by warm artificial light."
- [ ] District labels: the landed mode's area transitions show district
      names. "Entering the Starport District." "Now in the Warrens
      (Industrial District)." Changing districts changes the visual
      backdrop.
- [ ] NPCs in the station reflect the planet's culture: clothing colors,
      greeting text, attitude. A suspicious NPC from an isolationist
      world is terse. A welcoming NPC from a trade hub is chatty.
      Generated from the planet's culture parameters.

### 4. Cultural interaction system

- [ ] Custom recognition: the player's dialogue system (S16) can reference
      local customs. "You offer the traditional Verdant greeting: 'May the
      rock hold you.' The station master relaxes visibly." Using local
      customs improves NPC disposition. Ignoring them (using your own
      culture's greeting) is neutral. Violating a taboo damages it.
- [ ] Cultural knowledge: the player learns cultural traits by interacting
      with a planet's NPCs, reading the discovery log, or researching the
      planet. Known traits appear in the planet info panel. Unknown traits
      are hidden until discovered.
- [ ] Cultural gaffes: the player can accidentally violate a custom.
      "You try to bargain. On this world, bargaining is taboo — it implies
      the other party is dishonest. The merchant's face hardens." The
      outcome: reputation loss, higher prices, refused service. The player
      learns through error. The game doesn't prevent cultural mistakes.

### 5. Content authoring overrides

- [ ] Any field on `PlanetCulture` can be authored in a `.ron` override
      file. Override the greeting on Aethon. Override the architectural
      style on Kessel. Override the entire culture on Earth (if it exists).
      The generator fills everything not explicitly overridden.
- [ ] Settlement authoring: specific cities can be authored. "Sorrow
      Station's host city, Verdant Landing, has these specific districts
      and this specific history." The generator fills the rest of the
      planet's settlements.
- [ ] Cultural event injection: authored events that change a planet's
      culture over time. "After the Veil crisis, Verdant Landing's
      attitude toward outsiders shifts from Welcoming to Suspicious."
      Triggered by story arcs, faction events, or player actions.

### 6. Metrics collection

- [ ] `planet_generation_events` table: planet complexity distribution,
      culture attribute correlations, settlement counts, population
      distribution.
- [ ] Research questions: what cultural attribute combinations produce
      the most player interaction? Do players seek out specific cultural
      types? Does cultural coherence correlate with planet visit duration?
      How often do players reference local customs?

## Acceptance gates

```
cargo test -p reachlock-core generator::planet_extended generator::culture::
# generation deterministic, culture coherence rules, settlement logic
cargo test -p reachlock-core determinism::  # planet/culture goldens
make check
```

Manual: fly to a populated planet → scan it → read the culture summary →
dock at the main city → observe the district names and visual backdrop →
greet an NPC with the local custom → see their disposition improve →
accidentally violate a taboo → see consequences → travel to a different
planet → observe that the culture is distinct and coherently different.

## Non-goals

- Full 3D city exploration (walking city streets). The landed mode is
      station interiors and immediate surroundings. City view is a
      backdrop + district labels. Full city exploration is a future
      expansion.
- City simulation (population growth, district expansion, economic
      development over time). Cities are static per seed. Changes
      are event-driven (authored events, faction wars).
- Political simulation (elections, unrest, revolution). Faction
      allegiance is fixed per planet. Political change is event-driven.
- NPC daily schedules based on culture. NPCs have soul files and
      contracts. Cultural behavior is expressed through dialogue and
      disposition, not simulated routines.

## Gotchas

- Cultural coherence rules must be defined as a validation pass, not
      enforced during generation. The generator produces a candidate
      culture. The coherence checker identifies contradictions and
      re-rolls conflicting attributes. This is simpler than trying to
      generate coherent output in one pass. Document the coherence rules
      in the module docs.
- Settlement population must produce believable numbers. A city of 500K
      on a world with a single airlock-sized starport doesn't make sense.
      Starport size must correlate with population. Validation rule:
      starport_size >= population.log10() - 2.
- Language drift must not produce offensive or nonsensical output. The
      greeting and farewell are generated from templates, not free-form
      LLM output. This is a core generator — it's deterministic templates,
      not LLM. Same approach as item names (S05).
- The cultural interaction system (customs, taboos) must not overwhelm
      the player. The planet info panel shows known customs upfront.
      The player can read them before interacting. Taboo violation requires
      an action the player initiates — the game doesn't punish the player
      for a custom they couldn't have known about.
