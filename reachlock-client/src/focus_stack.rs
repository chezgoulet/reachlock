use bevy::prelude::*;

use crate::systems::interaction::ActivePanel;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FocusLayer {
    #[default]
    World,
    #[allow(dead_code)]
    Panel(ActivePanel),
    #[allow(dead_code)]
    Pause,
    #[allow(dead_code)]
    Settings,
    #[allow(dead_code)]
    Tool(ActivePanel),
    #[allow(dead_code)]
    Modal,
}

#[derive(Resource)]
pub struct FocusStack {
    layers: Vec<FocusLayer>,
}

impl Default for FocusStack {
    fn default() -> Self {
        FocusStack {
            layers: vec![FocusLayer::World],
        }
    }
}

impl FocusStack {
    pub fn push(&mut self, layer: FocusLayer) {
        self.layers.push(layer);
    }

    pub fn pop(&mut self) -> Option<FocusLayer> {
        self.layers.pop()
    }

    pub fn top(&self) -> &FocusLayer {
        self.layers.last().unwrap_or(&FocusLayer::World)
    }

    pub fn is_active(&self, layer: FocusLayer) -> bool {
        self.layers.last() == Some(&layer)
    }

    pub fn pop_until(&mut self, target: FocusLayer) {
        while let Some(top) = self.layers.last() {
            if *top == target {
                break;
            }
            self.layers.pop();
        }
    }

    pub fn pop_until_not_modal(&mut self) {
        while self.layers.last() == Some(&FocusLayer::Modal) {
            self.layers.pop();
        }
    }

    pub fn depth(&self) -> usize {
        self.layers.len()
    }

    pub fn top_captures_input(&self) -> bool {
        matches!(
            self.layers.last(),
            Some(FocusLayer::Modal | FocusLayer::Settings | FocusLayer::Pause)
        )
    }

    pub fn has_any_panel(&self) -> bool {
        self.layers
            .iter()
            .any(|l| matches!(l, FocusLayer::Panel(_) | FocusLayer::Tool(_)))
    }
}

pub fn init_focus_stack(mut commands: Commands) {
    commands.insert_resource(FocusStack::default());
}

pub fn push_focus(mut stack: ResMut<FocusStack>, layer: FocusLayer) {
    stack.push(layer);
}

pub fn pop_focus(mut stack: ResMut<FocusStack>) {
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_stack_lifecycle() {
        let mut stack = FocusStack::default();
        assert_eq!(*stack.top(), FocusLayer::World);

        stack.push(FocusLayer::Pause);
        assert!(stack.is_active(FocusLayer::Pause));
        assert!(!stack.is_active(FocusLayer::World));

        stack.push(FocusLayer::Settings);
        assert!(stack.is_active(FocusLayer::Settings));

        assert_eq!(stack.pop(), Some(FocusLayer::Settings));
        assert!(stack.is_active(FocusLayer::Pause));

        assert_eq!(stack.pop(), Some(FocusLayer::Pause));
        assert!(stack.is_active(FocusLayer::World));
    }

    #[test]
    fn pop_until_removes_layers() {
        let mut stack = FocusStack::default();
        stack.push(FocusLayer::Panel(ActivePanel::Market));
        stack.push(FocusLayer::Pause);
        stack.push(FocusLayer::Settings);

        stack.pop_until(FocusLayer::World);
        assert_eq!(*stack.top(), FocusLayer::World);
    }

    #[test]
    fn pop_until_leaves_target() {
        let mut stack = FocusStack::default();
        stack.push(FocusLayer::Panel(ActivePanel::Market));
        stack.push(FocusLayer::Pause);

        stack.pop_until(FocusLayer::Panel(ActivePanel::Market));
        assert_eq!(*stack.top(), FocusLayer::Panel(ActivePanel::Market));
    }

    #[test]
    fn modal_stays_on_top() {
        let mut stack = FocusStack::default();
        stack.push(FocusLayer::Panel(ActivePanel::Market));
        stack.push(FocusLayer::Modal);

        assert!(stack.is_active(FocusLayer::Modal));
        assert!(!stack.is_active(FocusLayer::Panel(ActivePanel::Market)));
    }
}
