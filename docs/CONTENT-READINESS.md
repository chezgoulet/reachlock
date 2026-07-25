# ReachLock — Content Authoring Readiness

**Date:** 2026-07-25 · **Branch:** `testing` (v2) · **Gate:** `make check` green

What exists, what is missing, and what actually blocks you from sitting down
and building the game. This supersedes the point-in-time review documents for
"what should I do next" purposes — those are marked historical.

---

## 0. The short answer

**Nothing blocks you.** The three blockers this document opened with are
closed, and the tree now has a gate that stops them coming back.

The highest-value first job is the six content types that are fully plumbed
and completely empty: dialogues, dungeons, events, recipes, encounters, and
tropes. Each has an editor, a schema, and a live consumer. Anything you author
there lands in the game immediately.

---

## 1. What the whole-tree check found

`reachlock content check` walks the content tree and reports references to ids
that nothing defines — the thing per-file validation structurally cannot see,
because a file naming a nonexistent ship is a perfectly well-formed file.

The first run found **30 missing ids behind 40 dangling references**. Nine of
the ten origins were unplayable as authored. All are now closed:

| What was missing | Count | Now |
|---|---|---|
| Ship templates the origins name | 8 | Authored under `hulls/` |
| Career paths (only `compact_navy` existed) | 9 | Authored under `careers/` |
| Souls — 5 canonical crew, 5 origin companions | 10 | Authored under `souls/` |
| Faction ids (`free_traders`/`megacorp`/`reach_pirates`) | 3 | Corrected to canon `isc`/`corp`/`reach` |
| A theme that parsed as no payload | 1 | Wrapped in a `ContentFile` envelope |

Two schemas were also wrong and had been rejecting valid content: the soul
schema had no `look` property (added in S76), and the career schema required
`faction_id` to be a string when the type is `Option<String>` — so every
independent career was unvalidatable.

Authoring the crew souls with `look:` blocks let `builtin_crew_config` go.
Four crew members' appearances lived in client code only because their souls
did not exist. That was the last exemption in the decoupling gate.

## 2. The gate

Three layers, each verified to fail on a tree with one bad reference:

```bash
reachlock content check            # whole tree, exit 1 on anything broken
reachlock content check --orphans  # also list content nothing references
make check-content                 # the same, wired into `make check`
cargo test -p reachlock-cli        # integration test, so the ordinary loop catches it
```

References are **typed**: an origin's `ship_template` must name a *ship
template*, not merely some id that exists. Untyped membership would call the
tree healthy the moment any file used the same string, so authoring a soul
named `corvette_mk2` would "fix" a missing ship. There is a test for that.

Two things are deliberately **not** references:

- `soul.identity.faction_affiliation` is prose, not an id — real values
  include `"Sorrow Station (independent, ISC-adjacent)"`.
- `origin.starting_gear[].item_id` — items are generated from seeds and have
  no authored id space.

Also not a broken reference: a `ShipTemplate.hull_id` with no authored hull
mesh. That falls back to a generated hull by design.

## 3. What is ready

The engine no longer assumes any particular content. A player creates a
character, picks an origin, and the origin supplies the ship, crew, career,
faction standing, credits, and starting location. `make check-purity` fails the
build if engine code names a specific ship or crew member.

| Layer | State |
|---|---|
| Content pipeline | `ContentFile` envelope, 17 `AssetType` variants, all 17 dispatched to a consumer (gate-enforced) |
| Editors | 27 content types, every one reachable from the Content Browser and `File > New` (gate-enforced) |
| Schemas | 26 JSON schemas under `mods/reachlock/schemas/` |
| Cross-references | `reachlock-core::content::refs` (tree-wide) and `reachlock-editor/src/cross_ref.rs` (editor UX) |
| Validation | Per-file `content validate`; whole-tree `content check`; plus `validate-goods`, `validate-factions`, `validate-storylines` |
| Determinism | 354 golden entries, bit-identical across x86_64 / aarch64 / i686 |
| Server | Postgres + Redis verified live; register → verify → login works end to end |

## 4. Content inventory

| Type | Files | Editor | Schema | Consumed | Note |
|---|---|---|---|---|---|
| origins | 10 | ✔ | ✔ | ✔ | all ten fully resolve |
| souls | 13 | ✔ | ✔ | ✔ | 7 canonical crew + Doss + 5 companions |
| hulls | 15 | ✔ | ✔ | ✔ | 10 ship templates, plus frames/rooms |
| careers | 10 | ✔ | ✔ | ✔ | conflicts are symmetric, validated at load |
| systems | 8 | ✔ | ✔ | ✔ | |
| combat | 4 | ✔ | ✔ | ✔ | |
| storylines | 2 | ✔ | ✔ | ✔ | |
| crews, cultures, economy, ecosystems, factions, gate_network, locations, stations, themes | 1 each | ✔ | ✔ | ✔ | one worked example apiece |
| dialogues, dungeons, events, recipes, encounters, tropes | 0 | ✔ | ✔ | ✔ | **plumbed and empty — start here** |

## 5. Remaining gaps, in rough priority

None of these block authoring. They make it slower or more annoying.

| # | Gap | Why it matters |
|---|---|---|
| 1 | No `Preview → Launch in game` from the editor | You author blind. Every check is "save, quit, run the client, navigate there." Closes the authoring loop more than any other single feature |
| 2 | Content root is CWD-relative | Running the editor from anywhere but the repo root silently reads and writes the wrong tree. Already caused stray files under `reachlock-editor/mods/` |
| 3 | Six content types have zero examples | One good worked example per type is worth more than a tutorial |
| 4 | `content/` vs `mods/reachlock/` | Two content trees exist. The server reads `content/`, the client reads `mods/reachlock/`. Souls live in both with different files — `content/souls/tove.ron` contradicts LORE.md. Pick one; `content check` only walks the tree you point it at |
| 5 | `content/factions/scaffold_faction.json` is not valid JSON | Warns at server startup and falls back to core defaults |
| 6 | No content hot-reload | Every change costs a client restart |
| 7 | Editor has no in-game preview of souls/dialogue | The graph editors show structure, not how a conversation reads in play |

## 6. Known engine debt that is *not* blocking

Recorded so it is not rediscovered as a surprise:

- **2FA does not survive a server restart.** Auth token/TOTP state is
  `Mutex<HashMap>` on `AppState` with no Postgres path. Single-player and
  local dev are unaffected.
- **Self-jump has no story gate** and **saves are a single file** — both
  inherited from deleted v1 branches, recorded in
  `docs/PENDING-FEATURE-PRS.md`.
- **The client is text-rendered.** The widget kit exists (`widget_kit/`,
  `focus_stack.rs`) but menus, settings, market, HUD, and character creation
  still draw as formatted text. Porting them is the largest single visual win
  available and is independent of content work.

## 7. Suggested order

1. **Author into the six empty types.** Dialogues first — they are the most
   directly playable and the dialogue editor validates node graphs.
2. **Pick one content tree** (§5.4) before the two diverge further.
3. **Fold in `Launch in game`** (§5.1) when the round-trip starts to hurt.
