# S45 — Ship Room Upgrades

**Spec:** §19 (ship editor interior) · **Wave 9 (Living Galaxy) ·
Depends on:** S18 (ship interior editor), S05 (items), S44 (advanced goods)

## Outcome

The ship is a character that levels up. Every room has upgrade slots. Widgets
— purchased, found, or crafted — slot into rooms and provide stat bonuses.
A hydroponics bay with a basic grow bed is functional; with an Aeroponic
Array upgrade, it produces 3x the food. A med bay with an Auto-surgery Suite
heals crew 5x faster. Room types can be repurposed: your cargo hold becomes
a second med bay, your workshop becomes an ore processor. The ship IS your
character sheet — every room, every widget, every upgrade choice is a
statement about what kind of captain you are.

## Context

- S18 provides the interior editor: grid-based room placement from templates.
  Rooms have types (Cockpit, MedBay, Engineering, etc.) and furniture slots.
  This sprint extends furniture slots into UPGRADE SLOTS — widgets with
  gameplay effects.
- S05 provides the item generator. Widgets ARE items — they have seeds,
  tiers, rarities, stats. They generate from the same pipeline. A "Med Bay
  Upgrade - Advanced Auto-surgery Suite" is a GeneratedItem with
  `ItemFamily::RoomUpgrade` and stats that map to med bay bonuses.
- S44 provides the economy. Widgets are produced at specific station types,
  traded as goods, priced by the market. High-tier widgets are rare and
  expensive. Finding one in a derelict ship is a treasure moment.
- Room repurposing is a shipyard service. Pay credits + wait N ticks →
  room type changes. The interior layout recalculates. Crew duty rooms
  remap. The onboard mode reflects the change.

## Freeze first

### Room upgrade types (`item/types.rs` extension)

```rust
// New ItemFamily variant
pub enum ItemFamily {
    // ... existing ...
    RoomUpgrade(RoomUpgradeKind),
}

pub enum RoomUpgradeKind {
    // Med Bay
    MedStation,       // basic → improved → advanced → experimental
    PharmacyLocker,
    TriageBed,
    SurgerySuite,

    // Engineering
    ReactorConsole,
    RepairBench,
    ComponentStorage,
    OverclockModule,

    // Hydroponics
    GrowBed,
    IrrigationSystem,
    NutrientProcessor,
    AeroponicArray,

    // Cargo Hold
    CargoExpansion,
    RefrigerationUnit,
    SecureVault,
    HiddenCompartment,     // contraband concealment (S43)

    // Crew Quarters
    Bunk,
    RecreationModule,
    PrivacyScreen,
    LuxurySuite,

    // Galley
    GalleyUnit,
    FoodProcessor,
    EntertainmentSystem,
    ChefStation,

    // Workshop
    Workbench,
    NanoForge,
    MaterialAnalyzer,
    PrototypePrinter,

    // Cockpit / Bridge
    NavComputer,
    SensorArray,
    CommRelay,
    TargetingComputer,

    // Armory
    WeaponRack,
    ArmorLocker,
    AmmoFabricator,
    CombatSimulator,

    // Science Lab
    SampleAnalyzer,
    IsolationChamber,
    ResearchDatabase,
    PredecessorInterface,
}

pub struct RoomUpgradeStats {
    pub room_kind: RoomKind,        // which room type this fits
    pub slot_type: UpgradeSlotType,
    pub stat_bonuses: Vec<StatBonus>,
    pub power_draw: i64,            // energy consumption increase
    pub install_ticks: u64,         // how long to install
    pub visual: UpgradeVisual,
}

pub enum UpgradeSlotType {
    Primary,         // one per room — the main function upgrade
    Secondary,       // two per room — supporting upgrades
    Utility,         // one per room — quality of life upgrade
}

pub struct StatBonus {
    pub stat: StatKey,              // from S05's stat system
    pub value: i64,                 // fixed-point
    pub is_percentage: bool,        // additive or multiplicative?
}
```

