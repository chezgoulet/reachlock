//! REACHLOCK mode 1 economy (spec §14). Pure, IO-free, wasm-safe.
//!
//! Two layers:
//!   1. Authored catalogue — [`Good`] + [`GoodsCatalog`], the static
//!      reference data (base price, mass, category, legality) authored as
//!      `content/economy/goods.ron` and validated by the CLI.
//!   2. Live runtime — [`EconomyState`] + [`StationEconomy`], the seeded,
//!      ticking market each station instance runs. `tick` nudges prices
//!      toward base and toward last-trade pressure (mean reversion + a
//!      memoryless drift term); `price` reads the current mid, buy, and
//!      sell quotes behind the client's `PriceSource` seam so the market UI
//!      never changes when the economy gains teeth.
//!
//! Gameplay money stays integer credits; `tariff_numer` (fixed-point
//! 1/1024, 1024 == 1.0) is the only fractional knob and it's applied at
//! quote time, not stored in the economy. No floats in any persisted value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::util::rng::SeededRng;

/// Fixed-point scale for the tariff multiplier. `1024` reads as `1.0`
/// (no tariff). Tariffs are introduced by faction standing (S11); the
/// knob exists here so the seam is stable.
pub const TARIFF_ONE: i64 = 1024;

/// String newtype for a trade-good id. S07 froze this; S10 attaches the
/// real [`Good`] definition keyed by it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GoodId(pub String);

/// Static per-station price table: good → current mid price (credits).
pub type PriceTable = BTreeMap<GoodId, i64>;

/// Seam the market UI talks to. S07 shipped the static backend; S10 provides
/// the live [`EconomyState`] behind the same trait so the UI (and the client
/// `market.rs` system) never changes when the economy gains teeth.
pub trait PriceSource {
    /// Prices for a station, deterministic in `seed`.
    fn price_table(&self, seed: u64) -> PriceTable;
}

/// A trade good's static reference data (authored).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Good {
    /// Stable id, e.g. `"water"`. Also the map key in [`GoodsCatalog`].
    pub id: GoodId,
    /// Human label for market UI.
    pub name: String,
    /// Reference mid-price in credits (the "base" every quote pivots on).
    pub base_price: i64,
    /// Mass per unit in arbitrary cargo units (drives cargo capacity, S08).
    pub mass: i64,
    /// Coarse bucket for UI grouping / legality rules.
    pub category: GoodCategory,
    /// Whether selling it to a core-lawful station is a crime (S11+).
    #[serde(default)]
    pub contraband: bool,
}

/// Buckets goods fall into. Authored; used by UI and by future legality
/// and tariff tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodCategory {
    Consumable,
    Fuel,
    Material,
    Manufactured,
    Medical,
    Luxury,
    Contraband,
}

impl GoodCategory {
    /// Every variant, for exhaustive iteration (catalog completeness checks).
    pub const ALL: [GoodCategory; 7] = [
        GoodCategory::Consumable,
        GoodCategory::Fuel,
        GoodCategory::Material,
        GoodCategory::Manufactured,
        GoodCategory::Medical,
        GoodCategory::Luxury,
        GoodCategory::Contraband,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            GoodCategory::Consumable => "consumable",
            GoodCategory::Fuel => "fuel",
            GoodCategory::Material => "material",
            GoodCategory::Manufactured => "manufactured",
            GoodCategory::Medical => "medical",
            GoodCategory::Luxury => "luxury",
            GoodCategory::Contraband => "contraband",
        }
    }
}

/// The authored set of goods. Serialized as `content/economy/goods.ron`
/// and validated by `reachlock content validate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoodsCatalog {
    /// Bump when the catalogue shape changes in a way old saves can't read.
    pub version: u32,
    /// Goods keyed by id (BTreeMap keeps deterministic ordering).
    pub goods: BTreeMap<GoodId, Good>,
}

impl GoodsCatalog {
    /// Look up a good by id.
    pub fn get(&self, id: &GoodId) -> Option<&Good> {
        self.goods.get(id)
    }

    /// Every good id, sorted (deterministic iteration for tests/manifest).
    pub fn ids(&self) -> Vec<GoodId> {
        self.goods.keys().cloned().collect()
    }

