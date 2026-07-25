use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ingredient {
    pub item_id: String,
    pub quantity: u32,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub item_id: String,
    pub quantity: u32,
    pub quality_min: u32,
    pub quality_max: u32,
    pub durability: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRequirement {
    pub category: String,
    pub minimum_level: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub ingredients: Vec<Ingredient>,
    pub output: OutputConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_requirement: Option<SkillRequirement>,
    pub workbench_type: String,
    pub duration_ticks: u64,
}
