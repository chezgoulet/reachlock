//! Market (spec §14 Mode 1; S07). Buy/sell against a *static* per-station
//! price table (seeded ±15% around base, `reachlock_core::economy`). The
//! `PriceSource` seam is implicit here — S10 swaps `market_table` for a live
//! backend and the UI below is untouched. Credits/cargo are integers; the only
//! decimal is the one we never show (prices are whole credits).

use bevy::prelude::*;

use reachlock_core::economy::{
    apply_buy, apply_sell, buy_price, can_buy, can_sell, default_base_prices, generate_price_table,
    sell_price, GoodId, PriceTable,
};

use crate::states::CurrentLocation;
use crate::systems::interaction::ActivePanel;
use crate::systems::inventory::{save_player, PlayerInventory};

/// Derive the price table for a station. `se` shifts the seed so the market
/// table isn't identical to the hull/station tables; deterministic in `seed`.
pub fn market_table(seed: u64) -> PriceTable {
    generate_price_table(seed ^ 0x5EA17, &default_base_prices())
}

/// Selection + quantity for the keyboard market UI. A `Resource` (not `Local`)
/// so the HUD can render the same cursor.
#[derive(Resource, Default)]
pub struct MarketState {
    pub sel: usize,
    pub qty: u32,
}

/// Buy/sell from the open market panel. Keyboard: `W`/`S` (or arrows) move
/// the selection, `A`/`D` shift quantity, `B` buys, `N` sells. `Esc`
/// closes the panel (handled by `pause::toggle_pause`). Writes the save on
/// every settled trade.
pub fn market_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut inv: ResMut<PlayerInventory>,
    loc: Res<CurrentLocation>,
    panel: Res<ActivePanel>,
    mut state: ResMut<MarketState>,
) {
    if *panel != ActivePanel::Market {
        return;
    }
    let table = market_table(loc.station_seed);
    let count = table.len();
    if count == 0 {
        return;
    }

    let up = keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown);
    let left = keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft);
    let right = keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight);

    if up {
        state.sel = state.sel.wrapping_sub(1) % count;
    }
    if down {
        state.sel = (state.sel + 1) % count;
    }
    if left {
        state.qty = state.qty.saturating_sub(1).max(1);
    }
    if right {
        state.qty = state.qty.saturating_add(1);
    }

    let good = table.keys().nth(state.sel).cloned().unwrap();
    let base = table[&good];

    if keys.just_pressed(KeyCode::KeyB) {
        let held = inv.cargo.get(&good).copied().unwrap_or(0);
        if inv.can_hold(state.qty) && can_buy(inv.credits, base, state.qty) {
            let (credits, _held) = apply_buy(inv.credits, held, base, state.qty);
            inv.credits = credits;
            inv.cargo.insert(good.clone(), held + state.qty);
            save_player(&inv, &loc);
        }
    }
    if keys.just_pressed(KeyCode::KeyN) {
        let held = inv.cargo.get(&good).copied().unwrap_or(0);
        if can_sell(held, state.qty) {
            let (credits, left) = apply_sell(inv.credits, held, base, state.qty);
            inv.credits = credits;
            if left == 0 {
                inv.cargo.remove(&good);
            } else {
                inv.cargo.insert(good, left);
            }
            save_player(&inv, &loc);
        }
    }
}

/// Render the market panel text (called by `hud::update_hud`).
pub fn market_panel_text(
    inv: &PlayerInventory,
    loc: &CurrentLocation,
    state: &MarketState,
) -> String {
    let table = market_table(loc.station_seed);
    let goods: Vec<&GoodId> = table.keys().collect();
    if goods.is_empty() {
        return "MARKET (no goods)".to_string();
    }
    let sel = state.sel.min(goods.len() - 1);
    let mut lines = vec![
        "── MARKET ──  W/S select · A/D qty · B buy · N sell".to_string(),
        format!(
            "credits: {}   cargo: {}/{}",
            inv.credits,
            inv.cargo_units(),
            inv.capacity
        ),
    ];
    for (i, g) in goods.iter().enumerate() {
        let base = table[*g];
        let held = inv.cargo.get(*g).copied().unwrap_or(0);
        let marker = if i == sel { "> " } else { "  " };
        lines.push(format!(
            "{}{:>8}  buy {}  sell {}  have {}",
            marker,
            g.0,
            buy_price(base),
            sell_price(base),
            held
        ));
    }
    lines.push(format!("qty: {}", state.qty));
    lines.join("\n")
}
