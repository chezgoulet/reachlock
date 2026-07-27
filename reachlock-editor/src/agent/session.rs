//! The agent loop (S101 P5).
//!
//! Runs on its own thread. Owns the conversation and the provider; reaches the
//! editor only through [`super::bridge`]. Emits transcript events back to the
//! UI over a channel, so the panel renders progress as it happens rather than
//! freezing until the whole task is done.
//!
//! The loop itself is small on purpose:
//!
//! ```text
//!   send request (system + history + tools for the current mode)
//!   ├─ no tool calls  → done, append the text
//!   └─ tool calls     → run each, append the results, go again
//! ```
//!
//! What makes it useful is not the loop, it is that every mutating tool hands
//! back its validation findings — so "generate, then repair against the
//! errors" is the default path rather than something the author has to drive.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::bridge::SessionHandle;
use super::mode::Mode;
use super::provider::{Message, Part, Provider, Request, Role, StopReason, ToolDef, ToolResult};
use super::tools::{ToolCtx, ToolRegistry};

/// Stop after this many provider round trips in one task.
///
/// A model that keeps calling tools without converging would otherwise spend
/// the author's tokens indefinitely. Hitting the cap is reported, not hidden —
/// silently truncating looks like the model gave up for its own reasons.
const MAX_TURNS: usize = 24;

/// What the loop tells the UI. One variant per thing worth rendering.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// What the author asked. Emitted by the UI rather than the loop, so the
    /// panel reads as a conversation instead of a series of answers with no
    /// questions.
    UserPrompt(String),
    /// Assistant prose.
    Text(String),
    /// A tool is about to run. Rendered before the result so a slow tool shows
    /// as in-flight rather than as nothing at all.
    ToolStarted {
        name: String,
        summary: String,
    },
    ToolFinished {
        name: String,
        is_error: bool,
    },
    /// The task ended. `Err` carries a message already fit to show.
    Done(Result<(), String>),
}

/// Handle to a running task.
pub struct AgentTask {
    pub events: Receiver<AgentEvent>,
}

/// State that outlives a single Send.
///
/// Each Send used to start from `vec![Message::user(prompt)]`, so the
/// assistant had no memory of the previous exchange: a follow-up like "now do
/// the same for the other one" landed with no idea what "the same" was, and
/// every send re-explored the tree from nothing. That is the opposite of what
/// is wanted from a tool whose main cost is rediscovery.
///
/// Shared with the agent thread, which appends as it goes, so the panel can
/// report how large the conversation has grown.
#[derive(Default)]
pub struct Conversation {
    pub messages: Vec<Message>,
    log: Option<TranscriptLog>,
    pub log_path: Option<std::path::PathBuf>,
    /// The mode the previous send ran under, so a switch can be made explicit
    /// to a model that has already been told the other set of rules.
    last_mode: Option<Mode>,
}

impl Conversation {
    /// Roughly how much has accumulated. Characters, not tokens: the point is
    /// to warn an author before a context limit bites, not to be exact.
    pub fn size_hint(&self) -> usize {
        self.messages
            .iter()
            .map(|m| match m {
                Message::Turn { parts, .. } => parts
                    .iter()
                    .map(|p| match p {
                        Part::Text(t) => t.len(),
                        Part::Image { data, .. } => data.len(),
                    })
                    .sum::<usize>(),
                Message::ToolResults(rs) => rs.iter().map(|r| r.content.len()).sum(),
            })
            .sum()
    }

    pub fn turns(&self) -> usize {
        self.messages.len()
    }

    /// Start over: new history, new log file.
    pub fn reset(&mut self) {
        *self = Conversation::default();
    }
}

/// Append-only transcript on disk.
///
/// Reading a session back out of the egui panel means selecting it with the
/// mouse, which is miserable for anything longer than a few lines. Every
/// session is written to `save/agent-logs/` as it happens, so a failed run can
/// be read, diffed and pasted with ordinary tools — and survives closing the
/// editor.
struct TranscriptLog {
    file: Option<std::fs::File>,
}

