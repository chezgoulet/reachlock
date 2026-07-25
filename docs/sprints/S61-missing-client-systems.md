# S61 — Missing Client Systems

**Spec:** §14 (three-mode gameplay) · **Wave 17 (Client Polish) · Depends on:** S60

## Outcome

Every gameplay mode in the spec has a corresponding client system. Resource gathering (foraging/mining), signature collection (contract co-signing), and deliberation rendering (LLM thinking state) each have dedicated systems. The contract library no longer shows placeholder text.

## Context

- `reachlock-client/src/systems/` has 44 files. Most of the spec-defined gameplay loops are covered. Three have been identified as gaps by the audits:
  - Resource gathering (spec §14 Mode 1: "forage for supplies, mine asteroids, harvest from planets")
  - Signature collector (spec §6: "contract evaluation signatures for online verification") — may be embedded in `contract.rs` but not surfaced as a dedicated UI
  - Deliberation renderer (spec §15: "visual deliberation state for every LLM call") — may be embedded in `dialogue.rs` or `comms.rs`
- `systems/contract_library.rs:174` shows `state.status = "imported to workshop (placeholder)"`.

## Freeze first

1. New systems follow the existing pattern: one file per system, a `Resource` type (if state is needed), a `Plugin` or `schedule` block, and at least one system function.
2. Deliberation rendering is a UI overlay — not a re-architecture. It reads the LLM call state from the existing deliberation resource and renders a visible indicator.

## Deliverables

### 1. Resource gathering system (`systems/resource_gathering.rs`)

- [ ] **ResourceNode component** — spawned on interactable objects in the game world (asteroid fields, harvestable flora, mineral deposits). Properties: `resource_type` (mineral, organic, gas, water), `yield_amount`, `difficulty` (gathering time in ticks), `depleted` flag.
- [ ] **Gathering interaction** — when the player presses the interact key near a `ResourceNode`, the system initiates a gathering timer. Timer duration = `difficulty` × (1.0 - crew_bonus). While gathering, a progress bar shows above the node.
- [ ] **Crew role bonus** — crew with relevant roles (miner, biologist, engineer) reduce gathering time. Bonuses from soul file `skills` field. No crew = baseline time.
- [ ] **Inventory integration** — gathered resources go into the ship's cargo hold. If cargo is full, gathering fails with "cargo hold full" notification.
- [ ] **Resource depletion** — nodes have a `max_yield` count. Each gather reduces it. At 0, the node is marked `depleted` and visually changes (asteroid breaks apart, plant wilts, etc.). Depleted nodes respawn after a configurable tick count.

### 2. Signature collector system (`systems/signature_collector.rs`)

- [ ] **SignatureCollector resource** — holds the list of contract evaluations awaiting signatures for the current session. Each entry: `contract_id`, `action_description`, `evaluated_by` (NPC or player), `signature_hash`, `signed` (bool), `rejected` (bool).
- [ ] **UI panel** — shows pending signatures when the player has contracts with pending evaluations. Panel shows: contract label, action taken, who evaluated it, timestamp. "Sign" and "Reject" buttons.
- [ ] **Online submission** — when signed, the evaluation + signature is sent to the server via `ClientMessage::EvalSubmit`. Online mode: server verifies and records. Offline mode: signature is stored locally.
- [ ] **Notification** — when a new signature request arrives (from crew deliberation or contract evaluation completion), a small notification appears: "Boris has signed off on course deviation. Review and countersign."

### 3. Deliberation renderer (`systems/deliberation_renderer.rs`)

- [ ] **DeliberationState resource** — already exists in the core/contract system as a shared state. This system reads it and renders.
- [ ] **Deliberation indicator** — when any LLM call is in progress, a persistent UI element appears: top-center of screen, "Crew is considering..." with a pulsing animation. Below it: the crew member's name and what they're considering (from the deliberation context).
- [ ] **Multiple deliberation tracks** — if multiple crew members are deliberating simultaneously (S33 concurrent crew deliberation), show up to 3 tracks stacked vertically. Each shows: portrait icon, name, "Considering: {context_summary}".
- [ ] **Deliberation result flash** — when a deliberation completes, the track briefly flashes green (success) or red (failure/misinterpretation) and shows the outcome for 3 seconds before fading.
- [ ] **Toggle** — deliberation UI can be toggled off in settings (S31). Default: always visible (per spec §5: "Every LLM call has a visible deliberation state. No silent inference.").

### 4. Fix contract library placeholder

- [ ] **`systems/contract_library.rs:174`** — replace `state.status = "imported to workshop (placeholder)".into()` with a real import flow:
  - [ ] Read the contract from the library via `ContractLibrary::get()` 
  - [ ] Clone it into the workshop as an editable contract
  - [ ] Set status to `"imported to workshop"`
  - [ ] Log the import in a local history
- [ ] Tests: import a contract from library → verify it appears in the workshop as an editable copy → verify the original is unchanged.

## Acceptance gates

```bash
cargo test -p reachlock-client
# Gathering system: resource node interaction, depletion, respawn
# Signature collector: sign/reject cycle, online submission path
# Deliberation renderer: state tracking, multi-track display
# Library import: no placeholder, real import flow
make check
```

Manual: fly to an asteroid field → press interact → watch gathering timer → collect resources → cargo hold fills → node depletes → node respawns after tick count. Open contract library → import a contract → verify it appears in workshop. Trigger an LLM call → watch deliberation indicator appear → see outcome result.

## Non-goals

Full inventory management UI (Phase 4). Resource market prices and trading UI (S44 covers this at the economy level). 3D gathering animations (the progress bar is 2D UI). Voice chat deliberation overlay (S62 covers audio).

## Gotchas

- Resource nodes are spawned by the system generator (S04) as part of the generated system. If a system is generated, resource nodes should be placed in asteroid fields, on planet surfaces (landed mode), and near gas giants. The bridge layer (`reachlock-client/src/bridge/`) must handle the `ResourceNode` → Bevy component conversion.
- The deliberation renderer must NOT block the game loop. If an LLM call hangs (timeout), the deliberation indicator shows "Crew is considering... (timeout warning)" after 10s. The game continues running — only the LLM call path waits.
- The signature collector's notification must be unobtrusive. Use the same notification pattern as the discovery panel (small text popup in the lower-left corner, auto-dismiss after 5 seconds).
- If a contract is imported from library and already exists in the workshop (same contract_id), the import creates a versioned copy: "cryo-pilot (2)". Do not silently overwrite.
