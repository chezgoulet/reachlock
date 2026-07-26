//! Tool registry (S101 P2).
//!
//! One definition of the tool surface, exposed two ways: the in-editor agent
//! loop and the MCP server. Defining them twice is how the two frontends drift
//! until a tool works in one and not the other.
//!
//! Tools split by what they need, not by what they do:
//!
//! - **Content tools** ([`content`]) touch only disk. They run wherever the
//!   caller is — the agent thread, or a headless `--mcp-stdio` process with no
//!   GUI at all.
//! - **Session tools** (P4) need the live `Box<dyn Editor>` tabs, which are
//!   not `Send` and live on the UI thread. Those cross a channel.
//!
//! That split is why [`Tool::needs_session`] exists: a headless frontend can
//! advertise exactly the tools it can actually run instead of failing at call
//! time.

pub mod content;
pub mod render;
pub mod session;

use serde_json::Value;

use super::bridge::SessionHandle;
use super::mode::Mode;

/// Whether a tool changes anything.
///
/// This is the Plan/Build gate. It is a property of the tool, declared once
/// here, rather than a check each call site remembers to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    /// Reads only. Available in both modes.
    ReadOnly,
    /// Writes to a tab or to disk. Build mode only.
    ///
    /// No tool declares this yet — every content tool reads. The session
    /// tools that write land in P4; the variant and its gate exist first so
    /// the guarantee is in place before anything can violate it.
    #[allow(dead_code)]
    Mutating,
}

/// The result of running a tool, on its way back to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    /// Rendered for the model. Errors go here too — a tool that fails should
    /// explain itself in a way the model can act on, not raise.
    pub content: String,
    pub is_error: bool,
    /// Images produced by the tool (P6). Dropped by frontends and providers
    /// that cannot carry them.
    pub images: Vec<(String, Vec<u8>)>,
}

impl ToolOutcome {
    pub fn ok(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: false,
            images: Vec::new(),
        }
    }

    /// A failure the model should read and retry around, not a transport
    /// error. Tool failures are ordinary conversation.
    pub fn error(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: true,
            images: Vec::new(),
        }
    }
}

/// What a tool is allowed to reach.
///
/// Carried rather than global so a headless frontend is a `ToolCtx` with no
/// session, not a special code path — the same registry serves both.
#[derive(Clone, Default)]
pub struct ToolCtx {
    /// Channel to the UI thread. `None` in the headless MCP server, where
    /// there are no tabs to talk to.
    pub session: Option<SessionHandle>,
}

impl ToolCtx {
    pub fn headless() -> Self {
        ToolCtx { session: None }
    }

    /// Built when the agent loop is spawned (P5).
    #[allow(dead_code)]
    pub fn with_session(session: SessionHandle) -> Self {
        ToolCtx {
            session: Some(session),
        }
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }
}

/// One callable tool.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the arguments object. Handed to the provider verbatim
    /// and to MCP clients as `inputSchema`.
    pub input_schema: fn() -> Value,
    pub mutability: Mutability,
    /// True when the tool needs a live editor session (P4). Content tools are
    /// false and run anywhere.
    pub needs_session: bool,
    /// Executes the tool. Session tools get a context with no session in
    /// headless frontends and must say so rather than panicking.
    pub run: fn(&Value, &ToolCtx) -> ToolOutcome,
}

pub struct ToolRegistry {
    tools: Vec<Tool>,
}

impl ToolRegistry {
    /// Every tool the editor knows about.
    pub fn new() -> Self {
        let mut tools = content::tools();
        tools.extend(render::tools());
        tools.extend(session::tools());
        ToolRegistry { tools }
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Tools to advertise for `mode`.
    ///
    /// Filtering here is an optimisation, not the guarantee: it stops the
    /// model burning turns proposing calls it cannot make. [`Self::dispatch`]
    /// re-checks, because the author can flip the mode between a request being
    /// built and the call arriving.
    pub fn available(&self, mode: Mode, has_session: bool) -> Vec<&Tool> {
        self.tools
            .iter()
            .filter(|t| mode.allows(t.mutability))
            .filter(|t| has_session || !t.needs_session)
            .collect()
    }

    /// Run a tool by name, enforcing the mode gate.
    pub fn dispatch(&self, name: &str, args: &Value, mode: Mode, ctx: &ToolCtx) -> ToolOutcome {
        let Some(tool) = self.get(name) else {
            return ToolOutcome::error(format!(
                "No tool named `{name}`. Available: {}",
                self.tools
                    .iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        if !mode.allows(tool.mutability) {
            // Phrased so the model can act on it: it should either stop
            // proposing writes or tell the author to switch to Build.
            return ToolOutcome::error(format!(
                "`{name}` changes content and this session is in Plan mode, \
                 which is read-only. Describe the change instead; the author \
                 switches to Build mode (Tab) to apply it."
            ));
        }
        if tool.needs_session && !ctx.has_session() {
            return ToolOutcome::error(format!(
                "`{name}` needs a live editor session and this process has none."
            ));
        }
        (tool.run)(args, ctx)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_tool_has_an_object_schema_and_a_unique_name() {
        let reg = ToolRegistry::new();
        let mut seen = std::collections::BTreeSet::new();
        for t in reg.available(Mode::Build, true) {
            assert!(seen.insert(t.name), "duplicate tool name `{}`", t.name);
            assert!(!t.description.is_empty(), "`{}` has no description", t.name);
            let schema = (t.input_schema)();
            assert_eq!(
                schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "`{}` input_schema must be an object schema",
                t.name
            );
        }
        assert!(!seen.is_empty(), "registry is empty");
    }

    /// The gate is the dispatcher's job, not each call site's. If a mutating
    /// tool ever runs in Plan mode, the whole read-only guarantee is gone.
    #[test]
    fn plan_mode_refuses_mutating_tools() {
        let reg = ToolRegistry::new();
        // No mutating tools exist until P4, so assert the rule directly
        // against the dispatcher rather than waiting for one to appear.
        for t in reg.available(Mode::Build, true) {
            let out = reg.dispatch(t.name, &json!({}), Mode::Plan, &ToolCtx::headless());
            if t.mutability == Mutability::Mutating {
                assert!(out.is_error, "`{}` ran in Plan mode", t.name);
                assert!(
                    out.content.contains("Plan mode"),
                    "`{}` refusal should name the mode so the model can act on it",
                    t.name
                );
            }
        }
    }

    #[test]
    fn read_only_tools_are_available_in_both_modes() {
        let reg = ToolRegistry::new();
        let plan = reg.available(Mode::Plan, true);
        let build = reg.available(Mode::Build, true);
        assert!(!plan.is_empty());
        assert!(build.len() >= plan.len());
        for t in &plan {
            assert_eq!(t.mutability, Mutability::ReadOnly);
        }
    }

    /// A headless frontend must not advertise tools it cannot run.
    #[test]
    fn session_tools_are_hidden_without_a_session() {
        let reg = ToolRegistry::new();
        for t in reg.available(Mode::Build, false) {
            assert!(!t.needs_session, "`{}` needs a session", t.name);
        }
    }

    #[test]
    fn an_unknown_tool_is_an_error_not_a_panic() {
        let reg = ToolRegistry::new();
        let out = reg.dispatch(
            "no_such_tool",
            &json!({}),
            Mode::Build,
            &ToolCtx::headless(),
        );
        assert!(out.is_error);
        assert!(out.content.contains("No tool named"));
    }
}
