# The Weave Contract — v0

How inference grows **branches from what's written** — dialogue the author
never typed, carrying world effects the author explicitly permitted — without
ever surrendering determinism, saves, or multiplayer to a model's mood.

Authored trees stay the backbone. A dialogue graph may now contain `woven`
nodes: like `generated` nodes they hand a `prompt_hint` to the NPC's mind,
but where a generated node gets back a *line*, a woven node gets back a
**proposal** — `{line, choices[], mutations[]}` as structured data — and the
engine treats that proposal the way a customs officer treats a manifest:
everything is checked against what the author said this node **may** do,
clamped or dropped, and only then does it become real.

Status: **v0** (2026-07-10). The contract is three schemas and one algorithm:

- the `woven` node kind in [schemas/dialogue.schema.json](schemas/dialogue.schema.json)
  (`prompt_hint`, `grounding`, `return_to`, and the `may` allowlist),
- the proposal shape in [schemas/weave_proposal.schema.json](schemas/weave_proposal.schema.json),
- the resolution algorithm below, whose reference implementation is
  `scripts/check_weave_contract.py` over the golden + adversarial fixtures in
  `weave/fixtures/` (runs in `make check`); the engine implementation
  (`godot/scripts/framework/weave_loom.gd`) must produce byte-identical
  resolutions for every fixture (`godot/tests/test_weave_loom.gd`).

Changes bump nothing silently: the dialogue schema is framework-versioned,
and the resolution algorithm changes only with a migration note here.

## The three properties (in priority order)

1. **The allowlist is law.** A woven branch can move Reach standing 3 points
   because the author granted `adjust_faction reach_compact trust ±3` on
   that node — never 40 because a model felt like it. A proposal that
   exceeds its grants is **clamped** (amounts) or **neutered** (ungranted
   ops, targets, flags: dropped), never rejected wholesale — the scene
   plays on with whatever survived. Adversarial fixtures prove this.
2. **Resolved once, then data.** The proposal is resolved at one authority
   (single-player: the local game; multiplayer: the host, per
   [protocol/SHIP-SHARE.md](protocol/SHIP-SHARE.md)) and the *resolution*
   is persisted in the save (`weaves` block, save schema). Replays,
   reloads, and every client consume the persisted resolution as ordinary
   dialogue data. The world only ever consumes data; generation happens
   once.
3. **Offline is authored.** No mind → a woven node plays its `text`
   fallback and its authored `goto`, exactly like a generated node. A save
   that already carries a resolution plays it verbatim, mind or no mind.

## The `woven` node

```json
"the_rumor": {
  "kind": "woven",
  "prompt_hint": "Grissom trades a rumor about the cordon for what the player did in the bar. Offer 2-3 ways to press him.",
  "text": "Grissom looks at his glass. \"Ask me another day.\"",
  "buffer_line": "He turns the glass one full turn before he answers.",
  "grounding": [
    {"vault": "lore", "query": "duskway cordon patrol schedule rumors"}
  ],
  "return_to": "after_rumor",
  "may": {
    "grants": [
      {"op": "adjust_faction", "factions": ["reach_compact"], "axes": ["trust"], "max_amount": 3},
      {"op": "set_player_flag", "flags": ["heard_cordon_rumor"]},
      {"op": "add_memory", "max_importance": 0.6}
    ],
    "max_choices": 3,
    "max_mutations": 3
  }
}
```

- `prompt_hint` — the perceive objective, exactly as `generated` nodes.
- `text` — the offline/authored fallback line (required: woven nodes must
  degrade to authored).
- `buffer_line` — the latency mask, unchanged and untouchable.
- `grounding` — recall queries the host runs through the memory interface
  (MemoryStore → ragamuffin, **read-only**) and folds into the `memory`
  context channel alongside the soul's own recall. Ragamuffin is untouched
  by this contract; a compendium/lore vault is just another vault.
- `return_to` — where every woven choice lands (a node id or `"end"`).
  The author keeps the spine: woven branches rejoin the tree where the
  tree says, so reachability analysis (the M12 journey tests) still holds.
- `may` — the allowlist. No `may`, or an empty `grants` array, means the
  node may propose *words only*: line and choice text, zero mutations.

## Grants

One grant per mutation shape the node may produce, mirroring the dialogue
mutation vocabulary (never extending it):

| grant | permits |
|---|---|
| `{op: "adjust_relationship", targets: [...], axes: [...], max_amount: N}` | that op toward listed targets, listed axes, amount in [−N, N] |
| `{op: "adjust_faction", factions: [...], axes: [...], max_amount: N}` | player standing moves on listed factions/axes, amount in [−N, N] |
| `{op: "set_flag" \| "clear_flag", flags: [...]}` | soul flags from the listed set only |
| `{op: "set_player_flag" \| "clear_player_flag", flags: [...]}` | player flags from the listed set only |
| `{op: "add_memory", max_importance: X}` | a memory write, importance clamped to [0, X] |

`max_amount` caps **magnitude**: a grant of 3 permits −3…+3. Flags cannot
be clamped, only granted or dropped — list them exhaustively.

## The proposal

What the mind returns (weave_proposal.schema.json):

```json
{
  "line": "\"Third shift, the picket thins. You didn't hear it from a man you bought a drink.\"",
  "mutations": [{"op": "adjust_faction", "faction": "reach_compact", "axis": "trust", "amount": 2}],
  "choices": [
    {"text": "Why tell me?", "mutations": []},
    {"text": "I owe you one.", "mutations": [{"op": "set_player_flag", "flag": "heard_cordon_rumor"}]}
  ]
}
```

## Resolution (normative)

Given a woven node and a proposal, the authority resolves deterministically:

1. A proposal that fails the proposal schema (or whose `line` is empty) is
   discarded whole and the node plays its authored fallback.
2. Choices beyond `may.max_choices` (default **3**) are dropped from the
   tail. Every surviving choice gets `goto: return_to` (default `"end"`).
3. Every remaining mutation (node-level first, then per surviving choice,
   in array order) is checked against the grants: **no grant with that op
   → dropped**; a listed-set grant whose set does not contain the
   mutation's target/faction/flag (or whose axes miss the mutation's axis)
   → **dropped**; amounts → **clamped** to the grant's magnitude cap;
   `add_memory` importance → clamped, tags passed through.
4. Mutations beyond `may.max_mutations` (default **4**, counted across the
   node and all surviving choices in the same order) are dropped.
5. The resolution `{line, mutations, choices}` is written to the save's
   `weaves` block under `"<dialogue_id>/<node_id>"` with the tick it
   resolved at — **before** any of its mutations are applied. From that
   moment the branch is ordinary dialogue data.

Silent success is the failure mode to fear: the reference implementation
and the engine both **log every drop and clamp** (op, reason) so an
over-reaching provider is visible in test transcripts, but the player
never sees the seam.

## v0 implementation note (the pan gap)

Pan's v0 LLM provider emits Express + Conclude only — no structured tool
invokes (M7). So v0 transports the proposal **as the Express body**: the
prompt built from `prompt_hint` instructs the mind to answer with a single
JSON object in the proposal shape; the engine parses the Express body
(first `{`…last `}`), validates against the proposal schema, and resolves.
Unparseable → authored fallback, exactly like offline. When pan grows
invoke-carrying proposals (a pan-budgeted sprint), the proposal arrives as
a capability invoke instead and this section is deleted — the contract
above does not change.
