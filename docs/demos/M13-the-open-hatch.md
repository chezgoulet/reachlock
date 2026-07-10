# M13 — The Open Hatch: voice, weave, a shared deck, the face, and the door ✅ (2026-07-10)

The sprint brief asked for the full vision with the budget priority
"contracts first — they are the decisions we cannot cheaply unmake; then
the face and the hatch; then two people on one deck." That is the exact
order it landed in, six commits, one per pillar.

## Phase A — the three contracts, frozen before any code (4a19b46)

Each is a versioned doc + schema + golden fixtures + a Python conformance
checker in `make check` + a new CI `contracts` job, exactly the Soul/Sim
pattern:

- **Ear Protocol v0** (`protocol/EAR-PROTOCOL.md`, port 40709): voice as
  an INPUT METHOD. Push-to-talk audio → `reachlock-eard` → transcript →
  the two doors that already exist (choice matching, then
  `perceive_utterance`). The deterministic choice matcher is defined IN
  the contract (weighted-Dice, threshold 0.5, ambiguity margin 0.1,
  closed stopword list) with `check_ear_protocol.py` as its reference
  implementation and 15 verdict cases. Design correction, noted in the
  contract: ragamuffin exposes no embedding endpoint (the brief assumed
  one), so v0 matching is lexical and engine-side; embeddings are a
  noted gap that slots behind the same verdict shape.
- **Weave contract v0** (`WEAVE-CONTRACT.md`): `woven` dialogue nodes
  carry a `may` allowlist of grants; the mind proposes
  `{line, choices[], mutations[]}`; the engine clamps or drops anything
  ungranted (never rejects wholesale), persists the resolution in the
  save's new `weaves` block BEFORE applying, and replays it forever as
  ordinary data. 11 fixtures, adversarial ones the point: the 40-point
  faction grab provably becomes the granted 3, in CI, forever.
