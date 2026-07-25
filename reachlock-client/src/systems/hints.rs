use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct HintRegistry {
    pub hints: HashMap<String, HintDef>,
}

#[derive(Clone, Debug)]
pub struct HintDef {
    pub hint_id: String,
    pub title: String,
    pub body: String,
}

#[derive(Component)]
pub struct HintTarget {
    pub hint_id: String,
}

pub fn init_hint_registry(mut registry: ResMut<HintRegistry>) {
    registry.hints = default_hints();
}

fn default_hints() -> HashMap<String, HintDef> {
    let mut m = HashMap::new();
    m.insert(
        "fuel_gauge".into(),
        HintDef {
            hint_id: "fuel_gauge".into(),
            title: "Fuel Gauge".into(),
            body: "Current fuel level. When fuel drops below 15%, Boris will warn you. At 0%, the ship drifts.".into(),
        },
    );
    m.insert(
        "hull_integrity".into(),
        HintDef {
            hint_id: "hull_integrity".into(),
            title: "Hull Integrity".into(),
            body: "Your ship's structural health. Below 30%, systems start failing. At 0%, hull breach.".into(),
        },
    );
    m.insert(
        "speed_indicator".into(),
        HintDef {
            hint_id: "speed_indicator".into(),
            title: "Speed".into(),
            body: "Current velocity relative to nearby objects. Use W/S to adjust.".into(),
        },
    );
    m.insert(
        "deliberation_panel".into(),
        HintDef {
            hint_id: "deliberation_panel".into(),
            title: "Deliberation".into(),
            body: "Your crew member is deciding. Watch their reasoning and the rules that run out."
                .into(),
        },
    );
    m.insert(
        "rule_list".into(),
        HintDef {
            hint_id: "rule_list".into(),
            title: "Rules & Gaps".into(),
            body: "Green rules matched the situation. Red gaps are conditions your rules didn't cover.".into(),
        },
    );
    m.insert(
        "interjection_prompt".into(),
        HintDef {
            hint_id: "interjection_prompt".into(),
            title: "Interjection".into(),
            body: "Override the deliberation. Quick way to take command — but costs trust.".into(),
        },
    );
    m.insert(
        "uncovered_counter".into(),
        HintDef {
            hint_id: "uncovered_counter".into(),
            title: "Uncovered Evaluations".into(),
            body: "Uncovered evaluations = situations your rules didn't cover. Write more rules to reduce this number and improve your crew's deliberation odds.".into(),
        },
    );
    m
}