### Room repurposing (`editor/interior.rs` extension)

```rust
pub struct RoomRepurpose {
    pub room_index: usize,
    pub from_kind: RoomKind,
    pub to_kind: RoomKind,
    pub credit_cost: u64,
    pub tick_duration: u64,
    pub material_requirements: Vec<(String, u32)>,  // (good_id, quantity)
}

impl ShipInteriorLayout {
    pub fn repurpose_room(&mut self, repurpose: RoomRepurpose) -> Result<(), RepurposeError>;
}
```

## Deliverables

### 1. Room upgrade widget items (`content/economy/widgets.ron` + generator)

- [ ] 20-30 widget items covering all upgrade kinds. Tiers: Basic → Improved
      → Advanced → Experimental → Predecessor. Each tier: better stats,
      higher cost, rarer. Predecessor-tier widgets are relic items found
      only in deep space ruins — not purchasable.
- [ ] Widget item generation via S05's `generate_item()` with
      `ItemFamily::RoomUpgrade(kind)`. Stat bands defined in S05's
      `item/stats.rs` extension. Rarity weights skewed: Basic=Common,
      Improved=Uncommon, Advanced=Rare, Experimental=Epic, Predecessor=
      Legendary.
- [ ] Widget production: specific station types produce specific widget
      tiers. Shipyards produce ship components. Research stations produce
      science lab upgrades. Pirate havens produce black market widgets
      (hidden compartments, combat stimulators).

### 2. Room upgrade slot system (`editor/interior.rs` extension)

- [ ] Rooms defined in S18's `RoomTemplate` get an `upgrade_slots` field:
      `Vec<UpgradeSlotType>`. Each template defines which slots its rooms
      have. Med Bay: Primary (SurgerySuite), Secondary×2, Utility. Cargo
      Hold: Primary (CargoExpansion), Secondary×2, Utility.
- [ ] `install_upgrade(layout, room_idx, slot_type, widget_item) -> ShipInteriorLayout` —
      installs a widget into a room's slot. Validates: widget's `room_kind`
      matches the room's kind, slot type is available, no duplicate widget
      in the same slot. Returns the updated layout.
- [ ] `remove_upgrade(layout, room_idx, slot_type) -> ShipInteriorLayout` —
      removes a widget. Returns the widget item to inventory. Downtime:
      removal is instant, but re-installing takes time.
- [ ] Stat aggregation: `compute_room_bonuses(layout, widgets) -> RoomBonuses` —
      sums all installed widget bonuses per stat. Additive bonuses sum.
      Percentage bonuses multiply. "Med Bay: +20 heal_rate, ×1.5 heal_speed."
- [ ] Power draw: each widget consumes power. Total ship power budget is
      determined by the reactor (engineering room widget). Installing too
      many high-power widgets without upgrading the reactor → power
      deficit → some widgets run at reduced effectiveness.

### 3. Room repurposing system

- [ ] Shipyard service: at a station with shipyard facilities, the player
      can repurpose a room. Select room → select new room type → see cost
      (credits + materials + ticks) → confirm → room enters "refit" state.
- [ ] Refit state: room is unusable during refit. Crew duty assignments
      remap. The interior visually shows "Under Construction." Refit
      completes after `tick_duration` ticks. Player can be away (the
      clock runs).
- [ ] Compatibility: not all room types can go anywhere. The Cockpit must
      remain in its original position (hull structural element). Airlocks
      must remain on exterior walls. Other rooms can be freely repurposed
      within the hull grid.
- [ ] Refit cost: credits = base_cost × room_size × tier_difference (going
      from basic Quarters to Luxury Quarters costs more than to Cargo Hold).
      Materials = construction components (from S44 goods). Time = base_time
      × room_size.

### 4. Ship upgrade management UI