impl TranscriptLog {
    fn create(mode: Mode, provider: &str) -> (Self, Option<std::path::PathBuf>) {
        let dir = std::path::PathBuf::from("save/agent-logs");
        if std::fs::create_dir_all(&dir).is_err() {
            return (TranscriptLog { file: None }, None);
        }
        // Seconds sort readably; the counter makes the name unique. Seconds
        // alone are not: two sessions started in the same second and the same
        // mode produced the same filename, and `File::create` truncates — so
        // the second run silently destroyed the first one's log. Found by two
        // tests colliding, but an author kicking off a retry right after a
        // failure would hit exactly the same thing, and would lose precisely
        // the log worth keeping.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!(
            "{stamp}-{n:03}-{}.log",
            mode.label().to_lowercase()
        ));
        match std::fs::File::create(&path) {
            Ok(mut file) => {
                use std::io::Write as _;
                let _ = writeln!(file, "mode: {}\nprovider: {provider}\n", mode.label());
                (TranscriptLog { file: Some(file) }, Some(path))
            }
            // Logging is a convenience, never a reason to refuse the task.
            Err(_) => (TranscriptLog { file: None }, None),
        }
    }

    fn write_prompt(&mut self, prompt: &str) {
        use std::io::Write as _;
        if let Some(file) = self.file.as_mut() {
            let _ = write!(file, "\n== prompt ==\n{prompt}\n");
            let _ = file.flush();
        }
    }

    fn write(&mut self, event: &AgentEvent) {
        use std::io::Write as _;
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let line = match event {
            // The prompt is written by `write_prompt` when the send starts,
            // so it is on disk even if the model never answers.
            AgentEvent::UserPrompt(_) => String::new(),
            AgentEvent::Text(t) => format!("\n== assistant ==\n{t}\n"),
            AgentEvent::ToolStarted { summary, .. } => format!("-> {summary}\n"),
            AgentEvent::ToolFinished { name, is_error } => {
                if *is_error {
                    format!("   {name}: FAILED\n")
                } else {
                    String::new()
                }
            }
            AgentEvent::Done(Ok(())) => "\n== done ==\n".to_string(),
            AgentEvent::Done(Err(e)) => format!("\n== stopped ==\n{e}\n"),
        };
        if !line.is_empty() {
            let _ = file.write_all(line.as_bytes());
            // Flush per event: the interesting case is a session that hung or
            // was killed, and a buffered tail would lose exactly that.
            let _ = file.flush();
        }
    }
}

fn system_prompt(mode: Mode) -> String {
    let base = "You are a content-authoring assistant working inside the ReachLock content \
         editor, on a procedurally generated spacefaring game.\n\n\
         How to find your way around, so you do not spend turns rediscovering it:\n\
         - `describe_content_type` gives you a type's directory, its schema, and the path of a \
           real example, in one call. Start there for any type you have not touched yet.\n\
         - `query_content` returns each id **with the file it lives in**. Take paths from there \
           and pass them to `read_file` unchanged. Do not construct a path: a directory name is \
           not the kind name (planet cultures live in `cultures/`), and an id is not always the \
           filename (the ecosystem `frontier_valis` is in `ecosystems/frontier_sample.ron`).\n\
         - A `read_file` that fails means the path is wrong, not that the content is missing. \
           Go back to `query_content` rather than trying another spelling.\n\n\
         Ground rules that matter more than they look:\n\
         - Never invent an id. Use what `query_content` reports; an id nothing defines is a \
           dangling reference that fails the project's build.\n\
         - Read an existing file of a type before authoring another one. RON has traps a \
           schema does not show: a fixed-size array serializes as a tuple, a newtype needs \
           its parens, enum variants are snake_case, and most payloads must be wrapped in a \
           ContentFile envelope. A file that gets any of these wrong is skipped silently by \
           every loader.\n\
         - After any write, read the validation findings in the result and fix what they \
           say before moving on. Do not report success on an unvalidated write.\n\
         - The engine must not name specific content. Ships, crew, and stories are things a \
           player picks or an author writes, never something engine code hardcodes.\n\n\
         Work in small steps. If a task is large, do the first piece well and say what is \
         left rather than exploring until you run out of turns.";
    match mode {
        Mode::Plan => format!(
            "{base}\n\n\
             You are in PLAN mode. Every tool you have is read-only. Investigate and \
             propose — say concretely what you would change, in which files, and why. Do \
             not ask to switch modes; the author does that."
        ),
        Mode::Build => format!(
            "{base}\n\n\
             You are in BUILD mode. You may open tabs, write documents, and save. Work in \
             small steps and validate as you go. Nothing reaches disk until you call \
             save_all, and the author can undo any write."
        ),
    }
}

