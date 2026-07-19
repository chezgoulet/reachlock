//! Gate Network editor (handoff §14): a visual directed-graph editor for
//! the charted-space gate network. Systems render as draggable circular
//! nodes on a pannable/zoomable canvas; gates are status-colored arrows.
//! Edits `GateNetwork` (`mods/reachlock/gate_network/*.ron`).

use std::collections::HashMap;

use reachlock_core::galaxy::gate::{Gate, GateNetwork, GateStatus};
use reachlock_core::galaxy::ChartedSystem;
use reachlock_core::seed::types::{Biome, SystemId};

use super::super::app::{ContentType, Editor};

const STATUSES: [GateStatus; 5] = [
    GateStatus::Active,
    GateStatus::Blockaded,
    GateStatus::Restricted,
    GateStatus::Contested,
    GateStatus::Destroyed,
];

fn status_name(s: GateStatus) -> &'static str {
    match s {
        GateStatus::Active => "Active",
        GateStatus::Blockaded => "Blockaded",
        GateStatus::Restricted => "Restricted",
        GateStatus::Contested => "Contested",
        GateStatus::Destroyed => "Destroyed",
    }
}

fn status_color(s: GateStatus) -> egui::Color32 {
    match s {
        GateStatus::Active => egui::Color32::from_rgb(0x4C, 0xAF, 0x50),
        GateStatus::Blockaded => egui::Color32::from_rgb(0xF4, 0x43, 0x36),
        GateStatus::Restricted => egui::Color32::from_rgb(0xFF, 0x98, 0x00),
        GateStatus::Contested => egui::Color32::from_rgb(0xFF, 0xEB, 0x3B),
        GateStatus::Destroyed => egui::Color32::from_rgb(0x42, 0x42, 0x42),
    }
}

fn cycle_status(s: GateStatus) -> GateStatus {
    match s {
        GateStatus::Active => GateStatus::Restricted,
        GateStatus::Restricted => GateStatus::Blockaded,
        GateStatus::Blockaded => GateStatus::Contested,
        GateStatus::Contested => GateStatus::Destroyed,
        GateStatus::Destroyed => GateStatus::Active,
    }
}

fn biome_color(b: Option<Biome>) -> egui::Color32 {
    match b {
        Some(Biome::Core) => egui::Color32::from_rgb(0xDA, 0xA5, 0x20),
        Some(Biome::Frontier) => egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
        Some(Biome::Nebula) => egui::Color32::from_rgb(0x93, 0x70, 0xDB),
        Some(Biome::Derelict) => egui::Color32::from_rgb(0x80, 0x80, 0x80),
        Some(Biome::DeepSpace) => egui::Color32::from_rgb(0x2F, 0x4F, 0x4F),
        None => egui::Color32::from_rgb(0xCC, 0xCC, 0xCC),
    }
}

const NODE_RADIUS: f32 = 40.0;

pub struct GateNetworkEditor {
    network: GateNetwork,
    path: Option<std::path::PathBuf>,
    has_changes: bool,
    /// World-space node positions, keyed by system id.
    node_positions: HashMap<String, egui::Pos2>,
    /// Charted system biomes for node coloring.
    biomes: HashMap<String, Biome>,
    selected_gate: Option<usize>,
    pan: egui::Vec2,
    zoom: f32,
    add_from: String,
    add_to: String,
    new_system_name: String,
}

