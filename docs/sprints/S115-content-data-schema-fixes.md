# S115 — Content Data & Schema Fixes (M12, M13, M14, M15, M27, M28, M29, H7)

**Wave: Hotfix · Depends on:** None (content/schema only)

## Outcome

Eight content and schema correctness issues fixed:
- **M12**: Faction id mismatch between gate network and storyline
- **M13**: Crew role three-way mismatch in origin
- **M14**: Soul schema rejects `null` for `next` field
- **M15**: Bare arrays without `ContentFile` envelope
- **M27**: Chinese characters in English description
- **M28**: Dangling `cryo-pilot` contract references in 3 soul files
- **M29**: Dialogue schema completely stale
- **H7**: CLI uses wrong schema for dialogue validation

---

## Fix 1 — M12: Align faction id across content files

**Files:**
- `mods/reachlock/gate_network/core_region.ron` lines 16, 19
- `storylines/veil_arc.ron` lines 3, 19

The gate network uses `controlled_by: Some("earth_remnant")` but the storyline uses `faction: ("reach_remnant")`. Pick ONE consistent id.

**Fix:** Change `core_region.ron` lines 16 and 19 from `"earth_remnant"` to `"reach_remnant"`:

```ron
// line 16 — was "earth_remnant"
(from: "sorrow", to: "earth", status: blockaded, controlled_by: Some("reach_remnant")),
// line 19 — was "earth_remnant"  
(from: "earth", to: "fringe_b", status: blockaded, controlled_by: Some("reach_remnant")),
```

Also update the comment on line 15 from `"Earth Remnant"` to `"Reach Remnant"` for consistency.

---

## Fix 2 — M13: Align crew roles in origin

**File:** `mods/reachlock/origins/loup_garou_veteran.ron` lines 25-30

**Current roles assigned by origin:**

| Soul | Origin Role | Soul's Identity Role |
|------|-------------|---------------------|
| tove | pilot | pilot ✅ |
| keene | engineer | engineer ✅ |
| bardo | medic | doctor ❌ |
| prudence | navigator | (varies) |
| risc | gunner | (varies) |
| boris | tactical | (varies) |

**Fix:** Read each soul file to verify its `identity.role` field, then align the origin to match. Check each file:
- `mods/reachlock/souls/bardo.ron` — the `identity.role` field
- `mods/reachlock/souls/prudence.ron`
- `mods/reachlock/souls/risc.ron`
- `mods/reachlock/souls/boris.ron`

Then update `loup_garou_veteran.ron` to use the correct role per soul. At minimum, change `"medic"` to `"doctor"` if bardo's soul says `"doctor"`.

---

## Fix 3 — M14: Soul schema accepts null for `next`

**File:** `mods/reachlock/schemas/soul.schema.json` line 376-378

**Before:**
```json
"next": {
  "type": "string"
},
```

The Rust type is `Option<String>`. When `next` is `None` (end conversation), the JSON value is `null`. The schema must accept both.

**After:**
```json
"next": {
  "type": ["string", "null"]
},
```

Then verify the CLI validation passes a soul file with `next: null` in a dialogue choice:
```bash
cargo run -p reachlock-cli -- content validate mods/reachlock/souls/boris.ron
```

If boris has no dialogue choices with `null` next, create a test case.

---

## Fix 4 — M15: Wrap bare arrays in ContentFile envelopes

**File 1:** `mods/reachlock/storylines/compact_arc.ron`

**Current:** Starts with `[` — bare array of storylines.

**Fix:** Wrap in `ContentFile`:
```ron
ContentFile(
    id: "compact_arc",
    display_name: "Compact Arc",
    asset_type: storyline,
    seed: 0,
    universe: "all",
    priority: authoritative,
    payload: storylines([
        // ... existing storyline entries ...
    ]),
)
```

**File 2:** `mods/reachlock/storylines/loup_garou_souls.ron`

**Current:** Starts with `[` — bare array of soul mutations.

**Fix:** Wrap in `ContentFile`:
```ron
ContentFile(
    id: "loup_garou_souls",
    display_name: "Loup-Garou Soul Mutations",
    asset_type: soul_mutations,
    seed: 0,
    universe: "all",
    priority: authoritative,
    payload: soul_mutations([
        // ... existing mutation entries ...
    ]),
)
```

**Important:** After wrapping, update the code that reads these files:

In `soul.rs` line 142: the current code reads `Vec<SoulMutation>` directly. After wrapping, it must parse a `ContentFile` envelope first. Update to:

```rust
// Before:
if let Ok(mutations) = ron::from_str::<Vec<reachlock_core::soul::SoulMutation>>(&text) {

// After:
if let Ok(cf) = ron::from_str::<reachlock_core::content::ContentFile>(&text) {
    if let Some(mutations) = cf.payload.as_soul_mutations() {
        registry.mutations.extend(mutations);
    }
}
```

If `ContentPayload` doesn't have a `soul_mutations` variant, add one or use a different envelope type.

---

## Fix 5 — M27: Remove Chinese characters from earth.ron

**File:** `mods/reachlock/systems/earth.ron` line 7

**Before:**
```ron
description: "The cradle of humanity, lost. ... remnants of the old秩序的, guarding what remains.",
```

**After:**
```ron
description: "The cradle of humanity, lost. ... remnants of the old order, guarding what remains.",
```

