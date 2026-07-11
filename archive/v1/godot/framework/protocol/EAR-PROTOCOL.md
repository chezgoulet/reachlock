# The Ear Protocol — v0

The wire contract between a REACHLOCK host (the Godot engine) and the
**speech daemon** (`reachlock-eard`, a whisper.cpp wrapper run as a sidecar).
Same family as the Soul and Sim Protocols: NDJSON over TCP loopback,
versioned envelope, offline-tolerant host. Where the Soul Protocol carries
minds and the Sim Protocol carries the moving universe, this one carries the
player's **voice** — and nothing else.

**Voice is an input method, not a dialogue system.** The daemon turns held-key
audio into a transcript; the transcript enters the game as ordinary text at
exactly the two doors that already exist (choice matching, then
`perceive_utterance`). Multiplayer, saves, logging, and tests all see voice as
typed input. That one property is the whole design; every rule below protects
it.

Status: **v0** (2026-07-10). Changes bump `protocol_version` and require a
migration note — additive only. Conformance: every fixture in `ear/fixtures/`
validates against [schemas/ear_message.schema.json](schemas/ear_message.schema.json)
via `scripts/check_ear_protocol.py` (runs in `make check`), which is also the
**reference implementation of the choice matcher** — the GDScript matcher
must produce the same verdict for every case in `ear/match_cases.json`
(asserted headlessly by `godot/tests/test_ear_match.gd`, no mic, no network).

## Transport

- **TCP loopback** (`127.0.0.1`), port chosen by the host and passed to the
  daemon (`reachlock-eard --port N`; `--port 0` = OS-assigned, reported on
  stderr as `eard: bound 127.0.0.1:<port>`). Default **40709**
  (`REACHLOCK_EAR_PORT`). The sidecar family: pan 40707, simd 40708,
  eard 40709.
- **NDJSON framing**: one JSON object per line, exactly as its siblings:
  unparseable line → `error: bad_frame`, connection stays open; unknown
  `type` → `error: unknown_type`; version mismatch at `hello` →
  `error: version_unsupported`, then close.
- One connection per host process; a reconnect displaces the old one. The
  daemon holds **no state worth persisting** — an utterance in flight when
  the connection drops is simply gone (the player can say it again).

## Optional and silent — the pan rule

No daemon on the port, no model file, no microphone permission → the host's
voice affordance (the push-to-talk hint on the dialogue panel) **does not
exist**. Not greyed out, not error-logged every frame: absent. The game is
complete without it. This is the same contract pan and simd honor, and it is
load-bearing for the demo's first ninety minutes.

## Envelope

Identical to the Soul Protocol envelope:

```json
{"v": 0, "seq": 5, "re": 2, "type": "transcript", "body": { ... }}
```

`seq` sender-local monotonic; `re` names the request a response answers;
unknown *optional* envelope keys are ignored.

## Message types

| type | direction | body | reply |
|---|---|---|---|
| `hello` | host → daemon | `{protocol_version, profile, client}` | `welcome` |
| `welcome` | daemon → host | `{protocol_version, server, engine, model}` | — |
| `audio_begin` | host → daemon | `{utterance_id, sample_rate, format}` | — |
| `audio_chunk` | host → daemon | `{utterance_id, data}` | — |
| `audio_end` | host → daemon | `{utterance_id}` | `transcript` |
| `partial` | daemon → host | `{utterance_id, text}` | — |
| `transcript` | daemon → host | `{utterance_id, text, confidence, duration_ms}` | — |
| `cancel` | host → daemon | `{utterance_id}` | `ack` |
| `shutdown` | host → daemon | `{}` | `ack`, then close |
| `ack` | daemon → host | `{}` | — |
| `error` | daemon → host | `{code, message}` | — |

Notes:

- **The utterance lifecycle** is push-to-talk shaped: key down →
  `audio_begin`; while held, `audio_chunk`s (host paces them, ~250 ms of
  audio per chunk); key up → `audio_end`; the daemon decodes and answers
  one `transcript`. `utterance_id` is host-chosen (`utt_<n>`), scopes every
  message, and lets the host drop a late transcript for an utterance it no
  longer cares about — the Soul Protocol's supersession idea, one letter
  simpler.
- **Audio format is fixed in v0**: `sample_rate: 16000`, `format: "pcm16"`
  — mono 16-bit little-endian PCM, base64 in `data`. (Whisper wants 16 kHz
  mono; the host resamples from the capture bus. `audio_begin` carries the
  fields anyway so a future version can negotiate instead of migrate.)