impl GateNetworkEditor {
    fn new() -> Self {
        let default_path = std::path::Path::new("mods/reachlock/gate_network/core_region.ron");
        let (network, path) = match crate::io::read_ron::<GateNetwork>(default_path) {
            Ok(n) => (n, Some(default_path.to_path_buf())),
            Err(_) => (GateNetwork { gates: Vec::new() }, None),
        };
        let mut biomes = HashMap::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/systems") {
            for entry in dir.flatten() {
                if let Ok(system) = crate::io::read_ron::<ChartedSystem>(&entry.path()) {
                    biomes.insert(system.id.clone(), system.biome);
                }
            }
        }
        let mut editor = GateNetworkEditor {
            network,
            path,
            has_changes: false,
            node_positions: HashMap::new(),
            biomes,
            selected_gate: None,
            pan: egui::vec2(80.0, 80.0),
            zoom: 1.0,
            add_from: String::new(),
            add_to: String::new(),
            new_system_name: String::new(),
        };
        editor.ensure_positions();
        editor
    }

    /// Every system named by a gate (sorted, deduped).
    fn system_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .network
            .gates
            .iter()
            .flat_map(|g| [g.from.0.clone(), g.to.0.clone()])
            .chain(self.node_positions.keys().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Grid auto-layout for any node without a stored position.
    fn ensure_positions(&mut self) {
        let ids = self.system_ids();
        let mut next = self.node_positions.len();
        for id in ids {
            self.node_positions.entry(id).or_insert_with(|| {
                let pos = egui::pos2(
                    (next % 4) as f32 * 180.0 + 60.0,
                    (next / 4) as f32 * 160.0 + 60.0,
                );
                next += 1;
                pos
            });
        }
    }

    fn auto_layout(&mut self) {
        let ids = self.system_ids();
        self.node_positions.clear();
        for (i, id) in ids.into_iter().enumerate() {
            self.node_positions.insert(
                id,
                egui::pos2(
                    (i % 4) as f32 * 180.0 + 60.0,
                    (i / 4) as f32 * 160.0 + 60.0,
                ),
            );
        }
    }
}

/// Distance from point `p` to segment `a`-`b`.
fn point_segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

