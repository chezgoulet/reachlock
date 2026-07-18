# S44 — Advanced Economy: Goods

**Spec:** New (expanded economy) · **Wave 9 (Living Galaxy) ·
Depends on:** S10 (economy engine), S05 (item generator)

## Outcome

The economy goes beyond raw materials. Luxury goods, clothes, cybernetics,
weapons, exploration tools, ship components — each with production chains,
consumption patterns, and trade flows driven by universe state. A war zone
consumes weapons; wealthy stations consume luxuries; frontier systems consume
exploration tools. Players can invest in production infrastructure for
passive income, corner markets through trade, or exploit blockades through
smuggling. The economy is a living system the player can read, predict, and
profit from.

## Context

- S10 established the base economy: goods catalog, per-station supply/demand,
  tick-driven price recomputation, faction tariffs. This sprint adds DEPTH
  to the catalog and SOPHISTICATION to the economy model.
- Goods are authored content (`.ron` files, validated by S01). Each good has
  a category, base price, production stations, consumption stations, weight,
  rarity, and now: production chain inputs, luxury tier, legality per
  faction, and trade bonuses.
- Production chains mean goods are not independent — raw ferrite becomes
  refined ferrite becomes ship hull plates becomes a ship. Each step adds
  value. Disrupting a step upstream affects everything downstream. This is
  an economy you can sabotage, invest in, and profit from.
- Ship components as goods means the ship upgrade ecosystem (S45) plugs
  into the economy. Upgrades are purchased from stations that produce them.
  Rare upgrades are found in deep space, on derelicts, or crafted from
  rare materials.

## Freeze first

### Expanded goods catalog (`economy/goods.rs`)

```rust
pub struct Good {
    pub id: String,
    pub name: String,
    pub category: GoodCategory,
    pub base_price: u64,
    pub weight: Fixed,
    pub rarity: Rarity,
    pub description: String,
    pub production: Vec<ProductionSource>,
    pub consumption: Vec<ConsumptionSink>,
    pub production_chain: Option<ProductionChain>,
    pub luxury_tier: Option<u8>,        // 1-5 — higher = more expensive, richer consumer
    pub legality: HashMap<String, LegalityStatus>,  // per faction
    pub trade_bonuses: Vec<TradeBonus>,
}

pub enum GoodCategory {
    // Raw Materials (from S10)
    RawMineral,
    RawOrganic,
    RawEnergy,

    // Refined / Manufactured
    RefinedMetal,
    RefinedOrganic,
    ManufacturedComponent,
    ElectronicComponent,

    // Consumer Goods
    LuxuryGood,
    Clothing,
    Cybernetic,
    Weapon,
    MedicalSupply,
    Foodstuff,

    // Industrial / Specialized
    ExplorationTool,
    ShipComponent,
    StationModule,
    ResearchEquipment,

    // Illegal
    Contraband,
}

pub struct ProductionSource {
    pub station_type: StationType,
    pub production_rate: f64,         // units per tick
    pub production_cost: u64,         // credits per unit to produce
    pub inputs: Vec<ProductionInput>, // raw materials consumed
}

pub struct ProductionInput {
    pub good_id: String,
    pub quantity: f64,                // units consumed per unit produced
}

pub struct ConsumptionSink {
    pub station_type: StationType,
    pub consumption_rate: f64,        // units per tick
    pub consumption_elasticity: f64,  // 0.0 = fixed, 1.0 = fully price-responsive
}

pub struct ProductionChain {
    pub chain_id: String,
    pub steps: Vec<ProductionChainStep>,
}

pub struct ProductionChainStep {
    pub good_id: String,
    pub inputs: Vec<(String, f64)>,   // (good_id, quantity)
    pub output_quantity: f64,
    pub production_time_ticks: u64,
}

pub enum LegalityStatus {
    Legal,
    Restricted { license_required: String },  // needs a license to trade
    Contraband,                                // illegal to possess
}

pub struct TradeBonus {
    pub condition: TradeBonusCondition,
    pub bonus_type: TradeBonusType,
    pub magnitude: f64,
}

pub enum TradeBonusCondition {
    RouteBetween { faction_a: String, faction_b: String },
    DuringEvent { event_type: String },
    WithCareerRank { path_type: PathType, min_rank: u8 },
    UnderBlockade,
    Smuggled,
}

pub enum TradeBonusType {
    PriceMultiplier,
    DemandMultiplier,
    ReputationGain,
    CareerProgress,
}
```