    /// Authoring-time check: ids unique (guaranteed by the map), base
    /// prices positive, mass positive, categories known. Returns a list of
    /// human-readable problems (empty == clean).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.version == 0 {
            errors.push("catalogue version must be >= 1".into());
        }
        for (id, good) in &self.goods {
            if good.base_price <= 0 {
                errors.push(format!("good {} has non-positive base_price", id.0));
            }
            if good.mass <= 0 {
                errors.push(format!("good {} has non-positive mass", id.0));
            }
            if good.contraband && good.category != GoodCategory::Contraband {
                errors.push(format!(
                    "good {} is contraband but category is {}",
                    id.0,
                    good.category.as_str()
                ));
            }
        }
        errors
    }
}

/// The live economy for one station instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationEconomy {
    /// Current mid price per good (credits). Seeded at base; drifts on tick.
    pub prices: BTreeMap<GoodId, i64>,
    /// Storage capacity per good (units on hand). Caps how much the station
    /// will buy before the price collapses.
    pub storage: BTreeMap<GoodId, i64>,
    /// Last-trade pressure accumulator per good: +buy / -sell, decays each
    /// tick. Drives short-term moves independent of the base pull.
    pub pressure: BTreeMap<GoodId, i64>,
}

impl StationEconomy {
    /// Seeded initial state from a catalogue. `seed` makes each station's
    /// starting prices + storage differ deterministically.
    pub fn new(catalog: &GoodsCatalog, seed: u64, kind: StationKind) -> Self {
        let mut rng = SeededRng::new(seed ^ 0xEC010);
        let mut prices = BTreeMap::new();
        let mut storage = BTreeMap::new();
        let mut pressure = BTreeMap::new();
        let storage_scale = kind.storage_scale();
        for (id, good) in &catalog.goods {
            // Start within ±10% of base so no station opens wildly off.
            let delta = (good.base_price * 10 / 100).max(1) as u64;
            let off = rng.next_below(delta * 2 + 1) as i64 - delta as i64;
            prices.insert(id.clone(), (good.base_price + off).max(1));
            storage.insert(
                id.clone(),
                (good.base_price * storage_scale / 100).max(good.mass),
            );
            pressure.insert(id.clone(), 0);
        }
        Self {
            prices,
            storage,
            pressure,
        }
    }

    /// Mid (fair) price for one good right now.
    pub fn mid(&self, id: &GoodId) -> i64 {
        *self.prices.get(id).unwrap_or(&0)
    }

    /// Player *pays* to buy one unit: mid plus spread, times tariff.
    pub fn buy_price(&self, id: &GoodId, tariff_numer: i64) -> i64 {
        let mid = self.mid(id);
        apply_tariff(((mid * 110 + 50) / 100).max(1), tariff_numer)
    }

    /// Player *receives* to sell one unit: mid minus spread, times tariff.
    pub fn sell_price(&self, id: &GoodId, tariff_numer: i64) -> i64 {
        let mid = self.mid(id);
        apply_tariff((mid * 90 / 100).max(1), tariff_numer)
    }

    /// Advance the market one step. Mean-reverts prices toward base and
    /// bleeds off trade pressure. Pure in (`self`, `catalog`, `seed`);
    /// deterministic given equal inputs.
    pub fn tick(&mut self, catalog: &GoodsCatalog, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0x71C0);
        for (id, good) in &catalog.goods {
            let mid = *self.prices.get(id).unwrap_or(&good.base_price);
            let press = *self.pressure.get(id).unwrap_or(&0);

            // Pressure pulls the mid; base pulls it back. Both bounded so a
            // single tick can't run away.
            let pull = ((good.base_price - mid) * 5 / 100).clamp(-good.base_price, good.base_price);
            let push = (press * 3 / 100).clamp(-good.base_price, good.base_price);
            let drift = (rng.next_below(3) as i64 - 1) * (good.base_price / 200).max(1);
            let next = (mid + pull + push + drift)
                .clamp(1, good.base_price * 4)
                .max(1);

            self.prices.insert(id.clone(), next);
            // Pressure decays toward zero a little each tick.
            let new_press = press * 90 / 100;
            self.pressure.insert(id.clone(), new_press);
        }
    }

    /// Record a trade so subsequent ticks reflect it. `qty` is signed:
    /// positive = player bought from the station (price pressure up),
    /// negative = player sold (pressure down). Ignored if `qty == 0`.
    pub fn record_trade(&mut self, id: &GoodId, qty: i64) {
        if qty == 0 {
            return;
        }
        let entry = self.pressure.entry(id.clone()).or_insert(0);
        *entry += qty;
    }
}