fn tool_defs(registry: &ToolRegistry, mode: Mode, has_session: bool) -> Vec<ToolDef> {
    registry
        .available(mode, has_session)
        .into_iter()
        .map(|t| ToolDef {
            name: t.name.to_string(),
            description: t.description.to_string(),
            input_schema: (t.input_schema)(),
        })
        .collect()
}

/// One-line rendering of a tool call, for the transcript.
fn summarize(name: &str, args: &serde_json::Value) -> String {
    let interesting = ["path", "id", "kind", "contains"];
    let shown: Vec<String> = interesting
        .iter()
        .filter_map(|k| {
            args.get(k)
                .and_then(|v| v.as_str())
                .map(|v| format!("{k}: {v}"))
        })
        .collect();
    if shown.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({})", shown.join(", "))
    }
}

/// Spawn a task. Returns immediately; progress arrives on the channel.
pub fn spawn(
    provider: Box<dyn Provider>,
    session: SessionHandle,
    mode: Mode,
    max_tokens: u32,
    prompt: String,
    conversation: Arc<Mutex<Conversation>>,
) -> AgentTask {
    let (tx, rx) = channel();
    {
        let mut c = conversation.lock().unwrap_or_else(|e| e.into_inner());
        // One log per conversation, not per send: a transcript split across
        // files at every prompt is exactly as hard to read as no transcript.
        if c.log.is_none() {
            let (log, path) = TranscriptLog::create(mode, provider.name());
            c.log = Some(log);
            c.log_path = path;
        }
        // A mid-conversation mode switch changes the rules under a model that
        // was told the previous set. Saying so beats letting it infer the
        // change from a refusal.
        if let Some(previous) = c.last_mode {
            if previous != mode {
                c.messages.push(Message::user(format!(
                    "(The author switched to {} mode. {})",
                    mode.label(),
                    match mode {
                        Mode::Plan => "Your tools are read-only again.",
                        Mode::Build => "You may write and save now.",
                    }
                )));
            }
        }
        c.last_mode = Some(mode);
        c.messages.push(Message::user(prompt.clone()));
        if let Some(log) = c.log.as_mut() {
            log.write_prompt(&prompt);
        }
    }
    std::thread::spawn(move || {
        run(provider, session, mode, max_tokens, &tx, conversation);
    });
    AgentTask { events: rx }
}

/// Emit an event to the UI and the log together, so the file is never a
/// partial record of what the panel showed.
fn emit(tx: &Sender<AgentEvent>, conversation: &Mutex<Conversation>, event: AgentEvent) {
    if let Ok(mut c) = conversation.lock() {
        if let Some(log) = c.log.as_mut() {
            log.write(&event);
        }
    }
    let _ = tx.send(event);
}