### Infrastructure investment

```rust
pub struct PlayerInfrastructure {
    pub investments: Vec<InfrastructureInvestment>,
}

pub struct InfrastructureInvestment {
    pub station_id: String,
    pub production_good_id: String,
    pub shares_owned: f64,            // 0.0-1.0 — fraction of production capacity
    pub invested_credits: u64,
    pub dividend_rate: f64,           // credits per tick per share
    pub total_dividends_earned: u64,
}
```

## Deliverables

### 1. Expanded goods catalog (`content/economy/`)

- [ ] 30-50 authored goods covering all categories. Minimum per category:
      - Raw: 6 types of minerals, 4 organic, 2 energy
      - Refined: refined ferrite, refined titanium, synthetic fibers, polymers
      - Components: microprocessors, hull plate sections, engine coils,
        shield emitters, sensor arrays, life support modules
      - Luxury: fine wines, gemstones, artwork, rare spices, designer clothes
      - Clothing: civilian wear, environmental suits, faction uniforms,
        armored vests
      - Cybernetics: neural interface, muscle augmentation, sensory enhancement,
        medical implant, combat chassis
      - Weapons: small arms, ship-grade weapon components, ammunition,
        targeting computers
      - Exploration: deep space scanner module, probe drone, anomaly analyzer,
        survey beacon
      - Ship components: engine upgrade kit, shield upgrade kit, weapon mod,
        hull reinforcement, cargo expansion module, hidden compartment,
        room upgrade widget (S45)
      - Contraband: precursor artifact (illegal everywhere), combat stims
        (illegal in Compact), untaxed luxury goods (illegal in ISC),
        AI core fragment (illegal in all factions except Reach)
- [ ] Each good has: production sources with inputs, consumption sinks with
      elasticity, legality map, trade bonuses.
- [ ] Validated by content pipeline. Schema update for new fields.

### 2. Production chain engine (`core/src/economy/production.rs`)

- [ ] `compute_production(station, goods_catalog, universe_state) -> ProductionOutput` —
      for each production source at a station, consumes inputs from station
      inventory, produces outputs, adds to station inventory. Input
      shortages throttle production. "Ferrite refinery has 0 raw ferrite
      → produces 0 refined ferrite."
- [ ] `compute_price(good, supply, demand, tariffs, events) -> u64` —
      extended from S10 to include: production chain depth (further
      processed = higher base value), luxury tier multiplier, legality
      multiplier (contraband = higher price), trade bonuses, faction
      tariff modifiers.
- [ ] Demand elasticity: consumption is not fixed. If the price of luxury
      goods doubles, wealthy stations consume 80% as much (elasticity 0.2).
      If the price of life support modules doubles, all stations still
      consume 98% (elasticity 0.02 — necessity). Elasticity creates
      realistic market dynamics.
- [ ] Determinism: same seed + same universe state = same production output
      and prices. Production is a pure function of the tick's state.

### 3. Player infrastructure investment

- [ ] Invest UI: at any station with production facilities, the player can
      invest credits in expanding production capacity of a specific good.
      Investment = buying shares of the station's output. Shares produce
      dividends each tick proportional to the station's profit on that good.
- [ ] ROI: dividend rate = (sell_price - production_cost) × production_rate
      × shares_owned. Rate fluctuates with market prices. A good investment
      today might be a bad one next week if the market shifts.
