# S100 — Editor Data Gaps, Schema Drift, and Non-Humanoid Content

**Status:** ready to execute
**Branch:** `sprint-v2/s100-editor-data-gaps` cut from `testing`
**Prepared:** 2026-07-26, from a verified audit of an external agent's report.

---

## Read this first

An external agent filed a report on editor data gaps. **Every claim in it was
verified against the code before this brief was written.** Most held; three did
not. This brief carries only the verified findings, with the corrections folded
in. **Do not re-derive the audit** — do not go re-read the original report, and
do not "fix" the three corrected items. They are listed in
[Appendix A](#appendix-a--claims-that-were-wrong) so you can recognise them if
someone hands you the original report again.

The audit ran against commit `afabc322` on `main`. `make check` was green and
`content check mods/reachlock` reported **clean** (73 ids, 69 references, 0
unparseable) at that commit. If either is failing when you start, fix that
first and note it — it is not part of this sprint.

---

## Outcome

The editor stops losing data silently, validates against the right schemas, and
the content tree gains its first non-humanoid reference material.

Ten tasks, T1–T10. **They are independent** except where noted. Each ends with
`make check` green. Do not batch `make check` to the end — a task is not done
until it passes.

---

## Ground rules (repo iron rules that bite in this sprint)

1. `reachlock-core` gets zero rendering/IO deps. T3 adds a Make target, not a
   core dep.
2. No floats in gameplay values. Nothing here should introduce one; if you
   reach for `f32` in core or in a content type, stop and re-read the task.
3. Wire shapes are pinned. **T2 and T5 touch schemas, which are pinned
   artifacts.** Schema changes need a note in the commit message.
4. UI colors come from the stylesheet. T1's warning panel must use
   `theme::text(...)` / named style classes — a literal `TextColor`,
   `BackgroundColor`, or `BorderColor` fails `make check-theme`.
5. `cargo fmt --all` churns ~33 untouched files due to rustfmt version skew.
   **Format only the crate you touched:** `cargo fmt -p reachlock-editor`.
6. Check `git status` and `.git/MERGE_HEAD` before every commit. Concurrent
   sessions have moved branches under this repo before. **Never `git add -A`** —
   stage the specific files you edited.
7. `~/.cargo/bin` is not on PATH in a fresh shell:
   `export PATH="$HOME/.cargo/bin:$PATH"`.

---

## T1 — Surface constructor-scan parse failures

**The bug.** Ten multi-entry editors scan their content directory in
`ClassName::new()` and load every `.ron` file they find. Each uses
`if let Ok(...)`. A file that fails to parse is dropped with no error anywhere —
no status text, no panel, no log. If every file in `souls/` failed, the Soul
editor would open showing one blank placeholder entry and report itself clean.

**What is NOT broken, so don't "fix" it:** explicit File → Open already reports
parse errors. `main.rs:287` sets `status_text = format!("Open failed: {e}")`.
The gap is only the eager constructor scan.

**Affected call sites** (all in `reachlock-editor/src/editors/`):

| File | Line | Parses as | Extra filter |
|---|---|---|---|
| `soul.rs` | 201 | `read_enveloped::<SoulFile>` | — |
| `station.rs` | 68 | `read_ron::<ContentFile>` | payload must be `Station{..}` |
| `enemy.rs` | 72 | `read_ron::<HostileArchetype>` | — |
| `item.rs` | 165 | `read_ron::<ItemSeed>` | — |
| `contract.rs` | 101 | `read_ron::<Contract>` | — |
| `location.rs` | 65 | `read_ron::<HostileLocation>` | — |
| `charted_system.rs` | 91 | `read_ron::<ChartedSystem>` | — |
| `hull_mesh.rs` | 73 | `read_ron::<ContentFile>` | payload must be `Hull(_)` |
| `hull_frame.rs` | 97 | `read_ron::<ContentFile>` | payload must be `HullFrame(_)`; filename must end `_frame.ron` |
| `gate_network.rs` | 95 | `read_ron::<ChartedSystem>` | see note below |

### ⚠️ FOOT-GUN 1 — two different reasons a file is skipped

`station.rs`, `hull_mesh.rs`, and `hull_frame.rs` skip files for **two**
reasons, and they are not the same thing:

- **Parse failed** → this is the bug. Warn.
- **Parsed fine, wrong payload variant** → this is correct. `hulls/` legitimately
  holds `HullFrame`, `HullMesh`, and `RoomTemplates` side by side, so the
  HullMesh editor skipping a frame file is working as designed. **Do not warn
  on this.** A warning here would fire on every open of a healthy tree and
  train the author to ignore the panel.

Keep the payload-variant `matches!` check *outside* the warning path.

### ⚠️ FOOT-GUN 2 — `gate_network.rs` is a different shape

`gate_network.rs:91-99` is **not** loading editor entries. It builds a
`biomes: HashMap<String, Biome>` lookup from `systems/`. Two differences:

- It iterates `dir.flatten()` directly with **no `.ron` extension filter**, so
  it currently attempts to parse every file in the directory including
  non-RON ones. Add the extension filter as part of this task, or your new
  warning will fire on `README.md`-style files.
- A failure here degrades gate-network biome coloring, it does not lose an
  entry. Still warn, but the message should say so.

### Implementation

**Step 1** — add a shared helper to `reachlock-editor/src/io.rs`. All ten sites
duplicate the same read_dir → filter → sort → parse loop; collapsing them fixes
the bug in one place and removes the duplication.

```rust
/// Scan a content directory, parsing each `.ron` file as `T`.
///
/// Returns the successfully parsed files alongside a warning per file that
/// failed to parse. Multi-entry editors previously dropped unparseable files
/// with `if let Ok(..)`, so a malformed file vanished from the editor with no
/// error anywhere — the tab simply opened short an entry, or empty.
///
/// Callers that also reject files on a *payload variant* check must do that
/// filtering on the returned values, NOT here: a `hulls/` file that parses as
/// a `HullFrame` is correctly skipped by the HullMesh tab and must not warn.
pub fn scan_content_dir<T: serde::de::DeserializeOwned>(
    dir: &Path,
) -> (Vec<(PathBuf, T)>, Vec<String>) {
    let mut loaded = Vec::new();
    let mut warnings = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        // A missing directory is normal: a mod need not define every type.
        return (loaded, warnings);
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();
    for path in paths {
        match read_ron::<T>(&path) {
            Ok(value) => loaded.push((path, value)),
            Err(e) => warnings.push(e),
        }
    }
    (loaded, warnings)
}
```

Add a sibling `scan_enveloped_dir<T: Enveloped>` with the same shape that calls
`read_enveloped` and yields `(PathBuf, EnvelopeMeta, T)`. `soul.rs` is the only
caller today; write it anyway so the next enveloped editor has it.

`read_ron` (`io.rs:95`) already formats errors as
`"failed to parse {path}: {e}"`, so the warning strings are usable as-is.

**Step 2** — add a warnings channel to the `Editor` trait
(`reachlock-editor/src/app.rs:290`). Provided method, so the ~20 editors that
don't scan a directory need no change:

```rust
    /// Files this editor tried to load at construction and could not parse.
    ///
    /// Empty for editors that do not scan a directory. Drained into the
    /// status bar and the warning panel when the tab opens, so an
    /// unparseable file is visible instead of silently absent.
    fn load_warnings(&self) -> &[String] {
        &[]
    }
```

Each of the ten editors gains a `load_warnings: Vec<String>` field and returns
it.

**Step 3** — surface them in `reachlock-editor/src/main.rs`.

- `EditorApp` has `status_text: String` at `main.rs:63`, initialised
  `"Ready"` at `main.rs:208`. Both `open_new_editor` (`main.rs:247`) and
  `open_editor_for_file` (`main.rs:261`) create editors via
  `self.registry.create(ct)`.
- After creating an editor in **both** functions, if `editor.load_warnings()`
  is non-empty, set `status_text` to
  `format!("{} file(s) failed to parse — see Warnings", n)` and push the
  warnings onto a new `EditorApp` field `load_warnings: Vec<String>`.
- Render them in a dismissable `egui::Window` titled "Content Warnings", styled
  via named theme classes (see ground rule 4).

**Step 4** — test. Add to `io.rs`:

```rust
#[test]
fn scan_content_dir_reports_unparseable_and_keeps_the_rest() { /* … */ }
```

Write a temp dir with one valid RON file, one malformed one, and one non-`.ron`
file. Assert one loaded, exactly one warning, and that the warning names the
malformed file's path.

**Acceptance:** corrupt `mods/reachlock/souls/tib.ron` (append a stray `(` ),
open the Soul editor, see a warning naming `tib.ron`. **Restore the file
afterwards** — `git checkout mods/reachlock/souls/tib.ron`.

---

## T1b (optional, do it) — startup content-tree scan

The editor already depends on `reachlock-core`
(`reachlock-editor/Cargo.toml:9`), and core already has exactly the scanner
this needs. `reachlock_core::content::refs::ContentTree::scan(root)` returns a
`ContentTree` with an `unparseable: Vec<Unparseable>` field
(`reachlock-core/src/content/refs.rs:150-163`); `.check()` returns a
`CheckReport` with `unparseable`, `dangling`, `duplicates`, and `orphans`.
`Unparseable` is `{ file: PathBuf, reason: String }`.

At `EditorApp` startup, run `ContentTree::scan(crate::app::content_root())`,
call `.check()`, and feed `report.unparseable` into the same warning panel T1
builds. This catches malformed files in directories no tab has open.

**Do not** surface `report.orphans` as a warning — unreferenced content is
normal and explicitly documented as informational at `refs.rs:176-181`. Dangling
references and duplicates are already covered by the existing reference
validator; don't duplicate them here.

---

## T2 — Point the Dialogue editor at the dialogue schema

**One line.** `reachlock-editor/src/schema.rs:64` reads:

```rust
        ContentType::Dialogue => "ecosystem", // placeholder — S53 has dialogue schema pending.
```

`mods/reachlock/schemas/dialogue.schema.json` **already exists** — it was
authored and never wired. Change the mapping to `"dialogue"` and delete the
placeholder comment.

**Why this matters more than it looks:** `schema_id` feeds `SchemaCache`, and
`ai.rs:357` puts the schema text into the LLM system prompt
(`build_system_prompt`, `ai.rs:255`). AI-assisted dialogue generation is
currently being prompted with the *ecosystem* schema and then validated against
it at `ai.rs:407`. This is a live correctness bug, not just a lint.

**Verify** the schema actually matches the type before you call it done: build
a `Dialogue` default, serialise to JSON, and validate against the schema. If it
fails, the schema is stale — fix the schema in the same commit and say so in the
message (ground rule 3). T3 will make this check permanent.

---

## T3 — A schema↔type drift gate

**The recurring pattern.** `soul.schema.json` shipped without the `look`
property during S76 and nothing caught it. (It has `look` now —
`soul.schema.json:454` — so **do not go add it**; the gap is the missing gate,
not the missing property.)

Add `make check-schema`, wired into the `check` target
(`Makefile:11`) alongside the existing `check-purity check-features
check-content check-theme check-resources check-dead-code`.

**What it does:** for every `ContentType` where `schema::schema_id()` returns
`Some`, serialise that type's `Default` (or a canonical fixture) to
`serde_json::Value` and validate it against
`mods/reachlock/schemas/{id}.schema.json`. Any validation error fails the build,
naming the type, the schema, and the offending JSON path.

Implement as a `#[test]` in `reachlock-editor` (it already depends on
`jsonschema` — see `CompiledSchema` at `schema.rs:78`) plus a Makefile target
that runs that test alone. **Do not put it in `reachlock-core`** — core must
not gain a JSON-schema dep (ground rule 1).

Cover every `Some`-returning arm of `schema_id`. Two arms return `None`
deliberately and must be excluded:

- `ContentType::ItemBrowser | ContentType::SpriteViewer` — previewers persist
  nothing (`schema.rs:73`).
- `ContentType::CrewPackage` — until T5 lands. **Make the test fail loudly
  once `crew_package.schema.json` exists**, so T5 can't land a schema that
  nothing validates.

Note that `ContentType::HullMesh` maps to `hull_configuration`, not `hull`, on
purpose (`schema.rs:52-55`) — the editor edits a `HullConfiguration`, not the
raw `GeneratedMesh` that `hull.schema.json` describes. Don't "fix" that.

---

## T4 — Replace the hull substring hack

`reachlock-editor/src/browser.rs:81-93`:

```rust
fn classify_hull_file(path: &Path) -> ContentType {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ContentType::HullMesh;
    };
    if text.contains("RoomTemplates(") {
        ContentType::RoomTemplates
    } else if text.contains("HullFrame(") {
        ContentType::HullFrame
    } else {
        ContentType::HullMesh
    }
}
```

Three content types share `mods/reachlock/hulls/` (15 files today), and this
sorts them by raw substring match. A ship description or an id containing the
literal text `HullFrame(` routes the file to the wrong editor.

**Fix:** parse to `ron::Value` and inspect the payload tag, or deserialise the
envelope and match on `ContentFile::payload`. Keep the `HullMesh` fallback for
unreadable/unparseable files — `detect_content_type` (`browser.rs:72`) returns
`Option` and the browser expects a type back, so failing soft here is correct.

**Note** `hull_frame.rs:87-92` separately filters on the filename suffix
`_frame.ron`. That is a second, independent heuristic. Leave it alone in this
task — changing both at once makes a bisect painful. If you want it gone, that
is a follow-up.

**Test:** a `hulls/` fixture whose *description string* contains `HullFrame(`
but whose payload is a `Hull`. Assert it classifies as `HullMesh`.

---

## T5 — Author a CrewPackage schema

`schema.rs:70-72` returns `None` for `ContentType::CrewPackage` with the
comment "No crew_package schema is authored yet". Confirmed: there is no
`mods/reachlock/schemas/crew_package.schema.json`.

`mods/reachlock/crews/` holds exactly one file, `loup_garou.ron`. Derive the
schema from the Rust type (not from that one file — one example under-constrains
it), write `crew_package.schema.json` following the shape of the existing 26
schemas in that directory, and change `schema_id` to return
`Some("crew_package")`.

T3's gate then covers it automatically. Validate `loup_garou.ron` against the
new schema before committing.

---

## T6 — Comment-loss guard

`reachlock-editor/src/io.rs:102-110`. `write_ron` pretty-prints, and RON cannot
carry comments through a deserialize → serialize round trip. The docstring warns
about it; nothing in the UI does. Opening a hand-commented file and hitting
Ctrl+S silently strips every comment.

**Fix:** when an editor loads a file, record whether the source text contained
`//` or `/*`. If it did, the first save on that tab shows a confirm dialog:
"This file has authored comments. RON cannot preserve them through a save —
they will be removed. Save anyway?"

`reachlock-editor/src/dialogs.rs` (56 lines) already exists for this kind of
prompt; use it rather than inventing a second dialog mechanism.

⚠️ Detect comments on the **raw file text**, before parsing. Once RON is parsed
the comments are gone and there is nothing left to detect. A naive `//` search
will false-positive on a `//` inside a string literal (e.g. a URL) — that is
acceptable here: a spurious confirm is much cheaper than silent data loss.
Don't build a RON lexer for this.

---

## T7 — A SoulMutations entry point

`reachlock-editor/src/cross_ref.rs:177-178` maps
`AssetType::SoulMutations → ContentType::Soul`, with the comment "Mutation arcs
are soul data; there is no tab of their own for them." The Soul editor handles
mutations as a sub-tab, so a `soul_mutations.ron` file cannot be opened
directly — clicking it in the browser opens the Soul editor, which then loads it
as a soul and (per T1) will now warn that it failed to parse.

**Smallest correct fix:** make the Soul editor accept a `SoulMutations` payload
and open directly on its mutations sub-tab, rather than adding a whole new
`ContentType`. Adding a `ContentType` variant means touching `schema_id`,
`directory()`, `name()`, the registry (`app.rs:440`), the browser's
`FILE_TYPES`, and `cross_ref` — a lot of surface for one file.

**Sequencing:** do T7 **after** T1, or T1's warning will fire on
`soul_mutations.ron` and look like a T1 regression.

---

## T8 — Wire or retire the four dead stash takers

### ⚠️ FOOT-GUN 3 — the original report got this wrong

The report listed **six** dead takers. Two of them are live:

- `stash::take_tropes()` is consumed at
  `reachlock-client/src/systems/trope_dispatcher.rs:26`
- `stash::take_careers()` is consumed at
  `reachlock-client/src/systems/career.rs:45`

Both were wired in commit `b08f29ec`. Neither carries `#[expect(dead_code)]`.
**Leave them alone.**

The genuinely dead four, all in `reachlock-client/src/systems/dispatch.rs`, all
marked `#[expect(dead_code)]`:

| Function | Line |
|---|---|
| `take_dialogues` | 493 |
| `take_dungeons` | 501 |
| `take_events` | 516 |
| `take_recipes` | 531 |

The report framed this as "content reaches the index, gets stashed, and stops."
That framing is also wrong, and the correction changes what you should do:
`mods/reachlock/` has **no** `dialogues/`, `dungeons/`, `events/`, or `recipes/`
directory. All four are registered as envelope dirs in
`reachlock-core/src/content/dirs.rs:85-88`, but nothing is authored for any of
them. Nothing is being dropped — there is no content *and* no consumer.

**So this is not a data-loss bug.** It is iron rule 8 ("a system nobody can
reach is not done") pointing at four unfinished pipelines. Scope for this
sprint:

- **Do not** delete the takers. The `set_*` side is wired and the dirs are
  registered; deleting the takers would make the pipeline harder to finish, not
  cleaner.
- **Do** replace each `#[expect(dead_code)]` with a comment naming what is
  missing (no authored content, no consuming system) and pointing at the sprint
  that owns finishing it.
- **Do** add a line to the gotcha ledger in `docs/sprints/00-INDEX.md`: four
  content directories are registered and loadable but have zero authored files
  and no consumer.

Wiring the four features is out of scope. Say so in the PR.

---

## T9 — Non-humanoid reference content

Verified: **0 of 13** authored souls are Voidborn or Xenotype. The breakdown is
10 human, 2 android (`prudence.ron`, `risc.ron`), 1 robot (`boris.ron`).

The painters exist and are unexercised by any authored file:
`paint_voidborn` at `reachlock-client/src/pixel.rs:835`, and the Xenotype arm
reached via `paint_character` at `pixel.rs:734`.
`body_kind_from_species` (`pixel.rs:419-425`) maps all five species.

**Deliverable:** author at least one Voidborn soul and one Xenotype soul in
`mods/reachlock/souls/`, each with an explicit `look:` block, plus one
species-appropriate hostile archetype each in `mods/reachlock/combat/` (all
four existing files are generic raiders/robots).

### ⚠️ FOOT-GUN 4 — RON syntax that fails silently

Every one of these produces a file that *every* loader skips without an error.
The repo has lost authored content to each of them:

| Wrong | Right |
|---|---|
| `skin_color: Some([128, 160, 96])` | `Some((128, 160, 96))` — `[u8; 3]` is a **tuple** in RON |
| `allowed_variations: 65535` | `allowed_variations: (65535)` — newtype needs parens |
| `species: Human` | `species: human` — variants are snake_case |
| `payload: origin(…)` | `payload: origin((…))` — newtype payload needs the second paren |
| bare `Soul(…)` at top level | wrap in a `ContentFile` envelope |

Both new souls also need `look.hair_style: Some(0)` (Bald) if the species has
crests or tendrils rather than hair.

**Verify** with `cargo run -p reachlock-cli -- content check mods/reachlock`.
A file that parses as no known payload is reported as UNPARSEABLE — it must not
appear. Then confirm the sprites actually render (T10's preview, or the sprite
viewer tab).

**Also:** procedural crew generation weights species from `local_species`
(`reachlock-client/src/systems/crew.rs:897,926-929`). When `local_species` is
empty the generator falls back to a default distribution. Authoring stations
with broader species pools is a content follow-up, **out of scope here** — note
it in the PR.

---

## T10 — Character model fidelity (SCOPED DOWN — read carefully)

The original report proposed seven fidelity changes including full 3D
characters. **Only two are in scope for this sprint.** The rest are recorded in
Appendix B as a backlog, not as work.

Current verified state: 16×26 px sprites (`pixel.rs:753`), 4 facings
(`pixel.rs:605-608`), **2** frames per facing —
`interior.rs:142` declares `frames: [[Handle<Image>; 2]; 4]`, built by
`pixel::character_frames` (`interior.rs:1232`).

### T10a — Author `look` blocks on existing souls (content only)

Most souls leave `look: None` and fall back to seeded appearance. Adding
explicit `look` blocks with varied skin tones and uniforms is pure content work
with immediate visual payoff and zero code risk. Same RON foot-guns as T9.

### T10b — A portrait painter

`SoulFile.portrait_id` exists (`reachlock-core/src/soul/types.rs:108`) and is
**dead**: every construction site in the tree passes `String::new()` —
`character_creation.rs:1347`, `inventory.rs:526`, `crew.rs:960/1617/1656`,
`soul.rs:294`, `dialogue.rs:359`, `identity.rs:118`, `generator/soul.rs:150`,
`soul/types.rs:301`, `determinism.rs:667`. Dialogue falls back to unicode glyphs
(`reachlock-client/src/systems/dialogue.rs:492-494`: `◎ ⬡ ▣`).

Add `paint_portrait()` in `pixel.rs` rendering a 64×64 head-and-shoulders view,
and use it in the dialogue panel in place of the glyph.

### ⚠️ FOOT-GUN 5 — determinism

The sprite pipeline is seed-deterministic and that guarantee is gated in CI.
**A new generator, or a change to an existing one, requires extending
`reachlock-core/src/determinism.rs` and recapturing goldens deliberately.** If
the manifest changes, say so in the commit message — a silent golden change is
a bug (iron rule 3). Cross-platform determinism gates are non-negotiable.

`paint_portrait` is a new generator. Plan for the golden recapture.

### Explicitly OUT of scope

Sprite resolution changes (16×26 → 32×52), 4–8 frame walk cycles, equipment
layers, idle animations, and any 3D character path. The frame-count change alone
rewrites `[[Handle<Image>; 2]; 4]`, every `paint_*` painter, and
`character_frames`' return type — that is its own sprint.

---

## Sequencing

T2, T4, T5 are small and independent — land them first for quick green.
T1 before T7. T3 after T2 and T5 (it gates what they fix).
T8, T9, T10a are independent of everything.
T10b last; it carries the determinism recapture.

Per repo playbook, **keep this to focused commits**. If the branch gets wide,
split T9/T10 onto their own branch rather than piling slices on one PR.

---

## Acceptance gates

- [ ] `make check` green — fmt, clippy `-D warnings`, tests, purity, features,
      content, theme, resources, dead-code, **and the new check-schema**
- [ ] `cargo run -p reachlock-cli -- content check mods/reachlock` clean
- [ ] Corrupting a soul file produces a visible editor warning naming the file
      (then restore the file)
- [ ] A `hulls/` file whose description contains `HullFrame(` classifies as
      `HullMesh`
- [ ] Dialogue AI generation is prompted with `dialogue.schema.json`
- [ ] `crew_package.schema.json` exists and `loup_garou.ron` validates
- [ ] Saving a commented file prompts before stripping comments
- [ ] At least one Voidborn and one Xenotype soul render in game
- [ ] Determinism manifest change (if any) called out in the commit message

## Non-goals

Wiring the dialogue/dungeon/event/recipe features. Sprite resolution changes.
Multi-frame walk cycles. 3D characters. A sixth species. Broadening station
`local_species` pools.

---

## Appendix A — claims that were wrong

Recorded so nobody re-fixes them.

1. **"Six dead stash takers."** Two are live (`take_tropes`, `take_careers` —
   see T8). Four are dead, and the reason is absent content plus absent
   consumer, not a dropped payload.
2. **"`soul.schema.json` is missing `look`."** Historical, already fixed —
   `soul.schema.json:454`. The live half is the missing drift gate (T3).
3. **"The editor never reports parse errors."** Overstated. File → Open reports
   them (`main.rs:287`). Only the constructor scan is silent. And the tree is
   clean today (73 ids / 69 refs / 0 unparseable), so T1 fixes a latent trap,
   not active loss — do not report it as data currently being lost.

## Appendix B — fidelity backlog (NOT this sprint)

Scale sprites to 32×52 or 48×78 (`pixel.rs` canvas constants + `character_frames`
dimensions) · 4–8 frame walk cycles (`CharacterSprite`, all painters,
`character_frames` return type) · equipment layers on the `Look` struct
(painters already layer shirt → jacket → hair) · idle animations via the
existing `animate_figures` phase timer · 3D characters. All five need the
determinism story worked out first.