Replace `旧秩序的` with `the old order`. (Literal translation: "the old order")

---

## Fix 6 — M28: Fix dangling cryo-pilot contract references

**Files affected:**
- `mods/reachlock/souls/boris.ron` line 100
- `mods/reachlock/souls/grissom.ron` line 77
- `mods/reachlock/souls/alexandre_dubois.ron` line 82

All three reference `"cryo-pilot"` in their `contracts` list, but no `contracts/cryo-pilot.ron` exists.

**Fix:** Remove the `"cryo-pilot"` entry from each soul's `contracts` list. The remaining contracts in each list are sufficient.

OR create a `mods/reachlock/contracts/cryo-pilot.ron` file if the cryo-pilot contract is actually intended to exist. Simplest: remove the dangling references.

For `boris.ron` line 100 — change:
```ron
contracts: ["cryo-pilot"],
```
to:
```ron
contracts: [],
```

For `alexandre_dubois.ron` line 82 — remove `"cryo-pilot"` from the list:
```ron
contracts: ["combat-tactical", "emergency-procedures"],
```

For `grissom.ron` line 77 — remove `"cryo-pilot"`:
```ron
contracts: ["combat-tactical", "emergency-procedures"],
```

Verify with:
```bash
cargo run -p reachlock-cli -- content check mods/reachlock
```

---

## Fix 7 — M29: Regenerate dialogue schema from actual Rust types

**File:** `mods/reachlock/schemas/dialogue.schema.json`

The current schema uses `speaker` and `text` (old field names). The actual Rust types (`content/dialogue.rs`) define:

```rust
pub struct Dialogue {
    pub nodes: Vec<DialogueNode>,
    pub start_node: String,
}
pub struct DialogueNode {
    pub id: String,
    pub node_type: NodeType,          // enum: NarratorLine, NpcLine, PlayerChoice, Branch, End
    pub text: String,
    pub choices: Vec<DialogueChoice>,
    pub voice_clip: Option<String>,
}
pub struct DialogueChoice {
    pub display_text: String,
    pub condition: Option<String>,
    pub consequence: Option<String>,
    pub next_node: String,
}
```

**Replace the entire schema file** with:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Dialogue Tree",
  "description": "A branching dialogue tree",
  "type": "object",
  "required": ["nodes", "start_node"],
  "properties": {
    "nodes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "node_type", "text"],
        "properties": {
          "id": { "type": "string" },
          "node_type": {
            "type": "string",
            "enum": ["NarratorLine", "NpcLine", "PlayerChoice", "Branch", "End"]
          },
          "text": { "type": "string" },
          "voice_clip": { "type": ["string", "null"] },
          "choices": {
            "type": "array",
            "items": {
              "type": "object",
              "required": ["display_text", "next_node"],
              "properties": {
                "display_text": { "type": "string" },
                "condition": { "type": ["string", "null"] },
                "consequence": { "type": ["string", "null"] },
                "next_node": { "type": "string" }
              }
            }
          }
        }
      }
    },
    "start_node": { "type": "string" }
  }
}
```

---

## Fix 8 — H7: Fix CLI dialogue schema reference

**File:** `reachlock-cli/src/content.rs` line 39-40

**Before:**
```rust
// Dialogue schema is pending from S53 — use ecosystem as placeholder.
const DIALOGUE_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/ecosystem.schema.json");
```

**After:**
```rust
const DIALOGUE_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/dialogue.schema.json");
```

Remove the misleading comment. The correct schema now exists (fixed in M29 above). If M29 is done first, this is a 1-line change. If done independently, the schema file must be updated first.

---

## Acceptance gates

```bash
# Verify schema passes valid dialogue files
cargo run -p reachlock-cli -- content validate mods/reachlock/dialogues/

# Content check — no dangling references
cargo run -p reachlock-cli -- content check mods/reachlock

# Earth.ron — no non-ASCII in description
file mods/reachlock/systems/earth.ron  # should be ASCII/UTF-8 with only ASCII in description

# All tests still pass
cargo test --workspace

make check
```

## Non-goals

- Full content audit for every reference (only the 8 listed issues)
- Creating missing content (cryo-pilot contract) unless intended
- Schema versioning / migration system

## Gotchas

- **M15 wrap: The `ContentFile` wrapper changes the file format.** Soul mutation loading code in `soul.rs` must be updated simultaneously. If the RON file changes but the reader doesn't, soul mutations silently stop loading. Test by running the game: verify Boris/Tove/Keene have their mutation arcs applied.
- **M15 wrap: `asset_type` variant.** If `ContentPayload` doesn't have `soul_mutations(...)` or `storylines(...)`, you may need to add it to the enum. Check `reachlock-core/src/content/envelope.rs` for the `ContentPayload` enum.
- **M13 crew roles: Read soul files FIRST.** Don't guess the correct role. Open each `.ron` file under `mods/reachlock/souls/` and check `identity: (role: "...")`. Align the origin to match exactly.
- **M28: The `content check` command may not exist.** Check `reachlock-cli --help` for the correct subcommand. It may be `reachlock content validate` or `reachlock content check`. Use the command that checks cross-references.
- **Order of operations:** M29 (fix schema) before H7 (use schema). M15 (wrap files) before M28 (remove dangling refs — the wrapper changes may expose new validation issues).