/// Multiply a quote by a fixed-point tariff (1024 == 1.0). Rounds toward
/// the player-disadvantageous side for non-1.0 tariffs so tariffs always
/// cost *something*.
pub fn apply_tariff(price: i64, tariff_numer: i64) -> i64 {
    if tariff_numer <= 0 {
        return price;
    }
    let scaled = price * tariff_numer;
    // Ceil division for the player-facing quote.
    (scaled + TARIFF_ONE - 1) / TARIFF_ONE
}

/// What kind of station this is — drives starting storage depth. Authored
/// by the station generator (S04/S05); mirrored here as a plain enum so the
/// economy can be seeded without pulling in station structs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationKind {
    Refinery,
    Agri,
    Hub,
    Outpost,
    BlackMarket,
}

impl StationKind {
    /// Storage expressed as a percentage of base price per good. A refinery
    /// sits on materials; a hub keeps deep shelves of everything.
    fn storage_scale(&self) -> i64 {
        match self {
            StationKind::Refinery => 400,
            StationKind::Agri => 300,
            StationKind::Hub => 600,
            StationKind::Outpost => 150,
            StationKind::BlackMarket => 200,
        }
    }
}

/// The whole mode-1 economy: the catalogue plus one [`StationEconomy`] per
/// station, keyed by a string station id (deterministic order via BTreeMap).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EconomyState {
    pub catalog: GoodsCatalog,
    pub stations: BTreeMap<String, StationEconomy>,
}

impl EconomyState {
    /// Build a fresh economy: each station seeded from the catalogue with a
    /// distinct seed so their opening books differ but reproduce exactly.
    pub fn new(catalog: GoodsCatalog, station_seeds: &[(String, u64, StationKind)]) -> Self {
        let stations = station_seeds
            .iter()
            .map(|(id, seed, kind)| (id.clone(), StationEconomy::new(&catalog, *seed, *kind)))
            .collect();
        Self { catalog, stations }
    }

    /// Tick every station. `seed` is mixed per station inside
    /// [`StationEconomy::tick`], so a single global seed is fine.
    pub fn tick(&mut self, seed: u64) {
        for (i, (_, econ)) in self.stations.iter_mut().enumerate() {
            econ.tick(&self.catalog, seed.wrapping_add(i as u64 * 0x9E3779B1));
        }
    }

    /// Convenience quote for a station/good behind the client seam.
    pub fn buy_price(&self, station: &str, id: &GoodId, tariff_numer: i64) -> i64 {
        self.stations
            .get(station)
            .map(|e| e.buy_price(id, tariff_numer))
            .unwrap_or(0)
    }

    pub fn sell_price(&self, station: &str, id: &GoodId, tariff_numer: i64) -> i64 {
        self.stations
            .get(station)
            .map(|e| e.sell_price(id, tariff_numer))
            .unwrap_or(0)
    }
}

/// Price the player *pays* to buy one unit at a mid `base` (slightly above).
pub fn buy_price(base: i64) -> i64 {
    ((base * 110 + 50) / 100).max(1)
}

/// Price the player *receives* to sell one unit at a mid `base` (slightly
/// below).
pub fn sell_price(base: i64) -> i64 {
    ((base * 90) / 100).max(1)
}

/// Can the player afford `qty` units at mid `base`?
pub fn can_buy(credits: i64, base: i64, qty: u32) -> bool {
    let total = buy_price(base) * qty as i64;
    credits >= total
}