fn run(
    provider: Box<dyn Provider>,
    session: SessionHandle,
    mode: Mode,
    max_tokens: u32,
    tx: &Sender<AgentEvent>,
    conversation: Arc<Mutex<Conversation>>,
) {
    let registry = ToolRegistry::new();
    let ctx = ToolCtx::with_session(session);

    if !provider.caps().tools {
        emit(
            tx,
            &conversation,
            AgentEvent::Done(Err(format!(
                "The `{}` profile is not marked as supporting tool calling, so it cannot drive \
             the editor. Tick \"Supports tool calling\" in AI Settings if the model does \
             support it, or pick a profile that does. The one-shot Generate button works \
             with any model.",
                provider.name()
            ))),
        );
        return;
    }

    // The mode is captured once, at spawn. Flipping it mid-task would change
    // the rules under a model that has already been told what they are; the
    // dispatcher still re-checks every call, so this is a consistency choice,
    // not the safety boundary.
    let tools = tool_defs(&registry, mode, true);
    let system = system_prompt(mode);

    /// Append to the shared history. A poisoned lock means another thread
    /// panicked mid-update; the history is still structurally fine to append
    /// to, and losing the conversation on top of that helps nobody.
    fn push(conversation: &Mutex<Conversation>, message: Message) {
        conversation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .messages
            .push(message);
    }

    for turn in 0..MAX_TURNS {
        let messages = conversation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .messages
            .clone();
        let request = Request {
            system: system.clone(),
            messages,
            tools: tools.clone(),
            max_tokens,
            temperature: 0.7,
        };

        let response = match provider.complete(&request) {
            Ok(r) => r,
            Err(e) => {
                emit(tx, &conversation, AgentEvent::Done(Err(e.to_string())));
                return;
            }
        };

        if !response.text.trim().is_empty() {
            emit(tx, &conversation, AgentEvent::Text(response.text.clone()));
        }

        if response.tool_calls.is_empty() {
            // Record the closing turn. Without it the next Send resumes from a
            // history that contains the questions but none of the answers, so
            // the model contradicts its own previous reply for no visible
            // reason — worse than having no memory at all.
            if !response.text.trim().is_empty() {
                push(
                    &conversation,
                    Message::Turn {
                        role: Role::Assistant,
                        parts: vec![Part::Text(response.text.clone())],
                        tool_calls: Vec::new(),
                    },
                );
            }
            emit(
                tx,
                &conversation,
                AgentEvent::Done(match response.stop {
                    StopReason::MaxTokens => Err(
                        "The model hit its token ceiling mid-answer. Raise Max tokens in AI \
                     Settings, or ask for a smaller step."
                            .into(),
                    ),
                    StopReason::Other(why) => Err(format!("The model stopped early ({why}).")),
                    _ => Ok(()),
                }),
            );
            return;
        }

        // Record the assistant's turn *including* its tool calls before
        // running them. Both wire formats require the calls to be present in
        // the history the results answer; omitting them makes the next request
        // a protocol error rather than a silently worse one.
        push(
            &conversation,
            Message::Turn {
                role: Role::Assistant,
                parts: if response.text.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![Part::Text(response.text.clone())]
                },
                tool_calls: response.tool_calls.clone(),
            },
        );

        let mut results = Vec::new();
        for call in &response.tool_calls {
            emit(
                tx,
                &conversation,
                AgentEvent::ToolStarted {
                    name: call.name.clone(),
                    summary: summarize(&call.name, &call.arguments),
                },
            );
            let outcome = registry.dispatch(&call.name, &call.arguments, mode, &ctx);
            emit(
                tx,
                &conversation,
                AgentEvent::ToolFinished {
                    name: call.name.clone(),
                    is_error: outcome.is_error,
                },
            );
            results.push(ToolResult {
                call_id: call.id.clone(),
                content: outcome.content,
                // Images are dropped for a model that cannot read them (P6
                // gates on `caps().vision`), rather than sent and rejected.
                parts: if provider.caps().vision {
                    outcome
                        .images
                        .into_iter()
                        .map(|(media_type, data)| Part::Image { media_type, data })
                        .collect()
                } else {
                    Vec::new()
                },
                is_error: outcome.is_error,
            });
        }

        // All results for the turn go back as one message. Splitting them is
        // accepted by both APIs and then quietly suppresses parallel tool
        // calls on later turns.
        push(&conversation, Message::ToolResults(results));

        if turn + 1 == MAX_TURNS {
            emit(
                tx,
                &conversation,
                AgentEvent::Done(Err(format!(
                    "Stopped after {MAX_TURNS} turns without finishing. If most of those turns \
                 were spent looking things up, `describe_content_type` returns a type's \
                 directory, schema and an example in one call — otherwise ask for a \
                 narrower step."
                ))),
            );
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::bridge::SessionQueue;
    use crate::agent::provider::{Caps, ProviderError, Response, ToolCall};
    use std::sync::Mutex;

    /// A provider that replays a scripted list of responses.
    struct Scripted {
        caps: Caps,
        responses: Mutex<Vec<Response>>,
        seen: Mutex<Vec<Request>>,
    }

    impl Provider for Scripted {
        fn name(&self) -> &str {
            "scripted"
        }
        fn caps(&self) -> Caps {
            self.caps
        }
        fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
            self.seen.lock().unwrap().push(req.clone());
            let mut rs = self.responses.lock().unwrap();
            if rs.is_empty() {
                return Err(ProviderError::Protocol("script exhausted".into()));
            }
            Ok(rs.remove(0))
        }
        fn test_connection(&self) -> Result<Option<String>, String> {
            Ok(None)
        }
    }

    fn text_only(text: &str) -> Response {
        Response {
            text: text.into(),
            tool_calls: Vec::new(),
            stop: StopReason::EndTurn,
        }
    }

    fn drain(rx: &Receiver<AgentEvent>) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
            let done = matches!(e, AgentEvent::Done(_));
            out.push(e);
            if done {
                break;
            }
        }
        out
    }

    #[test]
    fn a_text_only_answer_finishes_in_one_turn() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![text_only("here is the plan")]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Plan,
            1024,
            "plan something".into(),
            convo.clone(),
        );
        let events = drain(&task.events);
        assert_eq!(events[0], AgentEvent::Text("here is the plan".into()));
        assert_eq!(events.last(), Some(&AgentEvent::Done(Ok(()))));
    }

    /// A profile that has not declared tool support cannot drive the editor,
    /// and must say so instead of sending tools the endpoint will reject.
    #[test]
    fn a_profile_without_tool_support_is_refused_up_front() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: false,
            },
            responses: Mutex::new(vec![text_only("unreachable")]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Build,
            1024,
            "do something".into(),
            convo.clone(),
        );
        let events = drain(&task.events);
        match events.last() {
            Some(AgentEvent::Done(Err(msg))) => {
                assert!(msg.contains("tool calling"), "{msg}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(events.len() == 1, "nothing should have been sent");
    }

    /// Plan mode must not advertise a mutating tool, and the dispatcher must
    /// refuse one anyway if the model asks.
    #[test]
    fn plan_mode_neither_offers_nor_runs_a_write() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![
                Response {
                    text: String::new(),
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "write_document".into(),
                        arguments: serde_json::json!({"ron": "()"}),
                    }],
                    stop: StopReason::ToolUse,
                },
                text_only("understood"),
            ]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Plan,
            1024,
            "change it".into(),
            convo.clone(),
        );
        let events = drain(&task.events);

        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ToolFinished { name, is_error: true } if name == "write_document"
            )),
            "the write should have been refused: {events:?}"
        );
        // And nothing reached the UI thread.
        assert!(
            queue.drain().is_empty(),
            "a refused write still posted an op"
        );
    }

    #[test]
    fn a_tool_call_round_trips_and_the_history_keeps_the_call() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![
                Response {
                    text: "looking".into(),
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "check_tree".into(),
                        arguments: serde_json::json!({}),
                    }],
                    stop: StopReason::ToolUse,
                },
                text_only("the tree is clean"),
            ]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Plan,
            1024,
            "check".into(),
            convo.clone(),
        );
        let events = drain(&task.events);
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == "check_tree")));
        assert_eq!(events.last(), Some(&AgentEvent::Done(Ok(()))));
    }

    #[test]
    fn a_truncated_answer_is_reported_rather_than_treated_as_finished() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![Response {
                text: "half an ans".into(),
                tool_calls: Vec::new(),
                stop: StopReason::MaxTokens,
            }]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Build,
            16,
            "write a lot".into(),
            convo.clone(),
        );
        let events = drain(&task.events);
        match events.last() {
            Some(AgentEvent::Done(Err(msg))) => assert!(msg.contains("token ceiling"), "{msg}"),
            other => panic!("expected a truncation report, got {other:?}"),
        }
    }

    /// The bug this whole type exists for: every Send used to start from
    /// `vec![Message::user(prompt)]`, so a follow-up arrived with no idea what
    /// the previous exchange had been, and the model re-explored from nothing.
    #[test]
    fn a_second_send_carries_the_first_exchange() {
        let queue = SessionQueue::new();
        let convo = Arc::new(Mutex::new(Conversation::default()));

        let first = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![text_only("the ecosystem lives in ecosystems/")]),
            seen: Mutex::new(Vec::new()),
        });
        let task = spawn(
            first,
            queue.handle(),
            Mode::Plan,
            1024,
            "where do ecosystems live?".into(),
            convo.clone(),
        );
        let _ = drain(&task.events);

        // Second send, same conversation.
        let second = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![text_only("as I said, ecosystems/")]),
            seen: Mutex::new(Vec::new()),
        });
        let task = spawn(
            second,
            queue.handle(),
            Mode::Plan,
            1024,
            "and the one after that?".into(),
            convo.clone(),
        );
        let _ = drain(&task.events);

        let c = convo.lock().unwrap();
        let text: String = format!("{:?}", c.messages);
        assert!(
            text.contains("where do ecosystems live?"),
            "the first prompt was dropped:\n{text}"
        );
        assert!(
            text.contains("the ecosystem lives in ecosystems/"),
            "the first answer was dropped — a follow-up would contradict it:\n{text}"
        );
        assert!(
            text.contains("and the one after that?"),
            "the second prompt is missing:\n{text}"
        );
        assert!(c.turns() >= 4, "expected both exchanges, got {}", c.turns());
    }

    /// Switching mode mid-conversation changes the rules under a model that
    /// was told the other set; it should be told, not left to infer it from a
    /// refusal.
    #[test]
    fn a_mode_switch_is_announced_in_the_history() {
        let queue = SessionQueue::new();
        let convo = Arc::new(Mutex::new(Conversation::default()));
        for (mode, reply) in [(Mode::Plan, "planned"), (Mode::Build, "built")] {
            let p = Box::new(Scripted {
                caps: Caps {
                    vision: false,
                    tools: true,
                },
                responses: Mutex::new(vec![text_only(reply)]),
                seen: Mutex::new(Vec::new()),
            });
            let task = spawn(p, queue.handle(), mode, 1024, "go".into(), convo.clone());
            let _ = drain(&task.events);
        }
        let text = format!("{:?}", convo.lock().unwrap().messages);
        assert!(
            text.contains("switched to Build mode"),
            "the mode change was not announced:\n{text}"
        );
    }

    /// Starting over must actually start over.
    #[test]
    fn a_reset_clears_history_and_the_log() {
        let mut c = Conversation::default();
        c.messages.push(Message::user("something"));
        assert_eq!(c.turns(), 1);
        c.reset();
        assert_eq!(c.turns(), 0);
        assert!(c.log_path.is_none());
    }

    /// A session that hangs or is killed is exactly the one worth reading, so
    /// the log must be on disk and flushed as it goes, not at the end.
    #[test]
    fn the_session_is_written_to_a_log_file() {
        let queue = SessionQueue::new();
        let p = Box::new(Scripted {
            caps: Caps {
                vision: false,
                tools: true,
            },
            responses: Mutex::new(vec![text_only("a written answer")]),
            seen: Mutex::new(Vec::new()),
        });
        let convo = Arc::new(Mutex::new(Conversation::default()));
        let task = spawn(
            p,
            queue.handle(),
            Mode::Plan,
            1024,
            "log this".into(),
            convo.clone(),
        );
        let _ = drain(&task.events);

        let path = convo
            .lock()
            .unwrap()
            .log_path
            .clone()
            .expect("a log path was reported");
        let text = std::fs::read_to_string(&path).expect("log file exists");
        assert!(text.contains("mode: Plan"), "{text}");
        assert!(text.contains("log this"), "prompt missing:\n{text}");
        assert!(text.contains("a written answer"), "reply missing:\n{text}");
        assert!(text.contains("== done =="), "outcome missing:\n{text}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_summary_names_the_interesting_argument() {
        let s = summarize("query_content", &serde_json::json!({"kind": "soul"}));
        assert_eq!(s, "query_content (kind: soul)");
        assert_eq!(
            summarize("check_tree", &serde_json::json!({})),
            "check_tree"
        );
    }
}
