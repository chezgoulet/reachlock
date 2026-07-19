use std::collections::HashSet;

pub struct SeedWorkflow {
    pub seed: u64,
    pub locked_fields: HashSet<String>,
}

impl SeedWorkflow {
    pub fn new() -> Self {
        Self {
            seed: 42,
            locked_fields: HashSet::new(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("Seed Workflow");
            ui.horizontal(|ui| {
                ui.label("Seed:");
                let mut seed_str = self.seed.to_string();
                if ui.text_edit_singleline(&mut seed_str).changed() {
                    self.seed = seed_str.parse().unwrap_or(self.seed);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Reroll All").clicked() {
                    self.seed = self.reroll();
                }
                if ui.button("Lock Current").clicked() {
                    self.locked_fields.insert("*".to_string());
                }
            });
            if !self.locked_fields.is_empty() {
                ui.label(format!("Locked fields: {}", self.locked_fields.len()));
            }
        });
    }

    pub fn reroll(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        (self.seed.wrapping_mul(0x9E3779B97F4A7C15) ^ nanos) & 0x001F_FFFF_FFFF_FFFF
    }
}