/// Apply a buy. Caller must have checked [`can_buy`] and cargo capacity.
/// Returns `(new_credits, new_cargo_qty)`.
pub fn apply_buy(credits: i64, cargo_qty: u32, base: i64, qty: u32) -> (i64, u32) {
    let total = buy_price(base) * qty as i64;
    (credits - total, cargo_qty + qty)
}

/// Can the player sell `qty` units they hold `cargo_qty` of?
pub fn can_sell(cargo_qty: u32, qty: u32) -> bool {
    cargo_qty >= qty
}

/// Apply a sell. Caller must have checked [`can_sell`].
/// Returns `(new_credits, new_cargo_qty)`.
pub fn apply_sell(credits: i64, cargo_qty: u32, base: i64, qty: u32) -> (i64, u32) {
    let total = sell_price(base) * qty as i64;
    (credits + total, cargo_qty - qty)
}

/// Starter catalogue baked into the engine so the economy works before any
/// authored `goods.ron` is loaded (and so tests have a stable fixture).
/// The authored file *overrides* this wholesale when present.
pub fn starter_catalog() -> GoodsCatalog {
    let mut goods = BTreeMap::new();
    let add = |goods: &mut BTreeMap<GoodId, Good>,
               id: &str,
               name: &str,
               base: i64,
               mass: i64,
               cat: GoodCategory,
               contraband: bool| {
        goods.insert(
            GoodId(id.into()),
            Good {
                id: GoodId(id.into()),
                name: name.into(),
                base_price: base,
                mass,
                category: cat,
                contraband,
            },
        );
    };
    add(
        &mut goods,
        "water",
        "Water",
        12,
        1,
        GoodCategory::Consumable,
        false,
    );
    add(
        &mut goods,
        "food",
        "Food Rations",
        28,
        1,
        GoodCategory::Consumable,
        false,
    );
    add(
        &mut goods,
        "fuel",
        "Reaction Fuel",
        40,
        1,
        GoodCategory::Fuel,
        false,
    );
    add(
        &mut goods,
        "alloy",
        "Alloy Plate",
        80,
        2,
        GoodCategory::Material,
        false,
    );
    add(
        &mut goods,
        "ore",
        "Raw Ferric Ore",
        22,
        3,
        GoodCategory::Material,
        false,
    );
    add(
        &mut goods,
        "medicine",
        "Medicine",
        140,
        1,
        GoodCategory::Medical,
        false,
    );
    add(
        &mut goods,
        "electronics",
        "Electronics",
        210,
        1,
        GoodCategory::Manufactured,
        false,
    );
    add(
        &mut goods,
        "machinery",
        "Machinery",
        260,
        4,
        GoodCategory::Manufactured,
        false,
    );
    add(
        &mut goods,
        "luxury",
        "Luxury Goods",
        340,
        1,
        GoodCategory::Luxury,
        false,
    );
    add(
        &mut goods,
        "art",
        "Artifact",
        520,
        1,
        GoodCategory::Luxury,
        false,
    );
    add(
        &mut goods,
        "narcotics",
        "Narcotics",
        600,
        1,
        GoodCategory::Contraband,
        true,
    );
    add(
        &mut goods,
        "weapons",
        "Arms",
        720,
        2,
        GoodCategory::Contraband,
        true,
    );
    GoodsCatalog { version: 1, goods }
}

/// The client market UI's seam. `EconomyState` is itself a `PriceSource`,
/// so swapping the S07 static backend for the live engine required no change
/// to `market.rs` beyond which resource it reads.
impl PriceSource for EconomyState {
    fn price_table(&self, seed: u64) -> PriceTable {
        // The live economy already holds the seeded, ticked prices; `seed`
        // is ignored (it only mattered for the static jitter backend). The
        // market reads the *current* book for whatever station it's at.
        let _ = seed;
        self.stations
            .values()
            .next()
            .map(|e| e.prices.clone())
            .unwrap_or_default()
    }
}

/// Load the authored goods catalogue embedded at compile time. Falls back to
/// the [`starter_catalog`] if the embedded RON ever fails to parse (it
/// shouldn't — `make check` validates it — but this keeps a release bootable
/// even if the asset is stripped).
pub fn load_goods_catalog() -> GoodsCatalog {
    match ron::from_str::<GoodsCatalog>(GOODS_CATALOG_RON) {
        Ok(cat) if cat.validate().is_empty() => cat,
        _ => starter_catalog(),
    }
}

