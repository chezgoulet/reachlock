# ReachLock — Content Authoring Readiness

**Date:** 2026-07-25 · **Branch:** `main` (v2) · **Gate:** `make check` green

What exists, what is missing, and what actually blocks you from sitting down
and building the game. This supersedes the point-in-time review documents for
"what should I do next" purposes — those are marked historical.

---

## 0. The short answer

**You can start authoring today**, and the highest-value first job is content
that already has an editor, a schema, and a live consumer: souls, systems,
stations, factions, storylines, and dungeons.

Three things block a *complete* first playthrough, in priority order:

1. **Eight ship templates are referenced by origins but not authored.** Nine of
   the ten origins hand the player a ship that does not exist. Until these are
   written, every origin except `loup_garou_veteran` starts on the neutral
   starter hull.
2. **Four of the seven canonical crew have no soul file.** The crew package
   references `keene`, `bardo`, `prudence`, `risc` — none exist. Their
   appearance is still a hardcoded fallback in the client (the one deliberate
   exemption in `make check-purity`).
3. **Nothing validates the content tree as a whole.** The CLI validates one
   file at a time. There is no command that walks every file and reports
   dangling ids — which is how the eight missing ships went unnoticed.

None of these block *starting*. All three block *finishing*.

---

## 1. What is actually ready

The engine no longer assumes any particular content. A player creates a
character, picks an origin, and the origin supplies the ship, crew, career,
faction standing, credits, and starting location. `make check-purity` fails the
build if engine code names a specific ship or crew member.

| Layer | State |
|---|---|
| Content pipeline | `ContentFile` envelope, 17 `AssetType` variants, all 17 dispatched to a consumer (gate-enforced) |
| Editors | 27 content types, every one reachable from the Content Browser and `File > New` (gate-enforced) |
| Schemas | 26 JSON schemas under `mods/reachlock/schemas/` |
| Cross-references | `reachlock-editor/src/cross_ref.rs` — id autocomplete, go-to-definition, find-usages, `is_known()` |
| Validation | Per-file: `reachlock content validate`. Plus `validate-goods`, `validate-factions`, `validate-storylines` |
| Determinism | 354 golden entries, bit-identical across x86_64 / aarch64 / i686 |
| Server | Postgres + Redis verified live; register → verify → login works end to end |

## 2. Content inventory

| Type | Files | Editor | Schema | Consumed | Note |
|---|---|---|---|---|---|
| systems | 8 | ✔ | ✔ | ✔ | healthiest area |
| origins | 10 | ✔ | ✔ | ✔ | 8 name missing ships |
| hulls | 7 | ✔ | ✔ | ✔ | 2 are ship templates; rest are frames/rooms |
| combat | 4 | ✔ | ✔ | ✔ | |
| souls | 3 | ✔ | ✔ | ✔ | 4 canonical crew missing |
| storylines | 2 | ✔ | ✔ | ✔ | |
| careers, crews, cultures, economy, ecosystems, factions, gate_network, locations, stations, themes | 1 each | ✔ | ✔ | ✔ | one worked example apiece |
| dialogues, dungeons, events, recipes, encounters, tropes | 0 | ✔ | ✔ | ✔ | **editor and schema ready, no content yet** |

The last row is the opportunity: six content types are fully plumbed and
completely empty. Anything you author there lands in the game immediately.

## 3. What blocks a complete playthrough

### 3.1 Eight missing ship templates *(highest impact, pure content)*

`origins/*.ron` reference these; only `loup_garou` exists:

```
cargo_hauler       compact_frigate    corvette_mk2      diplomatic_shuttle
long_range_scout   salvaged_shuttle   science_vessel    stolen_fighter
```

An unknown template now logs a warning and leaves the neutral starter hull in
place — it no longer silently substitutes the Loup-Garou. So the failure is
visible and safe, but nine of ten origins currently start you on the same hull.

