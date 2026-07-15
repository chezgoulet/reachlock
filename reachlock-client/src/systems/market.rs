//! Market (spec §14 Mode 1; S07 → S10). Buy/sell against the live
//! [`EconomyState`] produced by `reachlock_core::economy`. The `PriceSource`
//! seam the S07 brief promised is realized here: `EconomyState` implements
//! `PriceSource`, so this UI reads the current, ticking book per station and
//! never changed its shape when the static table was replaced. Mining (S09b-2)
//! deposits `raw_ferric_ore` into `PlayerInventory.cargo`, so selling it here
//! closes the loop. Credits/cargo are integers; quotes are whole credits.

use bevy::prelude::*;

use reachlock_core::economy::{
    apply_buy, apply_sell, can_buy, can_sell, load_goods_catalog, EconomyState, GoodId, PriceTable,
    TARIFF_ONE,
};
use reachlock_core::faction::tariff as faction_tariff;

use crate::states::CurrentLocation;
use crate::systems::factions::FactionState;
use crate::systems::interaction::ActivePanel;
use crate::systems::inventory::{save_player, PlayerInventory};

/// The live economy, held as a resource. Seeded once at boot from the
/// embedded goods catalogue; the universe ticker advances it. The market
/// reads the book for whatever station the player is docked at.
#[derive(Resource)]
pub struct Economy(pub EconomyState);

/// Build the economy resource at startup. Stations are seeded from the
/// current location's station plus a couple of neighbours so the market has
/// a real, deterministic book immediately. (Station roster is fleshed out
/// by S04/S05 system generation; here we guarantee a non-empty economy.)
pub fn init_economy(mut commands: Commands) {
    let catalog = load_goods_catalog();
    let station_seeds = vec![
        (
            "home".to_string(),
            0x5EA17,
            reachlock_core::economy::StationKind::Hub,
            None,
        ),
        (
            "refinery-prime".to_string(),
            0xABCDEF,
            reachlock_core::economy::StationKind::Refinery,
            None,
        ),
        (
            "outpost-7".to_string(),
            0x13579B,
            reachlock_core::economy::StationKind::Outpost,
            None,
        ),
    ];
    let state = EconomyState::new(catalog, &station_seeds);
    commands.insert_resource(Economy(state));
}

/// Selection + quantity for the keyboard market UI. A `Resource` (not `Local`)
/// so the HUD can render the same cursor.
#[derive(Resource, Default)]
pub struct MarketState {
    pub sel: usize,
    pub qty: u32,
}

/// Derive the price table for the player's current station from the live
/// economy. Falls back to an empty table if the station isn't in the book.
pub fn market_table(economy: &Economy, station: &str) -> PriceTable {
    economy
        .0
        .stations
        .get(station)
        .map(|e| e.prices.clone())
        .unwrap_or_default()
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
    mut economy: ResMut<Economy>,
    faction_state: Res<FactionState>,
) {
    if *panel != ActivePanel::Market {
        return;
    }
    let table = market_table(&economy, &loc.station_id);
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

    // Compute tariff based on station faction and player reputation.
    let tariff_num = compute_tariff(&loc, &economy, &faction_state, &good);
    let buy_quote = economy.0.stations[&loc.station_id].buy_price(&good, tariff_num);
    let sell_quote = economy.0.stations[&loc.station_id].sell_price(&good, tariff_num);

    if keys.just_pressed(KeyCode::KeyB) {
        let held = inv.cargo.get(&good).copied().unwrap_or(0);
        if inv.can_hold(state.qty) && can_buy(inv.credits, buy_quote, state.qty) {
            let (credits, _held) = apply_buy(inv.credits, held, buy_quote, state.qty);
            inv.credits = credits;
            inv.cargo.insert(good.clone(), held + state.qty);
            if let Some(station) = economy.0.stations.get_mut(&loc.station_id) {
                station.record_trade(&good, state.qty as i64);
            }
            save_player(&inv, &loc, None);
        }
    }
    if keys.just_pressed(KeyCode::KeyN) {
        let held = inv.cargo.get(&good).copied().unwrap_or(0);
        if can_sell(held, state.qty) {
            let (credits, left) = apply_sell(inv.credits, held, sell_quote, state.qty);
            inv.credits = credits;
            if left == 0 {
                inv.cargo.remove(&good);
            } else {
                inv.cargo.insert(good.clone(), left);
            }
            if let Some(station) = economy.0.stations.get_mut(&loc.station_id) {
                station.record_trade(&good, -(state.qty as i64));
            }
            save_player(&inv, &loc, None);
        }
    }
}

/// Compute the tariff multiplier for a given station/good, factoring in the
/// controlling faction's tariff policy and the player's reputation trust.
/// Falls back to `TARIFF_ONE` (1.0) when no faction data is available.
fn compute_tariff(
    loc: &CurrentLocation,
    economy: &Economy,
    faction_state: &FactionState,
    good: &GoodId,
) -> i64 {
    let r#try = || -> Option<i64> {
        let faction_id = economy
            .0
            .stations
            .get(&loc.station_id)?
            .station_faction
            .as_ref()?;
        let faction = faction_state
            .0
            .catalog
            .factions
            .iter()
            .find(|f| f.id.0 == *faction_id)?;
        let trust = faction_state.0.rep(&faction.id).trust;
        let category = economy.0.catalog.goods.get(good)?.category;
        let demand = economy
            .0
            .stations
            .get(&loc.station_id)?
            .pressure
            .get(good)
            .copied()
            .map(|p| p.unsigned_abs() as i64)
            .unwrap_or(1024);
        Some(faction_tariff(faction, category, trust, demand))
    };
    r#try().unwrap_or(TARIFF_ONE)
}

/// Render the market panel text (called by `hud::update_hud`).
pub fn market_panel_text(
    inv: &PlayerInventory,
    loc: &CurrentLocation,
    state: &MarketState,
    economy: &Economy,
    faction_state: &FactionState,
) -> String {
    let table = market_table(economy, &loc.station_id);
    let goods: Vec<&GoodId> = table.keys().collect();
    if goods.is_empty() {
        return "MARKET (no goods)".to_string();
    }
    let sel = state.sel.min(goods.len() - 1);
    // Compute a representative tariff for the first good so the display is
    // coherent (all goods on the same station share the same faction tariff).
    let tariff_num = compute_tariff(
        loc,
        economy,
        faction_state,
        goods.get(sel).unwrap_or(&&GoodId("".into())),
    );
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
        let held = inv.cargo.get(*g).copied().unwrap_or(0);
        let buy = economy.0.stations[&loc.station_id].buy_price(g, tariff_num);
        let sell = economy.0.stations[&loc.station_id].sell_price(g, tariff_num);
        let marker = if i == sel { "> " } else { "  " };
        lines.push(format!(
            "{}{:>8}  buy {}  sell {}  have {}",
            marker, g.0, buy, sell, held
        ));
    }
    lines.push(format!("qty: {}", state.qty));
    lines.join("\n")
}
