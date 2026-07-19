//! Contract generator (S25): seed + kind -> contract data.

use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Contract {
    pub name: String,
    pub trigger: String,
    pub action: String,
    pub priority: u32,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

fn name_tables(kind: &str) -> &'static [&'static str] {
    match kind {
        "bounty" => &["Red Mercury", "Iron Reach", "Cold Trail", "Last Debt", "Blood Price", "Wanted Signal", "Dead Letter", "Bounty Prime"],
        "delivery" => &["Package Run", "Supply Haul", "Data Courier", "Priority Freight", "Medical Drop", "Rush Order", "Care Package", "Discreet Shipment"],
        "escort" => &["Safe Passage", "Convoy Duty", "Guard Detail", "Shadow Run", "Protection Pact", "Armored Transit", "Watchdog", "Bodyguard"],
        "exploration" => &["Deep Scan", "Unknown Signal", "Rumor Trace", "Lost Contact", "Charting Run", "Anomaly Report", "Frontier Survey", "Ghost Signal"],
        "salvage" => &["Wreck Recovery", "Scavenger Rights", "Derelict Claim", "Harvest Run", "Junk Haul", "Rust Collector", "Debris Sweep", "Bone Picking"],
        _ => &["Open Contract", "General Task", "Standard Job", "Blank Order"],
    }
}

fn trigger_tables(kind: &str) -> &'static [&'static str] {
    match kind {
        "bounty" => &["on_dock", "system_entry", "jump_arrival", "comm_open", "hail_received"],
        "delivery" => &["cargo_loaded", "departure_clear", "jump_complete", "proximity_alert", "timer_expiry"],
        "escort" => &["convoy_damaged", "hostile_contact", "nav_point_reached", "shield_depleted", "emergency_hail"],
        "exploration" => &["signal_detected", "anomaly_scan", "landing_gear_deploy", "log_entry", "probe_launch"],
        "salvage" => &["wreck_grappled", "hull_breach_secured", "data_core_extracted", "debris_field_entered", "containment_breach"],
        _ => &["default_trigger", "manual_activation"],
    }
}

fn action_tables(kind: &str) -> &'static [&'static str] {
    match kind {
        "bounty" => &["locate_target", "engage_hostile", "disable_engines", "capture_crew", "verify_kill", "collect_bounty"],
        "delivery" => &["retrieve_package", "navigate_to_drop", "avoid_detection", "deliver_cargo", "confirm_receipt"],
        "escort" => &["scan_for_threats", "maintain_formation", "intercept_pursuer", "escort_to_gate", "report_safe_arrival"],
        "exploration" => &["scan_anomaly", "collect_sample", "log_coordinates", "activate_beacon", "return_data"],
        "salvage" => &["dock_with_wreck", "salvage_hull", "extract_components", "purge_hazards", "tow_to_station"],
        _ => &["execute_protocol", "report_status"],
    }
}

pub fn generate_contract(seed: u64, kind: &str) -> Contract {
    let mut rng = SeededRng::new(seed);

    let name = pick(&mut rng, name_tables(kind)).to_string();
    let trigger = pick(&mut rng, trigger_tables(kind)).to_string();
    let action = pick(&mut rng, action_tables(kind)).to_string();
    let priority = 1 + rng.next_below(10) as u32;

    Contract { name, trigger, action, priority }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_contract(42, "bounty");
        let b = generate_contract(42, "bounty");
        assert_eq!(a.name, b.name);
        assert_eq!(a.trigger, b.trigger);
        assert_eq!(a.action, b.action);
        assert_eq!(a.priority, b.priority);
    }

    #[test]
    fn kinds_differ() {
        let a = generate_contract(7, "bounty");
        let b = generate_contract(7, "delivery");
        assert_ne!(a.name, b.name);
    }

    #[test]
    fn priority_in_range() {
        let c = generate_contract(99, "exploration");
        assert!((1..=10).contains(&c.priority));
    }
}