- **Ship-Share v0** (`protocol/SHIP-SHARE.md`, port 40710): co-op crew on
  one boat, host-authoritative with the authority table written down
  (souls, sim, missions, weave resolution, RNG, the save: host's).
  Clients send intents, receive state; `share_version` stamped on every
  hello and refused loudly with both numbers. 19 payload fixtures with
  intent/state direction enforced by the checker.

## The slices (150c27a, 7e267c0, a5abd7f)

- **WeaveLoom** + DialogueRunner woven flow: generate once at the
  authority, clamp, persist, replay. Grounding refs recall from named
  vaults read-only (`MemoryStore.recall_vault`; ragamuffin untouched).
  Buffer-line latency mask identical to generated nodes. v0 transports
  the proposal as JSON in the Express body (pan can't emit invokes yet
  — the contract's pan-gap note; the invoke version changes nothing
  above it).
- **EarMatch + EarGateway + panel voice**: GDScript matcher agrees with
  the reference on every case (dsl-bridge pattern); the gateway streams
  16 kHz PCM16 chunks and folds away silently — no daemon, no V key, no
  hint, nothing greyed out. The panel shows "…listening" (lamp state,
  outranks COMPOSING…), prints the raw transcript before anything else
  touches it, fires a matched choice exactly as if clicked, and hands
  the rest to `DialogueRunner.speak_freely` (reply lands as a follow-up
  beat, choices stay up). `reachlock-eard` is a simd-sibling Go daemon:
  whisper.cpp CLI per utterance, echo engine for modelless dev/tests,
  13 wire fixtures round-tripped in Go CI, dies loudly at startup if
  the model is missing so the game side can stay silent.
- **ShipShare**: transport-free core — solo IS a hosted game with zero
  peers, every local action takes the same `handle_intent` door. Seat
  claims arbitrate first-wins with `seat_denied` naming the holder;
  claimed crew stop spawning as NPCs (the single-player substitution,
  generalized via `ShipShare.is_claimed`); station intents drive
  ShipOperation as the claimed crew member; only the hands on a station
  move its controls. RemotePawns renders the other players as their
  claimed crew, easing between 10 Hz move intents, on the ship AND on
  Sorrow. ENet is a thin pipe underneath; 13 contract tests drive golden
  payloads with zero network hardware.

## The face (dee755a)

Frames 24×32 → **32×48**, Terraria proportions: head a third of the
height, hair/headgear as identity silhouette, 3-step ramps, real stride
and arm swing and hair bounce, side views genuinely narrower. The npc
schema gained `wardrobe` (culture/work/palette/gear) and all eleven
characters are dressed by biome and trade — Reach canvas + sealed boots,
station synthetics, Earth-remnant fabric, Charter crisp, droids as
chassis (Prudence's violet-and-cyan presentation authored and honored).
`gen_pixel_art.py` consumes the block, so dressing a new NPC in data
yields a sheet for free. `CharacterSprite.FRAME` was the one engine
constant; the select-screen portrait now reads it.

## The hatch (fb9ca25)

Title screen (Continue only when a save exists; New Game asks in a
sentence before writing over a story; Join a Ship with the Tailscale
hint inline), pause menu (Esc defers to an open DialoguePanel — the
noted trap, honored and tested; "Open the Hatch" hosts LAN in one
button), settings outside the save (volume/fullscreen/typewriter +
keybind card), tracked export presets, a CI job exporting
version-stamped Linux/Windows artifacts and headless-smoking the
EXPORTED binary, and `docs/PLAYING.md` (three steps to playing, the LAN
quickstart, the honest four-sidecar table). Headless/CI boot behavior
is byte-for-byte the old contract.

## Verified

- **172/172 GUT tests** (target ≥170), green multiple consecutive runs;
  `make check` green (now including `ear`, `weave`, `share` conformance);
  dsl-bridge green; architecture guard clean; `go test ./...` green with
  the new `internal/eard` package.
- New-to-the-suite: weave loom fixture parity + fixed-point, runner-level
  weave clamping (the 40→3 case through the whole dialogue path),
  matcher parity battery, panel voice contract (match fires the choice,
  ambiguity never guesses, silence is not input, closed panel ignores
  late transcripts), ship-share seats/stations/controls/pawns, hatch
  settings + Esc ordering.

## Gaps noted (next budget, in the brief's own priority)

- **Multiplayer dialogue + world state**: `say`/`choose` intents and
  `dialogue_*`/`world` state payloads exist in the contract and the
  core emits/accepts them, but the host's DialogueRunner isn't yet
  broadcast to clients, and clients joining mid-campaign see their own
  local world load, not the host's save. The slice proves movement,
  seats, and stations; shared conversation is the next cut.
- **Pawn deck-awareness**: `move`/`pawn` payloads carry no deck field
  (v0); two players on different decks of the Loup-Garou will see each
  other's pawns through the floor. Additive field when it matters.
- **Crew routines (P2)** and **the sound of the boat (P3)**: not
  reached. Routines were next in line; better six pillars landed clean
  than a seventh rushed against the context budget.
- **Voice confidence**: whisper-cli reports no token probabilities; the
  exec engine sends 0.5 ("unknown") and v0 gates nothing on it.
- Ultra-rare test flake: the first suite run immediately after a fresh
  `--import` failed once (different test each time, never reproducible
  on rerun, only with the live dev stack up). Suspected import-cache /
  live-daemon interaction, not a code bug; CI (stackless) never saw it.

## Workflow ledger additions

- GDScript lambdas capture locals **by value** — a closure that
  decrements a shared counter silently resets it. Pass a one-element
  array (see `WeaveLoom._take`) or restructure.
- `_render_choices()` clears state before rendering: anything that must
  outlive the render (the voice matcher's `_offered` copy) must NOT be
  reset inside `_clear_choices()`.
- The architecture guard reads "base" in `ggml-base.en.bin` as a content
  id — `arch-allow` comments on the model-path lines.
- Run the GUT suite with `REACHLOCK_VAULT_PREFIX=test-` when the dev
  stack is up (vault hygiene; the suite connects to whatever's live).
- `export_presets.cfg` is now tracked deliberately (no secrets in ours;
  CI exports from it) — the .gitignore documents this.
