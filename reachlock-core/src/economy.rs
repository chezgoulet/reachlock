//! Static market math (spec §14 Mode 1; S07 freeze). Pure functions, no
//! IO, no Bevy — `make check` runs these on every target. Prices are
//! integers (the only place a decimal may appear is display formatting).
//!
//! S07 ships a *static* per-station price table; S10 swaps the backend.
//! The client's `PriceSource` trait (see `reachlock-client::systems::market`)
//! is the seam so the UI never changes when the economy gains teeth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::util::rng::SeededRng;

/// String newtype for a trade-good id (S07 freeze). S10 attaches real
/// goods definitions to this; until then it's just a stable key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GoodId(pub String);

/// Static per-station price table: good → base credits.
pub type PriceTable = BTreeMap<GoodId, i64>;

/// A few starter goods with base prices, so every station has something to
/// trade before S10's catalogue lands. Base prices are the *mid* price;
/// buy is above, sell is below.
pub fn default_base_prices() -> PriceTable {
    [
        ("water", 10),
        ("food", 25),
        ("fuel", 40),
        ("alloy", 80),
        ("medicine", 140),
        ("luxury", 320),
    ]
    .into_iter()
    .map(|(id, p)| (GoodId(id.into()), p))
    .collect()
}

/// Seed a price table by jittering each base price ±15% (spec §14: "seeded
/// ±15% around base"). Deterministic in `seed` — no manifest entry needed.
pub fn generate_price_table(seed: u64, base: &PriceTable) -> PriceTable {
    let mut rng = SeededRng::new(seed ^ 0xEC070);
    base.iter()
        .map(|(good, &b)| {
            let delta = (b * 15 / 100).max(1) as u64;
            let off = rng.next_below(delta * 2 + 1) as i64 - delta as i64;
            let price = (b + off).max(1);
            (good.clone(), price)
        })
        .collect()
}

/// Seam the market UI talks to. S07 ships the static backend; S10 will
/// provide a live exchange behind the same trait so the UI (and the client
/// `market.rs` system) never changes when the economy gains teeth.
pub trait PriceSource {
    /// Prices for a station, deterministic in `seed`.
    fn price_table(&self, seed: u64) -> PriceTable;
}

/// S07's static backend: seeded ±15% jitter around the base catalogue
/// (see `generate_price_table`). The default market `PriceSource`.
pub struct StaticPriceSource;

impl PriceSource for StaticPriceSource {
    fn price_table(&self, seed: u64) -> PriceTable {
        generate_price_table(seed ^ 0x5EA17, &default_base_prices())
    }
}

/// Price the player *pays* to buy one unit (slightly above base).
pub fn buy_price(base: i64) -> i64 {
    ((base * 110 + 50) / 100).max(1)
}

/// Price the player *receives* to sell one unit (slightly below base).
pub fn sell_price(base: i64) -> i64 {
    ((base * 90) / 100).max(1)
}

/// Can the player afford `qty` units at `base`?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_costs_more_than_sell() {
        assert!(buy_price(100) > sell_price(100));
        assert_eq!(buy_price(100), 110);
        assert_eq!(sell_price(100), 90);
    }

    #[test]
    fn cannot_buy_without_credits() {
        assert!(!can_buy(50, 100, 1));
        assert!(can_buy(110, 100, 1));
    }

    #[test]
    fn apply_buy_moves_credits_and_cargo() {
        let (c, q) = apply_buy(200, 0, 100, 2);
        assert_eq!((c, q), (200 - 220, 2));
    }

    #[test]
    fn cannot_sell_what_you_lack() {
        assert!(!can_sell(0, 1));
        assert!(can_sell(3, 2));
    }

    #[test]
    fn apply_sell_moves_credits_and_cargo() {
        let (c, q) = apply_sell(0, 5, 100, 3);
        assert_eq!((c, q), (270, 2));
    }

    #[test]
    fn price_table_jitter_is_seeded_and_bounded() {
        let base = default_base_prices();
        let a = generate_price_table(99, &base);
        let b = generate_price_table(99, &base);
        assert_eq!(a, b, "deterministic in seed");
        for (good, price) in &a {
            let mid = base[good];
            let delta = (mid * 15 / 100).max(1);
            assert!(
                (mid - delta..=mid + delta).contains(price),
                "{good:?} {price} outside ±15% of {mid}"
            );
        }
    }

    #[test]
    fn price_table_differs_by_seed() {
        let base = default_base_prices();
        let a = generate_price_table(1, &base);
        let b = generate_price_table(2, &base);
        assert_ne!(a, b);
    }
}