- **`partial` is optional** — a daemon MAY stream interim text while
  decoding (streaming STT is a noted gap, below); a v0 host renders it if
  it arrives and loses nothing if it never does. Hosts MUST tolerate zero
  or many partials before the final `transcript`.
- `transcript.confidence` ∈ [0, 1] (whisper avg token probability, or the
  engine's nearest equivalent). `text` MAY be empty — silence is a valid
  thing to have said, and the host treats it as no input.
- `cancel` aborts an in-flight utterance (tap instead of hold, dialogue
  closed mid-listen). The daemon MUST NOT send a `transcript` for a
  cancelled utterance; a `transcript` and a `cancel` that cross on the
  wire resolve in the host's favor (the host already dropped the id).
- Error codes (closed set for v0): `bad_frame`, `unknown_type`,
  `version_unsupported`, `unknown_utterance`, `invalid_args`, `no_model`,
  `decode_failure`.

## What the host does with a transcript

This section is the contract that keeps voice an input method. In order:

1. **Show it.** The dialogue panel shows "…listening" from key-down and the
   raw transcript the moment it arrives — BEFORE any matching, any mind,
   any latency. The dialogue latency contract is untouchable; STT time
   stacks on top of it, so the mask matters more, not less.
2. **Match it** against the offered choices (the matcher below). A match
   fires that choice **deterministically, exactly as if clicked** — same
   mutations, same goto, same transcript entry. The choice's authored text
   is what enters the record, not the player's phrasing.
3. **No match, live mind** → the transcript goes to
   `SoulInstance.perceive_utterance` verbatim — the free-speech path that
   already exists, buffer-line latency mask and all.
4. **No match, no mind** → the panel keeps the transcript on screen ("you
   said:") and the choices stay up. Nothing is lost, nothing pretends.

In multiplayer (SHIP-SHARE.md), STT runs on each player's own machine and
only the **transcript text** travels as an intent; matching runs at the
host against the choices the host offered. Voice never adds a second
network path.

## The choice matcher (engine-side, deterministic)

Semantic embedding matching was the aspiration; neither ragamuffin nor pan
exposes an embedding endpoint today (noted gap, below). v0 ships a
deterministic lexical matcher — same input, same verdict, everywhere: on
the host, on a replay, in CI with neither mic nor network.

Algorithm (normative — the Python reference in
`scripts/check_ear_protocol.py` and `godot/scripts/framework/ear_match.gd`
must agree on every case in `ear/match_cases.json`):

1. **Normalize** transcript and each choice text: lowercase; strip
   apostrophes (`don't` → `dont`); every other non-alphanumeric rune
   becomes a space; split on whitespace.
2. **Weigh** each token: stopwords (the closed list in the reference
   implementation) weigh 0.25, everything else 1.0.
3. **Score** each choice with weighted Dice overlap:
   `2 · w(T ∩ C) / (w(T) + w(C))` over token *sets* (T transcript,
   C choice; `w` sums weights).
4. **Verdict**: the best-scoring choice matches iff its score ≥ **0.5**
   AND it beats the runner-up by ≥ **0.1** (a transcript that lands
   between two choices matches nothing — ambiguity is the player's to
   resolve, never the matcher's). Otherwise: no match (index −1).

Empty transcripts, empty choice lists, and all-stopword transcripts are
no-matches by construction. The threshold constants are part of the
contract; tuning them is a `match_cases.json` change reviewed like any
schema change, not a code tweak.

## Versioning & evolution

`protocol_version` is a single integer; additive optional fields do not
bump it, anything else does. The daemon replies `welcome` with the version
it will speak; a host that can't speak it disconnects cleanly.

Noted gaps (design for them, do not build them in v0):

- **Streaming STT** — `partial` is specified so a streaming engine slots in
  without a protocol change; whisper.cpp v0 decodes at `audio_end`.
- **Semantic matching** — when an embedding endpoint exists (ragamuffin
  exposes one, or eard grows a local model), it becomes a *second* matcher
  behind the same verdict shape, and `match_cases.json` grows the cases
  lexical matching can't reach. The deterministic-verdict property must
  survive: embeddings would run at the authority (host), never per-client.
- **Wake words / open mic** — out of scope; push-to-talk is a design
  choice (deliberate speech, no accidental hot mic), not a limitation.
