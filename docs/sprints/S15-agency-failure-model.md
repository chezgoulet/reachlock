# S15 — LLM Agency & Failure Model

**Spec:** §18 (all) · **Wave 4 · Depends on:** S13, S14

## Outcome

"Who should decide?" is mechanical: the dispatch routes orders to robots
(execute-only) and droids (may question), every LLM call resolves through
the spec's outcome table (success / timeout / misinterpretation /
confabulation / collapse / catastrophic) with seeded, modifier-shifted
probabilities, and every outcome is traceable — the player can always
reconstruct "Boris timed out during jump 47" from the log. Failure is
gameplay, provably never silent.

## Context

- S14 gives the real error taxonomy (its Timeout/BadResponse map onto the
  spec's timeout/collapse rows). This sprint adds the GAME layer: outcome
  classification, probability modifiers, and the dispatch abstraction.
- The contract engine and deliberation flow are in core + client; the ship
  log is the traceability surface (S08 gave it a console).
- Spec §18's baseline table: success 70%, timeout 10%, misinterpretation
  10%, confabulation 5%, collapse 3%, catastrophic 2% — shifted by model
  quality, contract quality, equipment (S05 stat keys), crew soul modifiers
  (S13 trust).

## Freeze first

Core `agency/` module: `LlmOutcome` enum (the six rows), `OutcomeWeights`
(fixed-point per-row weights summing to 1024), and
`fn resolve_outcome(seed_roll, weights) -> LlmOutcome` — pure and
deterministic from the roll. `fn weights(base, modifiers: &[Modifier]) ->
OutcomeWeights` where `Modifier` carries source (Equipment/Crew/Model/
ContractQuality) and per-row deltas. Property test: weights always
normalize; no row goes negative.

## Deliverables

- [ ] Outcome resolution wired into the deliberation path: a real provider
      SUCCESS can still be classified misinterpretation/confabulation by the
      roll (the model answered; the fiction decides how well). Timeout and
      BadResponse from S14 map to their rows directly. Catastrophic wraps
      another row's action with an escalated consequence event.
- [ ] Misinterpretation/confabulation effects: the resolved action is
      perturbed deterministically (swap to a plausible-but-wrong verb from
      the contract's action set / inject an invented context field into the
      reasoning) — the ship log shows what the crew BELIEVED.
- [ ] Modifier sources: contract quality (rule count + coverage heuristic —
      more uncovered evaluations recently = worse), crew trust (S13
      `trust.player`), equipment hooks (read S05 stat keys
      `deliberation_speed`, `failure_resistance` off `ShipSystems` — zero
      until items are equippable, but the pipe exists), model tier
      (Classic n/a, FairPlay base, Spectrum better, BYOK declared).
- [ ] The dispatch: a core struct owning the ship's contract set with
      routing rules — robots get `Execute(order)` (fallible mechanically,
      never deliberating), droids get `Consider(order)` (may return a
      counter-proposal, which is itself a deliberation with this outcome
      model). Port the spec §18 dispatch/droid exchange as a test.
- [ ] Traceability guarantee, tested: every deliberation produces log
      entries for (1) deliberation start with context summary, (2) outcome
      with reasoning, (3) fallback if fired — a test drives 100 seeded
      deliberations and asserts the log reconstructs every one.
- [ ] Player-facing: the ship log console (S08) renders outcome categories
      distinctly (timeout vs misinterpretation read differently); the
      contract editor surface can wait, but the log must tell the §18
      black-hole story.

## Acceptance gates

```
cargo test -p reachlock-core agency::   # distribution, normalization, determinism,
                                        # dispatch exchange, traceability battery
make check
```
Manual: run 10 anomalies with the stub → at least one non-success outcome
whose log entry reads as a story, not an error.

## Non-goals

Contract-authoring UI. Equipment EQUIPPING (the modifier pipe suffices).
Interrogation/combat deliberations (S19/S20 consume this). Economy
negotiator droids.

## Gotchas

- Rolls derive from `(contract_id, tick, chain position)` — deterministic
  and replayable, never `rand::random()`.
- Catastrophic at 2% baseline WILL fire in playtests — its consequences
  must be recoverable (damage, strand, loss of cargo), never save-corrupting.
- Do not let outcome classification touch Classic tier: no LLM = no outcome
  table; rules-only ships fail only by having no rule (already handled).