- [ ] Upgrade panel: accessible from the ship interior (OnBoard mode) and
      docked station interface. Shows the ship layout with each room's
      upgrade slots. Click a room → shows installed widgets, empty slots,
      and available widgets (from inventory + station shop).
- [ ] Widget comparison: hover a widget to see its stats vs the currently
      installed widget. "Aeroponic Array: +30 food production, -5 power.
      VS Currently Installed: Improved Grow Bed: +15 food production,
      -3 power. UPGRADE: +15 food, -2 power." Color-coded: green for
      better, red for worse.
- [ ] Power budget indicator: total power consumed / total power available.
      Shows which widgets are suffering power deficit. "Warning: 3 widgets
      running at 75% effectiveness due to insufficient power. Upgrade your
      reactor."
- [ ] Room repurpose UI: select room → dropdown of valid target room types →
      see cost breakdown → see what widgets will be lost (those incompatible
      with the new room type) → confirm.
- [ ] All changes are preview-only until applied. Apply = credits deducted,
      materials consumed, refit timer starts.

### 5. Visual changes on the ship

- [ ] Installed widgets change the room's visual representation in OnBoard
      mode. A Med Bay with an Auto-surgery Suite installed has a surgery
      table fixture visible. A Cargo Hold with a Hidden Compartment has
      a subtle wall panel indicator. Visual changes are authored per
      widget (S25 content editor).
- [ ] Repurposed rooms visually change. A Cargo Hold repurposed to a Galley
      shows kitchen fixtures instead of cargo crates. The transition is
      immediate on refit completion.

### 6. Metrics collection

- [ ] `room_upgrade_events` table: widgets installed/removed, rooms
      repurposed, power budget events, widget tier distribution.
- [ ] Research questions: what widgets are most commonly installed? What
      room types are most commonly repurposed? Does ship configuration
      diversity increase over time (players specialize)? What widget
      tiers do players invest most in?

## Acceptance gates

```
cargo test -p reachlock-core item::room_upgrade editor::interior::upgrades::
# widget installation/removal, stat aggregation, power budget, repurposing
make check
```

Manual: dock at a shipyard → open upgrade panel → install a Med Bay
upgrade → see healing rate improve → repurpose a cargo hold to science
lab → wait for refit → see room changed → install science lab widgets →
power budget warns reactor is overloaded → upgrade reactor → power
budget green → undock → injured crew heal faster.

## Non-goals

- Widget crafting (combining materials to create widgets). Widgets are
      purchased or found. Crafting is Phase 4.
- Widget degradation / maintenance. Widgets don't wear out. Damage from
      combat (S20) can disable rooms, which disables all widgets in that
      room, but widgets don't degrade from use.
- Visual ship exterior changes from upgrades. Interior only. Exterior
      changes are S17 (hull plating, engine mounts).
- Widget modding (sub-upgrades on widgets). A widget is a single item
      with fixed stats.

## Gotchas

- Widgets in a room that gets repurposed: incompatible widgets are removed
      and returned to inventory. The UI must warn BEFORE applying the repurpose.
      "This will remove 3 widgets: Auto-surgery Suite, Pharmacy Locker,
      Triage Bed. These will be returned to your inventory."
- Power budget math: power_draw is negative (widget consumes power).
      Reactor widgets produce positive power. Total power = sum of all
      widget power values. Deficit = widgets lose effectiveness
      proportional to (power_consumed - power_available) / power_consumed.
      At 50% deficit, all widgets run at 50% effectiveness.
- Stat bonuses must be computable without the Bevy runtime (core purity).
      `compute_room_bonuses` is a pure function in `editor/interior.rs`.
      The client reads the bonuses and applies them through ECS systems.
      Bonus application is client-side; computation is core-side.
- Widget ownership: when the player replaces a ship (new hull), widgets
      are transferred from the old ship's rooms to inventory. The player
      doesn't lose their upgrades on ship swap. Document this early so
      players know upgrades are a long-term investment.