impl Editor for GateNetworkEditor {
    fn title(&self) -> &str {
        "Gate Network Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::GateNetwork
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.network = crate::io::read_ron(path)?;
        self.path = Some(path.to_path_buf());
        self.selected_gate = None;
        self.node_positions.clear();
        self.ensure_positions();
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.network)
    }

    fn validate(&self) -> Vec<String> {
        match self.network.validate() {
            Ok(()) => Vec::new(),
            Err(e) => vec![e],
        }
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        let system_ids = self.system_ids();

        egui::TopBottomPanel::top("gate_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Add System: charted ids not yet on the canvas, or free text.
                egui::ComboBox::from_id_salt("gate_add_system")
                    .selected_text(if self.new_system_name.is_empty() {
                        "system…".to_string()
                    } else {
                        self.new_system_name.clone()
                    })
                    .show_ui(ui, |ui| {
                        for id in self.biomes.keys() {
                            if !self.node_positions.contains_key(id) {
                                ui.selectable_value(
                                    &mut self.new_system_name,
                                    id.clone(),
                                    id,
                                );
                            }
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_system_name)
                        .hint_text("or type id")
                        .desired_width(100.0),
                );
                if ui.button("Add System").clicked() && !self.new_system_name.is_empty() {
                    let name = std::mem::take(&mut self.new_system_name);
                    self.node_positions
                        .entry(name)
                        .or_insert(egui::pos2(100.0, 100.0));
                }

                ui.separator();
                egui::ComboBox::from_id_salt("gate_add_from")
                    .selected_text(if self.add_from.is_empty() {
                        "from…".to_string()
                    } else {
                        self.add_from.clone()
                    })
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for id in &system_ids {
                            ui.selectable_value(&mut self.add_from, id.clone(), id);
                        }
                    });
                egui::ComboBox::from_id_salt("gate_add_to")
                    .selected_text(if self.add_to.is_empty() {
                        "to…".to_string()
                    } else {
                        self.add_to.clone()
                    })
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for id in &system_ids {
                            ui.selectable_value(&mut self.add_to, id.clone(), id);
                        }
                    });
                if ui.button("Add Gate").clicked()
                    && !self.add_from.is_empty()
                    && !self.add_to.is_empty()
                    && self.add_from != self.add_to
                {
                    self.network.gates.push(Gate {
                        from: SystemId(self.add_from.clone()),
                        to: SystemId(self.add_to.clone()),
                        status: GateStatus::Active,
                        controlled_by: None,
                    });
                    self.selected_gate = Some(self.network.gates.len() - 1);
                    self.has_changes = true;
                }
                if ui.button("Delete Selected").clicked() {
                    if let Some(i) = self.selected_gate.take() {
                        if i < self.network.gates.len() {
                            self.network.gates.remove(i);
                            self.has_changes = true;
                        }
                    }
                }
                if ui.button("Auto-Layout").clicked() {
                    self.auto_layout();
                }
                ui.label(format!("{}%", (self.zoom * 100.0) as i32));
                if self.has_changes {
                    ui.label("*");
                }
            });
        });

        egui::SidePanel::left("gate_list")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Gates");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..self.network.gates.len() {
                        let gate = &self.network.gates[i];
                        let mut label = format!("{} → {}", gate.from.0, gate.to.0);
                        if let Some(faction) = &gate.controlled_by {
                            label.push_str(&format!(" [{faction}]"));
                        }
                        ui.horizontal(|ui| {
                            ui.colored_label(status_color(gate.status), "■");
                            if ui
                                .selectable_label(self.selected_gate == Some(i), &label)
                                .clicked()
                            {
                                self.selected_gate = Some(i);
                            }
                        });
                    }
                });
                ui.separator();
                if let Some(i) = self.selected_gate {
                    if let Some(gate) = self.network.gates.get_mut(i) {
                        ui.label("Selected gate:");
                        egui::ComboBox::from_id_salt("gate_sel_status")
                            .selected_text(status_name(gate.status))
                            .show_ui(ui, |ui| {
                                for s in STATUSES {
                                    if ui
                                        .selectable_value(&mut gate.status, s, status_name(s))
                                        .changed()
                                    {
                                        self.has_changes = true;
                                    }
                                }
                            });
                        let mut controlled =
                            gate.controlled_by.clone().unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label("Controlled by:");
                            if ui.text_edit_singleline(&mut controlled).changed() {
                                gate.controlled_by =
                                    (!controlled.is_empty()).then_some(controlled);
                                self.has_changes = true;
                            }
                        });
                    }
                }
                let validation = self.validate();
                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });

        // Delete key removes the selected gate.
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            if let Some(i) = self.selected_gate.take() {
                if i < self.network.gates.len() {
                    self.network.gates.remove(i);
                    self.has_changes = true;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let (canvas_rect, canvas_response) = ui.allocate_exact_size(
                    ui.available_size(),
                    egui::Sense::click_and_drag(),
                );
                let painter = ui.painter_at(canvas_rect);

                // Scroll-wheel zoom around the pointer.
                if let Some(hover) = canvas_response.hover_pos() {
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll.abs() > 0.0 {
                        let old_zoom = self.zoom;
                        self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(0.25, 4.0);
                        // Keep the world point under the cursor fixed.
                        let world =
                            ((hover - canvas_rect.min) - self.pan) / old_zoom;
                        self.pan = (hover - canvas_rect.min) - world * self.zoom;
                    }
                }
                // Middle/right/background drag pans.
                if canvas_response.dragged_by(egui::PointerButton::Middle)
                    || canvas_response.dragged_by(egui::PointerButton::Secondary)
                    || canvas_response.dragged_by(egui::PointerButton::Primary)
                {
                    self.pan += canvas_response.drag_delta();
                }

                let to_screen = |world: egui::Pos2| -> egui::Pos2 {
                    canvas_rect.min + self.pan + world.to_vec2() * self.zoom
                };
                let radius = NODE_RADIUS * self.zoom;

                // ── Gates (drawn first, under the nodes) ──
                let mut clicked_gate: Option<usize> = None;
                for (i, gate) in self.network.gates.iter().enumerate() {
                    let (Some(&from_w), Some(&to_w)) = (
                        self.node_positions.get(&gate.from.0),
                        self.node_positions.get(&gate.to.0),
                    ) else {
                        continue;
                    };
                    let from = to_screen(from_w);
                    let to = to_screen(to_w);
                    let dir = (to - from).normalized();
                    // Perpendicular offset so A→B and B→A don't overlap.
                    let perp = egui::vec2(-dir.y, dir.x) * 6.0;
                    let start = from + dir * radius + perp;
                    let end = to - dir * radius + perp;
                    let color = status_color(gate.status);
                    let width = if self.selected_gate == Some(i) { 3.0 } else { 1.5 };
                    let stroke = egui::Stroke::new(width, color);
                    if gate.status == GateStatus::Destroyed {
                        // Dashed line for destroyed gates.
                        let seg = (end - start).length();
                        let n = (seg / 10.0).max(1.0) as usize;
                        for k in 0..n {
                            if k % 2 == 0 {
                                let a = start + (end - start) * (k as f32 / n as f32);
                                let b =
                                    start + (end - start) * ((k + 1) as f32 / n as f32);
                                painter.line_segment([a, b], stroke);
                            }
                        }
                    } else {
                        painter.line_segment([start, end], stroke);
                    }
                    // Arrowhead at the destination.
                    let tip = end;
                    let left = tip - dir * 10.0 + egui::vec2(-dir.y, dir.x) * 5.0;
                    let right = tip - dir * 10.0 - egui::vec2(-dir.y, dir.x) * 5.0;
                    painter.line_segment([tip, left], stroke);
                    painter.line_segment([tip, right], stroke);
                    // Status label at the midpoint.
                    let mid = start + (end - start) * 0.5;
                    painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        status_name(gate.status),
                        egui::FontId::proportional(10.0 * self.zoom.max(0.6)),
                        color,
                    );
                    // Hit test on click.
                    if canvas_response.clicked() {
                        if let Some(click) = canvas_response.interact_pointer_pos() {
                            if point_segment_distance(click, start, end) < 8.0 {
                                clicked_gate = Some(i);
                            }
                        }
                    }
                }
                if let Some(i) = clicked_gate {
                    // First click selects; clicking the selected gate cycles
                    // its status (handoff §14).
                    if self.selected_gate == Some(i) {
                        let gate = &mut self.network.gates[i];
                        gate.status = cycle_status(gate.status);
                        self.has_changes = true;
                    } else {
                        self.selected_gate = Some(i);
                    }
                }

                // ── Nodes ──
                for id in &system_ids {
                    let Some(world) = self.node_positions.get(id).copied() else {
                        continue;
                    };
                    let center = to_screen(world);
                    let node_rect =
                        egui::Rect::from_center_size(center, egui::vec2(radius * 2.0, radius * 2.0));
                    let response = ui.interact(
                        node_rect,
                        ui.id().with(("gate_node", id)),
                        egui::Sense::drag(),
                    );
                    if response.dragged() {
                        if let Some(pos) = self.node_positions.get_mut(id) {
                            *pos += response.drag_delta() / self.zoom;
                        }
                    }
                    let color = biome_color(self.biomes.get(id).copied());
                    painter.circle_filled(center, radius, color);
                    painter.circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(1.5, egui::Color32::BLACK),
                    );
                    painter.text(
                        center + egui::vec2(0.0, radius + 4.0),
                        egui::Align2::CENTER_TOP,
                        id,
                        egui::FontId::proportional(12.0 * self.zoom.max(0.7)),
                        ui.visuals().text_color(),
                    );
                }
            });
        });
    }

    fn generate_from_seed(&mut self, _seed: u64) {
        // Gate networks are purely authored (handoff §14) — no-op.
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(GateNetworkEditor::new())
}