/// Authored goods catalogue, embedded so the client and server ship identical
/// reference data without a filesystem dependency (also wasm-safe).
const GOODS_CATALOG_RON: &str = include_str!("../../content/economy/goods.ron");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tariff_identity() {
        assert_eq!(apply_tariff(100, TARIFF_ONE), 100);
    }

    #[test]
    fn tariff_raises_price() {
        // 10% tariff on 100 -> ceil(110) = 110.
        assert_eq!(apply_tariff(100, 1126), 110);
    }

    #[test]
    fn station_seeded_deterministic() {
        let cat = starter_catalog();
        let a = StationEconomy::new(&cat, 12345, StationKind::Hub);
        let b = StationEconomy::new(&cat, 12345, StationKind::Hub);
        assert_eq!(a, b, "same seed => same opening book");
        let c = StationEconomy::new(&cat, 99999, StationKind::Hub);
        assert_ne!(a, c, "different seed => different book");
    }

    #[test]
    fn station_prices_start_near_base() {
        let cat = starter_catalog();
        let e = StationEconomy::new(&cat, 7, StationKind::Refinery);
        for (id, good) in &cat.goods {
            let p = e.mid(id);
            let delta = (good.base_price * 10 / 100).max(1);
            assert!(
                (good.base_price - delta..=good.base_price + delta).contains(&p),
                "{} opened at {p}, outside ±10% of {}",
                id.0,
                good.base_price
            );
            // Buy quote >= sell quote, both positive.
            assert!(e.buy_price(id, TARIFF_ONE) >= e.sell_price(id, TARIFF_ONE));
            assert!(e.sell_price(id, TARIFF_ONE) >= 1);
        }
    }

    #[test]
    fn tick_is_stable_and_bounded() {
        let cat = starter_catalog();
        let mut a = StationEconomy::new(&cat, 42, StationKind::Hub);
        let mut b = StationEconomy::new(&cat, 42, StationKind::Hub);
        for step in 0..50 {
            a.tick(&cat, step);
            b.tick(&cat, step);
        }
        assert_eq!(a, b, "tick is deterministic");
        for (id, good) in &cat.goods {
            let p = a.mid(id);
            assert!((1..=good.base_price * 4).contains(&p), "{p} out of bounds");
        }
    }

    #[test]
    fn trade_pressure_moves_then_decay() {
        let cat = starter_catalog();
        let mut e = StationEconomy::new(&cat, 1, StationKind::Hub);
        let id = GoodId("water".into());
        let before = e.mid(&id);
        // Player sells a lot -> pressure negative -> mid should drop after
        // a tick that lets the push act.
        e.record_trade(&id, -100);
        e.tick(&cat, 2);
        let after = e.mid(&id);
        assert!(after <= before, "selling pressure should not raise price");
        // Pressure decays: a quiet tick series returns toward base.
        for step in 10..60 {
            e.tick(&cat, step);
        }
        let settled = e.mid(&id);
        let base = cat.get(&id).unwrap().base_price;
        assert!((settled - base).abs() <= base, "decayed back toward base");
    }

    #[test]
    fn economy_state_multistation_deterministic() {
        let cat = starter_catalog();
        let seeds = vec![
            ("hub-1".into(), 11u64, StationKind::Hub),
            ("ref-1".into(), 22u64, StationKind::Refinery),
        ];
        let a = EconomyState::new(cat.clone(), &seeds);
        let b = EconomyState::new(cat.clone(), &seeds);
        assert_eq!(a, b);
        let mut a = a;
        a.tick(5);
        let mut b = b;
        b.tick(5);
        assert_eq!(a, b, "global tick deterministic across stations");
    }

    #[test]
    fn starter_catalog_validates_clean() {
        assert!(starter_catalog().validate().is_empty());
    }

    #[test]
    fn catalog_rejects_bad_base() {
        let mut cat = starter_catalog();
        cat.goods
            .get_mut(&GoodId("water".into()))
            .unwrap()
            .base_price = 0;
        assert!(!cat.validate().is_empty());
    }
}
