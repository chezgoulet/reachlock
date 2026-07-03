# Soul Protocol integration test harness

The executable end-to-end specification of the Godot ⇄ Pan boundary. It
boots a **real `pan serve` subprocess**, connects over TCP loopback, and
walks the full Soul Protocol lifecycle using the 15 shared fixtures as the
expected wire traffic — then exercises every error path in the protocol's
closed code set. If this passes, the host and the mind daemon agree on the
contract, byte for byte where determinism allows.

The narrative companion is [docs/SYSTEM-INTEGRATION.md](../../docs/SYSTEM-INTEGRATION.md);
the wire contract is
[godot/framework/protocol/SOUL-PROTOCOL.md](../../godot/framework/protocol/SOUL-PROTOCOL.md).

## Run it

```sh
# Prerequisites: python3 (3.10+, stdlib only) and a pan checkout as a
# sibling of this repo (github.com/chezgoulet/pan), built or buildable.
python3 tests/soul-protocol-harness/run_harness.py
```

The harness finds the pan binary via `--pan-bin`, then `$PAN_BIN`, then
`../pan/target/{release,debug}/pan`, and as a last resort runs
`cargo build --bin pan` in a `../pan` checkout. Exit code 0 means every
step passed; 1 means at least one failed (per-step output says which, and
the pan stderr tail is printed for post-mortems).

No ports are hardcoded: `pan serve --port 0` lets the OS assign one, and
the harness parses the bound port from the daemon's stderr. The stub LLM
(below) also binds port 0.

## What it checks

**Connection 1 — the full lifecycle.** `hello` → `welcome` (exact fixture 02
body), `register_capabilities` → `ack`, `instantiate_soul` → `ack`,
`perceive` (utterance) → `decision` (fixture 06's Express line, exactly),
`perceive` (superseding revision) → `decision` carrying revision 2,
`perceive` (event) → `decision` (exact fixture 08 body), tick and signal
perceives, the unknown-capability enact rejection (conformance case 09),
`release_soul` → `ack`, perceive-after-release → `unknown_soul`, and
`shutdown` → `ack` + clean close.

**Connection 2 — error paths.** Malformed JSON → `bad_frame`; unknown
message type → `unknown_type`; perceive for a never-instantiated soul →
`unknown_soul` — all on one connection, proving rejected lines don't kill
the session.

**Connection 3 — version mismatch.** `hello` with `protocol_version: 99` →
`version_unsupported`, then the daemon closes the connection.

Plus: the fixture copies in `../pan/pan-daemon/tests/fixtures/` must be
**byte-identical** to `godot/framework/protocol/fixtures/` (skipped with a
warning when no pan checkout is present), and daemon envelopes must keep
`seq` strictly increasing with `re` correlating each reply to its request.

## How the LLM steps are deterministic

Fixture 04 instantiates `example_pilot` with `mind: llm`. The harness runs
a **stub OpenAI-compatible server** (stdlib `http.server`, port 0) that
always answers with fixture 06's exact Express line, and points
`PAN_LLM_BASE` at it. Pan takes the same code path a live Ollama exercises
— resolve, dialect probe, chat completion, `clean_line` — but the reply is
canonical, so the harness compares the Express text byte-for-byte.

For the event/signal/tick steps the harness re-instantiates `example_pilot`
as a `rules` mind (protocol-legal; `instantiate_soul` replaces the mind)
whose rule list makes fixture 08 an **exact full-body match**.

## The comparison contract

Fixtures are canonical *shapes*, not session transcripts. The harness sends
every host→daemon fixture byte-for-byte and compares daemon replies:

| daemon message | comparison |
|---|---|
| `welcome` (02), `ack` (13) | exact body |
| `decision` for 07 (fixture 08) | exact body |
| `decision` for 05 (fixture 06) | exact, **minus fixture 06's `invoke` intents** — see below |
| `error` bodies | `code` exact (the closed set is contract); `message` must name the offender but its wording is informational |
| envelope | `v == 0`; daemon `seq` strictly increasing; `re` equals the request's `seq` (absent on parse-reject replies) |

**The one documented divergence:** fixture 06 includes an
`invoke: npc.remember` intent — the full wire shape a tool-calling mind
will emit. Pan's v0 LLM provider emits Express + Conclude only (LLM
tool-invokes are the M7 conversation-memory work). The harness therefore
expects fixture 06 *without* its invoke intents, and states so in its
output. When Pan starts emitting invokes, this comparison fails loudly and
must be updated deliberately — that is a contract change, not noise.

## CI

Runs as the `soul-protocol-harness` job in `.github/workflows/ci.yml`: it
checks out `chezgoulet/pan` as a sibling, builds the daemon, and runs this
script — after the unit-level jobs, as a separate step. A PR that breaks
the harness is not reviewed; it is rejected.
