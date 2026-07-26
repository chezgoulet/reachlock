//! Live-tab tools (S101 P4).
//!
//! These are the mutating half of the surface. Every one of them posts a typed
//! [`SessionOp`] to the UI thread and waits — see [`crate::agent::bridge`] for
//! why they cannot call the editor directly.
//!
//! Two decisions worth knowing:
//!
//! **`write_document` goes through `Editor::snapshot`/`restore_snapshot`, not
//! `apply_ai_json`.** Snapshot is implemented by 26 of ~28 editors because it
//! backs undo; `apply_ai_json` by 14, with a trait default that returns an
//! error. Leading with the JSON path would mean half the editors answering
//! "not wired yet" to a model that had no way to know that in advance.
//!
//! **Every write returns the validation output.** That is the iteration loop:
//! the model writes, immediately reads back what is wrong, and repairs on its
//! next turn without the author relaying anything. RON has traps a JSON schema
//! cannot express — fixed-size arrays serialize as tuples, newtypes need their
//! parens, variants are snake_case — and no amount of prompting fixes those as
//! reliably as showing the model the error.

use serde_json::{json, Value};

use super::{Mutability, Tool, ToolCtx, ToolOutcome};
use crate::agent::bridge::SessionOp;

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn empty_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// Post an op, or explain that there is no editor to post it to.
fn run_op(ctx: &ToolCtx, op: SessionOp) -> ToolOutcome {
    match &ctx.session {
        Some(handle) => handle.run(op),
        // The registry already refuses session tools without a session, so
        // this is belt and braces — but a panic here would take down the
        // agent thread, and an error is something the model can read.
        None => {
            ToolOutcome::error("This tool needs a running editor with open tabs; none is attached.")
        }
    }
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_tabs",
            description:
                "List the editor's open tabs: name, content type, whether the tab has unsaved \
                 changes, and which one is active. Document tools act on the active tab.",
            input_schema: empty_schema,
            mutability: Mutability::ReadOnly,
            needs_session: true,
            run: |_args, ctx| run_op(ctx, SessionOp::ListTabs),
        },
        Tool {
            name: "read_document",
            description:
                "Read the active tab's full document state as RON, including every loaded entry \
                 and the current selection. This is the exact text `write_document` expects back, \
                 so read before writing and edit what you read.",
            input_schema: empty_schema,
            mutability: Mutability::ReadOnly,
            needs_session: true,
            run: |_args, ctx| run_op(ctx, SessionOp::ReadDocument),
        },
        Tool {
            name: "validate",
            description:
                "Validate the active tab: structural problems plus references pointing at ids \
                 nothing defines. Returns the same findings the editor's own validation panel \
                 shows.",
            input_schema: empty_schema,
            mutability: Mutability::ReadOnly,
            needs_session: true,
            run: |_args, ctx| run_op(ctx, SessionOp::Validate),
        },
        Tool {
            name: "open_tab",
            description:
                "Open a content file in the editor tab that handles its type, and make it \
                 active. Path is relative to the content root. Opening a file that is already \
                 open focuses the existing tab rather than opening it twice.",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path relative to the content root, e.g. <directory>/<id>.ron",
                        },
                    },
                    "required": ["path"],
                    "additionalProperties": false,
                })
            },
            // Opening a tab does not change content, but it does change what
            // every other document tool acts on. Treating it as mutating keeps
            // Plan mode genuinely side-effect free.
            mutability: Mutability::Mutating,
            needs_session: true,
            run: |args, ctx| match arg_str(args, "path") {
                Some(path) => run_op(
                    ctx,
                    SessionOp::OpenTab {
                        path: path.to_string(),
                    },
                ),
                None => ToolOutcome::error("`path` is required and must be a non-empty string."),
            },
        },
        Tool {
            name: "write_document",
            description:
                "Replace the active tab's document with the RON given. Send the full document as \
                 returned by `read_document`, not a fragment. The result carries the validation \
                 findings for the new state, so read them and repair rather than assuming the \
                 write was clean. Nothing reaches disk until `save_all`; the write is undoable \
                 with Ctrl+Z in the editor.",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "ron": {
                            "type": "string",
                            "description": "The complete document, in the shape read_document returns.",
                        },
                    },
                    "required": ["ron"],
                    "additionalProperties": false,
                })
            },
            mutability: Mutability::Mutating,
            needs_session: true,
            run: |args, ctx| match arg_str(args, "ron") {
                Some(ron) => run_op(
                    ctx,
                    SessionOp::WriteDocument {
                        ron: ron.to_string(),
                    },
                ),
                None => ToolOutcome::error("`ron` is required and must be a non-empty string."),
            },
        },
        Tool {
            name: "save_all",
            description:
                "Write every tab's changed entries back to their own files. Run `validate` and \
                 `check_tree` first — saving a document that fails validation puts a broken file \
                 in the tree, and an unparseable file is skipped by every loader with no error \
                 at the point of use.",
            input_schema: empty_schema,
            mutability: Mutability::Mutating,
            needs_session: true,
            run: |_args, ctx| run_op(ctx, SessionOp::SaveAll),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::bridge::SessionQueue;
    use crate::agent::mode::Mode;
    use crate::agent::tools::ToolRegistry;

    #[test]
    fn every_session_tool_declares_that_it_needs_a_session() {
        for t in tools() {
            assert!(t.needs_session, "`{}` should need a session", t.name);
        }
    }

    /// The write path must be gated. If any of these ran in Plan mode the
    /// read-only guarantee would be a comment rather than a property.
    #[test]
    fn the_write_tools_are_refused_in_plan_mode() {
        let queue = SessionQueue::new();
        let ctx = ToolCtx::with_session(queue.handle());
        let reg = ToolRegistry::new();
        for name in ["open_tab", "write_document", "save_all"] {
            let out = reg.dispatch(name, &json!({}), Mode::Plan, &ctx);
            assert!(out.is_error, "`{name}` ran in Plan mode");
            assert!(out.content.contains("Plan mode"), "{}", out.content);
        }
        // Nothing should have reached the UI thread at all.
        assert!(
            queue.drain().is_empty(),
            "a refused tool still posted an op"
        );
    }

    #[test]
    fn read_tools_are_allowed_in_plan_mode_and_reach_the_queue() {
        let queue = SessionQueue::new();
        let ctx = ToolCtx::with_session(queue.handle());
        let reg = ToolRegistry::new();
        let agent =
            std::thread::spawn(move || reg.dispatch("read_document", &json!({}), Mode::Plan, &ctx));
        loop {
            for req in queue.drain() {
                assert_eq!(req.op, SessionOp::ReadDocument);
                req.reply(ToolOutcome::ok("(document)"));
            }
            if agent.is_finished() {
                break;
            }
            std::thread::yield_now();
        }
        let out = agent.join().unwrap();
        assert!(!out.is_error, "{}", out.content);
    }

    /// A headless frontend must refuse before posting, not block.
    #[test]
    fn session_tools_refuse_without_a_session() {
        let reg = ToolRegistry::new();
        let out = reg.dispatch("list_tabs", &json!({}), Mode::Build, &ToolCtx::headless());
        assert!(out.is_error);
        // Refused by the registry before the tool body runs, so nothing is
        // posted and nothing blocks.
        assert!(
            out.content.contains("needs a live editor session"),
            "{}",
            out.content
        );
    }

    #[test]
    fn missing_required_arguments_are_errors() {
        let queue = SessionQueue::new();
        let ctx = ToolCtx::with_session(queue.handle());
        let reg = ToolRegistry::new();
        for name in ["open_tab", "write_document"] {
            let out = reg.dispatch(name, &json!({}), Mode::Build, &ctx);
            assert!(out.is_error, "`{name}` accepted empty arguments");
        }
        assert!(queue.drain().is_empty(), "a bad call still posted an op");
    }
}
