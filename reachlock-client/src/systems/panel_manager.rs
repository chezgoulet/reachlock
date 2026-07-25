use crate::states::PanelGroup;

#[allow(dead_code)]
pub fn close_panel_in_group(
    active: PanelGroup,
    exclude: &[&str],
    factions: &mut bool,
    discovery: &mut bool,
    career: &mut bool,
    log_viewer: &mut bool,
    mission_board: &mut bool,
) {
    if active == PanelGroup::InfoPanel {
        if !exclude.contains(&"factions") {
            *factions = false;
        }
        if !exclude.contains(&"discovery") {
            *discovery = false;
        }
        if !exclude.contains(&"career") {
            *career = false;
        }
        if !exclude.contains(&"log_viewer") {
            *log_viewer = false;
        }
        if !exclude.contains(&"mission_board") {
            *mission_board = false;
        }
    }
}
