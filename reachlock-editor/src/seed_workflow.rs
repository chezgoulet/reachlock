//! Seed workflow panel (handoff completion §Priority 5): one seed field
//! driving every open editor. "Reroll All" applies the current seed to all
//! editors that accept rerolls and auto-increments for the next click;
//! "Lock Current" derives a stable seed from the active tab so the user has
//! a reproducible starting point to tweak.

/// Seeds must survive JSON round-trips (≤ 2^53, iron rule §7).
pub const SEED_MASK: u64 = 0x001F_FFFF_FFFF_FFFF;

/// What the panel asks the app shell to do this frame.
pub enum SeedAction {
    /// Apply this seed to every open editor that accepts rerolls.
    RerollAll(u64),
    /// Fill the seed field from the active tab (shell computes the hash).
    LockCurrent,
}

pub struct SeedWorkflow {
    pub seed: u64,
}

impl SeedWorkflow {
    pub fn new() -> Self {
        Self { seed: 42 }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed & SEED_MASK;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<SeedAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label("Seed:");
            ui.add(
                egui::DragValue::new(&mut self.seed)
                    .range(0..=SEED_MASK)
                    .speed(1),
            );
            if ui
                .button("Reroll All")
                .on_hover_text("Regenerate every open editor from this seed, then advance the seed")
                .clicked()
            {
                let seed = self.seed;
                // Auto-increment so repeated clicks walk through seeds.
                self.seed = (self.seed + 1) & SEED_MASK;
                action = Some(SeedAction::RerollAll(seed));
            }
            if ui
                .button("Lock Current")
                .on_hover_text("Derive a stable seed from the active tab")
                .clicked()
            {
                action = Some(SeedAction::LockCurrent);
            }
        });
        action
    }
}