- [ ] Divestment: sell shares back to the station. Value = invested_credits ×
      (current station profitability / baseline profitability). You can
      lose money on a bad investment.
- [ ] Passive income: dividends accumulate in a ledger. Player collects at
      any station (transfer to credits). Or auto-deposits. Dividends are
      the reliable income stream that funds exploration and piracy.

### 4. Market analysis tools

- [ ] Price history: per-good, per-station price tracking over the last N
      ticks. Displayed as a simple sparkline in the market UI. "Refined
      ferrite: trending up 15% this cycle. War in the Veil is driving
      demand."
- [ ] Trade route finder: given the player's current location and cargo
      capacity, finds profitable trade routes. "Buy 20 refined ferrite at
      Sorrow Station (92 credits/unit), sell at Kessel Forge (147 credits/
      unit). Profit: 1100 credits. Risk: low. Distance: 2 jumps."
- [ ] Production chain viewer: select a good → see its production chain
      tree. "Refined ferrite ← Raw ferrite (mined at Mining Stations).
      Refined ferrite → Hull Plates (manufactured at Shipyards). Hull
      Plates → Ships (built at Orbital Dry Docks)." Shows the player where
      to invest or sabotage.
- [ ] Market alerts: set price thresholds per good. "Alert me when refined
      ferrite drops below 80 credits anywhere in Compact space." Alerts
      fire as comms panel notifications.

### 5. Integration with other systems

- [ ] Piracy (S43): contraband goods, smuggling routes from legality data.
      Pirate havens buy stolen goods. Black market upgrades consume
      contraband components.
- [ ] Ship upgrades (S45): ship components as goods. Purchase them, install
      them. Upgrades consume materials from the economy.
- [ ] Mission engine (S46): trade missions from the economy state. "Compact
      Shipyard needs 50 hull plates urgently — paying 2x market rate."
- [ ] Career (S42): Trade career progression from trade volume, route
      discovery, market manipulation.

### 6. Metrics collection

- [ ] Economy metrics from S10 are expanded: good-level price history,
      production chain throughput, player investment ROI, trade volume
      by good category.
- [ ] Research questions: what goods have the highest trade volume? What
      production chains are most profitable to invest in? Do players
      engage with production chain depth? What goods markets do players
      most successfully predict?

## Acceptance gates

```
cargo test -p reachlock-core economy::production:: economy::pricing::
# production chains, elasticity, investment math, trade bonuses
reachlock content validate content/economy/*.ron
make check
```

Manual: buy raw ferrite at a mining station → sell at a refinery → watch
refinery output refined ferrite → buy refined ferrite → sell at shipyard →
shipyard output hull plates → invest in the shipyard's production → receive
dividends → check price history → set a market alert → alert fires.

## Non-goals

- Full manufacturing player skill (crafting). The player invests and trades,
      not operates the factory floor.
- Stock market / speculation on goods futures. Simple buy-low-sell-high and
      investment. No derivatives.
- Player-run stations. Colonization is Phase 3+.
- Economy PvP (hostile takeovers, market cornering wars). Players can
      compete through trade but not through hostile economic actions.
- Economy balancing. The numbers ship with best-guess values. Balancing is
      Phase 4 after real player data.

## Gotchas

- Production chains can create circular dependencies if not validated.
  "A needs B, B needs A." Validate chains at content load: build the
  dependency graph, reject cycles.
- Elasticity values are authored per good. An elasticity of 0.0 means
  "consumption never changes regardless of price." A station that NEEDS
  life support will buy it at any price. This can create infinite credit
  loops if a station produces something it consumes. Validate: no station
  can be the sole producer AND consumer of the same good at different
  chain levels.
- Price history storage: tracking prices for 100 goods × 100 stations ×
  1000 ticks = 10 million data points. Cap history at 100 ticks per good
  per station. Old data rolls off.
- Player investment must survive station destruction (if stations can be
  destroyed — Phase 2 crisis events). If the invested station is destroyed,
  the investment is lost. Document this risk.
