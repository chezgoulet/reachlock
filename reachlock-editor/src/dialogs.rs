//! Reusable modal confirmation dialog (handoff completion §Priority 3).
//!
//! One function renders a centered `egui::Window` with an OK button, a
//! Cancel button, and an optional third button. Callers hold whatever
//! pending-action state they need and act on the returned click.

/// Which button the user clicked this frame, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationResult {
    /// The primary (leftmost) button.
    Ok,
    /// The optional third button (e.g. "Discard").
    Extra,
    /// The cancel button — the caller should drop the pending action.
    Cancel,
}

/// Render a centered modal confirmation. Returns `Some` on the frame a
/// button is clicked, `None` while the dialog stays open.
pub fn confirmation_dialog(
    ctx: &egui::Context,
    title: &str,
    message: &str,
    ok_label: &str,
    cancel_label: &str,
    extra_button: Option<&str>,
) -> Option<ConfirmationResult> {
    let mut result = None;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(ok_label).clicked() {
                    result = Some(ConfirmationResult::Ok);
                }
                if let Some(extra) = extra_button {
                    if ui.button(extra).clicked() {
                        result = Some(ConfirmationResult::Extra);
                    }
                }
                if ui.button(cancel_label).clicked() {
                    result = Some(ConfirmationResult::Cancel);
                }
            });
        });
    // Escape cancels any pending confirmation.
    if result.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(ConfirmationResult::Cancel);
    }
    result
}