**To author one:** copy `hulls/loup_garou_interior.ron`, change `id`,
`hull_id`, `name`, and the deck layout. `ShipTemplate` is
`{ id, name, description, hull_id, interior, default_system_seed }`. A
single-deck hull is valid — the engine makes no assumption about deck count,
zero-g, or where the cockpit sits.

### 3.2 Four canonical crew have no soul file

`crews/loup_garou.ron` lists seven `soul_id`s. Authored: `tib`, `tove`,
`boris`. Missing: `keene`, `bardo`, `prudence`, `risc`.

All four are richly described in `docs/LORE.md` §"The Crew". Their appearance
currently lives in `interior.rs::builtin_crew_config` — the single documented
exemption in the decoupling gate. Author the four soul files with a `look:`
block (copy the shape from `content/souls/tove.ron`) and that exemption can be
deleted, along with the function.

### 3.3 No whole-tree validation *(highest leverage, small build)*

This is the one worth building before authoring in bulk. Everything needed
already exists — `cross_ref.rs` has the reference graph and `is_known()`; the
CLI has per-file validation. What is missing is a command that walks the tree
and reports:

- ids referenced but never defined (the eight ships; the four souls)
- duplicate ids across files
- files that parse as no known payload
- orphans: content nothing references

Without it, a typo in a `faction_id` or `next_node` is found at runtime, if
ever. With it, authoring in volume is safe. Suggested shape:

```
reachlock content check            # whole tree, exit 1 on dangling refs
reachlock content check --orphans  # also list unreferenced content
```

Then add it to `make check` so the content tree cannot rot the way the code
gates stopped the code rotting.

## 4. Smaller gaps, in rough priority

| # | Gap | Why it matters |
|---|---|---|
| 1 | No `Preview → Launch in game` from the editor | You author blind. Every check is "save, quit, run the client, navigate there." Closes the authoring loop more than any other single feature |
| 2 | Content root is CWD-relative | Running the editor from anywhere but the repo root silently reads and writes the wrong tree. Already caused stray files under `reachlock-editor/mods/` |
| 3 | Six content types have zero examples | An empty type with a schema is hard to start; one good worked example per type is worth more than a tutorial |
| 4 | `content/` vs `mods/reachlock/` | Two content trees exist. The server reads `content/`, the client reads `mods/reachlock/`. Souls live in both, with different files. Pick one |
| 5 | Two content files fail to parse | `content/factions/scaffold_faction.json` is not valid JSON; `content/storylines/compact_arc.ron` does not parse as a `Storyline`. Both warn at server startup and fall back to core defaults |
| 6 | No content hot-reload | Every change costs a client restart |
| 7 | Editor has no in-game preview of souls/dialogue | The graph editors show structure, not how a conversation reads in play |

## 5. Known engine debt that is *not* blocking

Recorded so it is not rediscovered as a surprise:

- **2FA does not survive a server restart.** Auth token/TOTP state is
  `Mutex<HashMap>` on `AppState` with no Postgres path. Single-player and
  local dev are unaffected.
- **`builtin_crew_config`** — see §3.2. The one decoupling exemption.
- **Self-jump has no story gate** and **saves are a single file** — both
  inherited from deleted v1 branches, recorded in
  `docs/PENDING-FEATURE-PRS.md`.
- **The client is text-rendered.** The widget kit exists (`widget_kit/`,
  `focus_stack.rs`) but menus, settings, market, HUD, and character creation
  still draw as formatted text. Porting them is the largest single visual win
  available and is independent of content work.

## 6. Suggested order

1. **Build `content check`** (§3.3). Half a day, and it protects everything after.
2. **Author the eight ship templates** (§3.1). Unblocks nine origins; each is a
   copy-and-edit of an existing file.
3. **Author the four crew souls** (§3.2). Lore already written; deletes an
   engine exemption.
4. **Then author freely.** Dialogues, dungeons, events, recipes, encounters,
   and tropes are plumbed and empty — that is where the game gets made.
5. Fold in `Launch in game` (§4.1) when the round-trip starts to hurt.
