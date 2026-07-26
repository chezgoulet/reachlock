//! Plan / Build mode (S101 P5, defined here because [`super::tools`] gates on
//! it).
//!
//! Plan is read-only: the model can look at anything and propose changes, but
//! no tool it can call writes to a tab or to disk. Build unlocks the mutating
//! tools. The distinction is enforced in the dispatcher rather than asked for
//! in the system prompt, because a prompt is a request and a dispatcher is a
//! guarantee.

use super::tools::Mutability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Read-only. The safe default: a fresh session cannot change content
    /// before the author has seen what it intends to do.
    #[default]
    Plan,
    /// Mutating tools unlocked.
    Build,
}

impl Mode {
    pub fn allows(self, m: Mutability) -> bool {
        match self {
            Mode::Plan => m == Mutability::ReadOnly,
            Mode::Build => true,
        }
    }

    /// Shown on the assistant panel's mode button (P5).
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Mode::Plan => "Plan",
            Mode::Build => "Build",
        }
    }

    /// Driven by the panel button and the Tab key (P5).
    #[allow(dead_code)]
    pub fn toggled(self) -> Self {
        match self {
            Mode::Plan => Mode::Build,
            Mode::Build => Mode::Plan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_is_the_default() {
        assert_eq!(Mode::default(), Mode::Plan);
    }

    #[test]
    fn plan_allows_only_reads() {
        assert!(Mode::Plan.allows(Mutability::ReadOnly));
        assert!(!Mode::Plan.allows(Mutability::Mutating));
    }

    #[test]
    fn build_allows_everything() {
        assert!(Mode::Build.allows(Mutability::ReadOnly));
        assert!(Mode::Build.allows(Mutability::Mutating));
    }

    #[test]
    fn toggling_round_trips() {
        assert_eq!(Mode::Plan.toggled().toggled(), Mode::Plan);
        assert_eq!(Mode::Plan.toggled(), Mode::Build);
    }
}
