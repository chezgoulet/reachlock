# ReachLock — Operator's Guide

**How to run and use every component.** Written against `testing` on
2026-07-25. This is the "how do I drive it" document; `AGENTS.md` is the "how
do I change it" document, and `docs/CONTENT-READINESS.md` is the "what is
missing" document.

- §1 Setup — [Setup](#1-setup)
- §2 The five crates — [What's in the box](#2-whats-in-the-box)
- §3 The game — [The client](#3-the-client-playing-reachlock)
- §4 **The editor** — [The content editor](#4-the-content-editor)
- §5 The CLI — [The `reachlock` binary](#5-the-reachlock-cli)
- §6 The server — [The ledger server](#6-the-ledger-server)
- §7 The authoring loop — [End to end](#7-the-authoring-loop-end-to-end)
- §8 Gates — [What `make check` actually checks](#8-gates)
- §9 [Troubleshooting](#9-troubleshooting)
- §10 [File locations](#10-where-everything-lives-on-disk)

---

## 1. Setup

The toolchain is pinned in `rust-toolchain.toml` (channel `1.96.0`, with
`rustfmt` and `clippy`). Cargo picks it up automatically; you do not install a
version by hand.

```sh
# Cargo's bin dir is not on PATH in a fresh shell.
export PATH="$HOME/.cargo/bin:$PATH"

# Native Bevy system deps (Debian/Ubuntu).
sudo apt-get install -y libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev

cargo build          # everything
make check           # fmt + clippy + tests + purity + features + content
```

The workspace builds with `debug = false` on purpose — Bevy debuginfo target
directories run to many gigabytes. Don't flip it.

### The one-line smoke test

```sh
make run       # the game launches to a main menu
make check     # ends with "all gates green"
```

---

## 2. What's in the box

| Crate | Binary | What it is |
|---|---|---|
| `reachlock-core` | *(library)* | Generators, seed protocol, contract engine, content model. Zero rendering/IO deps — integer math only |
| `reachlock-client` | `reachlock-client` | The Bevy game |
| `reachlock-editor` | `reachlock-editor` | The egui content editor — 27 content types |
| `reachlock-server` | `reachlock-server` | Axum WebSocket ledger + auth + admin |
| `reachlock-cli` | **`reachlock`** | `gen`, `content`, `mod`, `determinism`, `check`, `codex` |

Every component works with no server and no network. Online adds; it never
replaces.

---

## 3. The client: playing ReachLock

```sh
make run           # normal
make run-debug     # Bevy `debug-names` feature — ECS errors name real systems
```

`make run` wraps the launch in `WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11`. That
is a deliberate workaround for a winit 0.30.13 panic on Wayland, not a
leftover — see §9.

### Main menu

Cycle with **Tab / ↑ / ↓**, confirm with **Enter**.

| Entry | What it does |
|---|---|
| New Game | Enters the 6-step character creation flow |
| Continue | Loads `save/player.ron`. Greyed out when no save exists |
| Settings | Opens the settings panel, which owns the keyboard while open |

### Character creation

Six steps: **Identity → Appearance → Origin → Ship & Crew → Galaxy → Confirm.**

| Key | Effect |
|---|---|
| `Enter` | Confirm / advance. On Confirm, starts the game |
| `Esc` | Back one step. At Identity, returns to the main menu |
| `R` | Randomize the current step (seeded generators) |
| `1`–`5` | *(Identity)* pick species: Human, Android, Robot, Voidborn, Xenotype |
| `P` | *(Identity)* cycle pronouns: they/them, she/her, he/him, it/its, xe/xem, custom |
| `1`–`9` | *(Origin)* pick an origin |

Identity requires a name and Origin requires a selection before `Enter`
advances — the other steps always advance.

**The origin is the game.** It supplies your ship template, crew, career path
and rank, faction standing, starting credits, gear, known systems, start
location, and opening log entries. The engine names no specific ship or crew;
`make check-purity` fails the build if it ever does.

### In-game modes

`AppState` is `MainMenu` / `CharacterCreation` / `InGame`. Inside `InGame`,
`GameMode` is the real state machine:

| Mode | What it is |
|---|---|
| `SpaceFlight` | Flying in a system's space volume |
| `Landed` | Top-down on a station or planet surface |
| `OnBoard` | Side-on inside your own ship, at consoles |
| `Docking` / `Undocking` | Short camera beats entering/leaving a station |
| `Hyperspace` | Gate-jump transit |
| `Paused` | Overlay — stops the sim clock without despawning the scene |

### Default keybinds

Everything below is rebindable in Settings. Defaults deliberately overlap
where contexts are disjoint (landed combat and the mission board are never
both live); only per-action uniqueness is enforced.

**Flight**

| Action | Key | Action | Key |
|---|---|---|---|
| Thrust forward | `W` | Thrust backward | `S` |
| Strafe left | `A` | Strafe right | `D` |
| Roll left | `Q` | Roll right | `E` |
| Boost | `LShift` | Brake | `Space` |

**Ship combat**

| Action | Key | Action | Key |
|---|---|---|---|
| Fire weapons | `F` | Fire missile | `G` |
| Cycle target | `R` | Cycle target back | `X` |
| Launch chaff | `C` | Power select | `↑` / `↓` |
| Power adjust | `←` / `→` | | |

**Landed combat**

| Action | Key | Action | Key |
|---|---|---|---|
| Light attack | `J` | Heavy attack | `K` |
| Dodge | `Space` | Block | `Q` |
| Lock-on next | `Tab` | Lock-on previous | `LShift` |

**Panels and interaction**

| Action | Key | Action | Key |
|---|---|---|---|
| Interact | `E` | Pause | `Esc` |
| Comms | `T` | Crew conference | `Y` |
| Galaxy map | `M` | Inventory | `I` |
| Crew roster | `U` | Ship log | `Z` |
| Captain's log | `L` | Mission board | `J` |
| Career | `O` | Culture | `P` |
| Discovery | `H` | Factions | `N` |
| Market | `K` | Leave helm | `B` |
| Diegetic help | `F1` | | |

**Ship editors, consoles, and system**

| Action | Key | Action | Key |
|---|---|---|---|
| Editor confirm / cancel | `Enter` / `Esc` | Editor cursor | arrows |
| Editor next tab | `Tab` | Editor rotate | `R` |
| Editor delete | `Backspace` | Install contract | `F2` |
| Consoles 1–4 | `1`–`4` | Quick save / load | `F5` / `F9` |
| Push-to-talk | `V` | Cycle mic device | `F7` |

Settings persist to `save/settings.ron` and are written on apply and on exit.
A corrupt file falls back to defaults with a warning rather than refusing to
start.

> **Known:** seven systems (dilemma, deliberation, encounters, onboarding,
> character creation, dialogue, jump) read `KeyCode::Digit1-9` directly
> instead of going through the keybind map, so **number-row selection cannot
> be rebound**. Each gates on its own state, so there is no live collision.
> Tracked in `docs/CONTENT-READINESS.md` §6.

---

## 4. The content editor

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -p reachlock-editor
```

On Wayland, use the same workaround as the game:

```sh
WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11 cargo run -p reachlock-editor
```

> ### Run it from the repository root
>
> The content root defaults to the **relative** path `mods/reachlock`. Launch
> the editor from anywhere else and it silently reads and writes a different
> tree — this has already produced stray files under `reachlock-editor/mods/`.
> Either `cd` to the repo root first, or set an absolute `content_root` in
> **Edit → Preferences**.

### Layout

```
┌──────────────────────────────────────────────────────────────────┐
│ File   Edit   View   AI   Help                        (menu bar) │
├──────────────────────────────────────────────────────────────────┤
│ Seed: [ 42 ]  [Reroll All]  [Lock Current]           (seed panel)│
├──────────────────────────────────────────────────────────────────┤
│ [describe what you want…]                    [Generate] (AI bar) │
├───────────────┬──────────────────────────────┬───────────────────┤
│ Content       │  tab  tab* tab               │  Preview          │
│ Browser       │                              │                   │
│  systems/     │   the active editor          │  summary card     │
│  souls/       │                              │  + validation     │
│  origins/     │                              │    issue count    │
│  …            │                              │                   │
├───────────────┴──────────────────────────────┴───────────────────┤
│ Ready                              3 unsaved · 5 editor(s) open  │
└──────────────────────────────────────────────────────────────────┘
```

A `*` on a tab means unsaved. The window title mirrors the unsaved count, and
the status line colours the count amber, then red past five. Closing a dirty
tab, closing all tabs, quitting, or hitting the window's X all stop and ask.

### Keyboard shortcuts

| Key | Action |
|---|---|
| `Ctrl+S` | Save the active editor |
| `Ctrl+Shift+S` | Save As (native file dialog) |
| `Ctrl+O` | Open a `.ron` content file |
| `Ctrl+Z` | Undo — *only when no text field has focus* |
| `Ctrl+Y` / `Ctrl+Shift+Z` | Redo |
| `Ctrl+W` | Close the active tab |
| `Ctrl+Q` | Quit (asks about unsaved changes) |
| `Delete` | Delete the selected entry, with confirmation |
| `Esc` | Close a window / cancel a dialog |
| `F1` | Help window — searchable, one paragraph per editor |

`F1` is the fastest reference for "what is this content type": it renders the
same per-type context paragraph the AI prompt uses.

### The three ways to create content

Every editor supports all three, and they compose — generate, then hand-tune.

**1. Manual.** `File → New`, pick a type, fill in fields.

**2. Procedural.** Each editor has a *Generate from Seed* button. The same
seed always produces the same content on every platform. The seed panel drives
this in bulk:

- **Reroll All** applies the current seed to every open editor that accepts
  rerolls, then auto-increments so repeated clicks walk through seeds.
- **Lock Current** derives a stable seed by hashing the active tab's name,
  giving you a reproducible starting point to riff on.
- Seeds are masked to ≤ 2^53−1 so they survive a JSON round-trip.

Thirteen editors opt out of Reroll All because their content is authored
rather than generated: career, dialogue, dungeon, ecosystem, event, gate
network, origin, planet culture, recipe, room templates, scripted encounter,
theme, trope.

**3. AI.** The bar under the seed panel sends your description plus the active
editor's JSON schema to any OpenAI-compatible `/v1/chat/completions` endpoint.

- Default endpoint is a **local Ollama** at `http://localhost:11434/v1`, model
  `llama3.2:3b`, 4096 max tokens. Pull a model first: `ollama pull llama3.2:3b`.
- Configure under **AI → AI Settings…**, with a *Test Connection* probe.
  Persisted to `save/editor-settings.ron`.
- Generation runs on a background thread. If you switch tabs before it
  returns, the result is discarded with "the active editor changed" — it never
  lands in the wrong editor.
- The response is parsed with four progressively looser strategies (whole
  body, ```` ```json ```` fence, first balanced `{}`, first balanced `[]`) and
  then validated against the schema. Schema failures are surfaced as warnings
  but the content is still applied, so you can fix it by hand.
- **14 of 27 editors accept AI JSON**: charted system, contract, economy,
  enemy, faction, gate network, hull frame, hull mesh, item, location, room
  templates, soul, station, storyline. The rest report *"this editor stores
  data in a content envelope — AI population isn't wired yet"* and you should
  use procedural generation instead.
- Nothing is saved automatically and, with local Ollama, nothing leaves your
  machine.

### Content browser (left panel)

A live file tree over the content root, one section per content type. Filter
box at the top; rescans disk every 2 seconds and immediately after a save.
Right-click a file to delete it, with confirmation. Toggle the whole panel
from **View → Content Browser**.

Three types share the `hulls/` directory — the browser peeks at the RON
payload tag to route a file to the hull-frame, hull-mesh, or room-templates
editor.

### Preview panel (right)

Renders the active editor's own summary card (all 27 implement one) plus a red
"*N* validation issue(s)" line when the editor's `validate()` returns
problems. With no tab open it shows an **Open Recent** list from preferences.

For a real visual check, `reachlock content preview` writes an SVG (§5).

### Undo

Snapshot-based, per tab: up to **50 steps**, with changes within **800 ms**
coalescing into one step so typing a sentence doesn't cost one step per
keystroke. Undo is available in all 25 file-backed editors (the two previewers
persist nothing). Redo clears whenever you make a new change.

Note the caveat: `Ctrl+Z` is only intercepted when **no text field has focus**,
so egui's own in-field undo wins inside a text box.

### Saving

- `Ctrl+S` saves the active tab. `Ctrl+Shift+S` opens a native Save As that
  suggests the right directory for the type.
- **20 of 27 editors are multi-entry** — they hold several entries at once and
  write *each dirty entry back to its own file*, not all of them onto the tab
  path. Trust this; do not try to "consolidate" a multi-entry save.
- **Autosave is off by default.** Set `auto_save_secs` in Preferences to turn
  it on. Only file-backed editors autosave.
- **File → Validate All Open Editors** runs every open editor's validator and
  reports "*N* clean, *M* with issues" in a dismissable window.

### Preferences (Edit → Preferences…)

Persisted to `save/editor-preferences.ron`, written on every change so they
survive a crash.

| Setting | Default | Notes |
|---|---|---|
| Theme | Dark | Dark / Light, applies immediately |
| Font scale | 1.0 | 0.75–1.5 UI zoom |
| Show line numbers | on | Informational, for code-like text areas |
| Auto-save seconds | 0 | 0 disables |
| Content root | `mods/reachlock` | **Set this to an absolute path** (see the warning above) |
| Recent files | — | 10 entries, drives Open Recent |

### The 27 content types

`File → New` groups them the same way. Every one is reachable from both the
browser and `File → New` — a test enforces it.

| Group | Type | Directory | What it is |
|---|---|---|---|
| **Systems** | Charted System | `systems/` | A star system: 3D position, biome, description shown on the galaxy map |
| | Gate Network | `gate_network/` | Directed graph of gates between systems, with status and controlling faction |
| **Ships** | Hull Frame | `hulls/` | Where hardpoints, armour zones, decals and the engine mount go. Classes: Shuttle, Corvette, Freighter, Station, Rock |
| | Hull Mesh | `hulls/` | How a hull is *outfitted* against a frame — hardpoint items, engine, plating, paint, decals. Not a raw mesh |
| | Room Templates | `hulls/` | Interior room templates: kind, dimensions, required systems, furniture slots, adjacency bonuses |
| | Station | `stations/` | Exterior hull, interior room/door layout, NPC spawns with dialogue |
| **Characters** | Soul | `souls/` | An NPC: species, traits, emotional state, memories, relationships, secrets, goals, optional dialogue |
| | Enemy Archetype | `combat/` | Landed-combat enemy: HP, speed, attack windows in ticks, block/dodge windows, chase radii, flee threshold |
| | Career Path | `careers/` | 2–10 ranks with requirements and perks, plus conflicting paths |
| | Origin | `origins/` | The starting package: career, faction standing, credits, ship template, gear, crew, known systems, opening log |
| **World** | Faction | `factions/` | Doctrine, tariffs, territory, internal divisions, relationships, goods produced |
| | Location | `locations/` | A hostile interior — derelict, bunker, station. Rooms, enemy spawns, props, keycard gates |
| | Ecosystem | `ecosystems/` | Species with taxonomy and roles, a food web, and event-driven change |
| | Planet Culture | `cultures/` | Language, customs, social structure, architecture, values, a cultural quirk |
| | Dungeon Layout | `dungeons/` | Grid of rooms with connectors and tags; puzzles, encounters, reward tables |
| **Narrative** | Storyline | `storylines/` | A faction's arc: chapters with triggers and narration |
| | Dialogue Tree | `dialogues/` | Node graph — NarratorLine, NpcLine, PlayerChoice, Branch, End — with conditions, consequences, variable interpolation |
| | Scripted Event | `events/` | Timeline of stages with AND/OR trigger trees and consequences |
| | Trope Template | `tropes/` | Procedural narrative template with named slots and branching choices |
| | Scripted Encounter | `encounters/` | Multi-scene authored encounter: prerequisites, scene graph, consequences, endings |
| | Contract | `contracts/` | Player automation: trigger, prioritized rules, actions, optional LLM fallback authority |
| **Economy** | Economy Goods | `economy/` | Trade catalogue: category, base price, mass, contraband flag |
| | Item | `items/` | Generated equipment: type hierarchy, tier 1–10, seed, faction/biome origin |
| | Crafting Recipe | `recipes/` | Ingredient grid, output config, skill requirement, workbench, duration |
| **Audio** | Music Theme | `themes/` | Seed note sequence, scale, tempo range, allowed-variations bitmask |
| **Preview** | Item Browser | — | Live previewer over the item generator. Persists nothing |
| | Sprite Viewer | — | Live character-look explorer. Persists nothing |

Six of these are **plumbed with zero authored files** and are the highest-value
place to start: dialogues, dungeons, events, recipes, encounters, tropes.

### RON traps that will cost you an hour

Content files are RON, not JSON, and the parse errors name the struct rather
than the real problem.

| Trap | Wrong | Right |
|---|---|---|
| Fixed-size arrays are **tuples** | `Some([176, 148, 92])` | `Some((176, 148, 92))` |
| Newtype structs need parens | `allowed_variations: 65535` | `allowed_variations: (65535)` |
| Content needs the envelope | bare `Theme(...)` | wrapped in `ContentFile` |
| Career conflicts are symmetric | `a → b` only | both `a → b` and `b → a` |
| Seeds must fit 53 bits | `9012774451336290` | ≤ `9007199254740991` |

A file that parses as no known payload is **skipped by every loader** — worse
than one that errors, because nothing tells you. `reachlock content check`
reports these as `UNPARSEABLE`. One authored theme sat dead in the tree this
way for months.

**The editor does not preserve comments.** RON cannot round-trip them through
deserialize → serialize. Never open a hand-commented file in the editor
expecting the comments to survive a save.

### What is *not* in the editor

Five source files under `reachlock-editor/src/` are **not declared as modules
in `main.rs`, so they are never compiled**: `command_palette.rs`,
`cross_ref.rs`, `diff.rs`, `template_manager.rs`, and `validation.rs`
(~1,370 lines, all added in commit `676e2dd4`).

That means the following do **not** exist in the running editor, whatever the
sprint briefs say:

- Command palette (`Ctrl+P`) — S67
- Cross-reference index: go-to-definition, find-usages, id autocomplete — S69
- Template manager, diff view, the editor-side validation panel

Wiring them in does not compile as-is: they reference a `similar` crate that
is not a dependency, an `Editor::validate_cross_refs` trait method that does
not exist, and they predate the `Origin` content type. Treat this as unstarted
work, not a wiring bug. Note that `docs/CONTENT-READINESS.md` §3 currently
credits `cross_ref.rs` as shipped editor UX — that line is wrong.

Also genuinely missing, and the biggest gap in the loop: there is **no
"Preview → Launch in game"**. Every visual check is save → quit → run the
client → navigate there.

---

## 5. The `reachlock` CLI

```sh
cargo build -p reachlock-cli
./target/debug/reachlock --help
# or: cargo run -q -p reachlock-cli -- <args>
```

### `content` — the one you'll use daily

```sh
# Whole-tree reference integrity. Exit 1 on anything broken.
reachlock content check mods/reachlock
reachlock content check mods/reachlock --orphans   # also list unreferenced content

# One file's structural integrity: seed range, universe, degenerate
# triangles, doors referencing real rooms.
reachlock content validate mods/reachlock/origins/drifter.ron

# Render geometry to a dependency-free SVG you can open in a browser.
reachlock content preview mods/reachlock/hulls/corvette_mk2.ron --out /tmp/x.svg

# Catalogue-specific validators.
reachlock content validate-goods
reachlock content validate-factions
reachlock content validate-storylines

# Upload to a running server as a content override.
reachlock content publish file.ron --server http://127.0.0.1:40711 \
    --universe all --priority curated
```

**`validate` and `check` answer different questions.** `validate` sees one
file, and a file naming a ship that does not exist is perfectly well-formed —
which is how ten origins came to reference eight ship templates, nine careers
and ten souls that had never been authored. `check` walks the whole tree.

References are **typed**: an origin's `ship_template` must name an actual ship
template, not merely some id that exists somewhere. Deliberate non-references:
`soul.identity.faction_affiliation` (prose, e.g. `"Sorrow Station (independent,
ISC-adjacent)"`) and `origin.starting_gear[].item_id` (seed-generated, no
authored id space). A `ShipTemplate.hull_id` with no authored hull mesh is not
broken either — it falls back to a generated hull by design.

### `gen` — run a generator without a window

Every subcommand takes `--seed` and prints a text summary; most also export a
file.

```sh
reachlock gen hull    --seed 42 --class corvette      --svg /tmp/hull.svg
reachlock gen station --seed 42 --kind trade --size 2 --svg /tmp/station.svg
reachlock gen planet  --seed 42 --biome frontier      --ppm /tmp/planet.ppm
reachlock gen music   --seed 42 --mood calm --seconds 4 --wav /tmp/theme.wav
reachlock gen system  --seed 42 --biome frontier --fidelity full --svg /tmp/sys.svg
reachlock gen item    --seed 42 --family kinetic_weapon --tier 4 \
                      --faction compact --biome frontier --ppm /tmp/icon.ppm
reachlock gen ui-panel --seed 42
```

`--fidelity sparse` is the deep-space trim. Item families are tokens like
`kinetic_weapon`, `shield`, `engine`.

### `determinism` — the cross-platform gate

```sh
make determinism                                  # local self-check
reachlock determinism emit > manifest.json        # this platform's checksums
reachlock determinism check manifest.json         # compare; exit nonzero on divergence
```

354 golden entries, bit-identical across x86_64, aarch64 and i686. The 32-bit
target is there to catch pointer-width assumptions the two 64-bit targets
would both agree on.

**If you add or change a generator, extend `core/src/determinism.rs` and
recapture goldens deliberately** — and say so in the commit message. A silent
manifest change is a bug.

### `mod` — packaging

```sh
reachlock mod pack mods/mymod -o mymod.reachmod   # validate + package
reachlock mod install mymod.reachmod              # into the user's mods dir
reachlock mod list                                # versions, load order, conflicts
```

### `check` and `codex` — agent tooling

```sh
reachlock check agent --json      # iron-rule battery, machine-readable
reachlock codex brief S19         # summarize a sprint brief with code cross-refs
reachlock codex types reachlock-core
reachlock codex deps <module-path>
reachlock codex diff <git-ref>    # scan a diff for iron-rule violations
reachlock codex update            # regenerate AGENT-INDEX.md / AGENT-TYPES.md
reachlock codex pattern <task-type>
```

---

## 6. The ledger server

The server is a **ledger, not a simulator**. It arbitrates seed discovery
(first write wins), verifies signed evaluations, proxies LLM calls under tier
gates, and ticks the universe. Everything runs offline without it.

### Zero-infrastructure mode

```sh
make server     # 127.0.0.1:40711, all stores in memory
```

### With the dev stack

```sh
make db         # Postgres + Redis + Mailpit, BLOCKS until healthchecks pass
cp .env.example .env
make server-db  # runs with --features postgres,redis, reads .env
```

| Service | Where | Notes |
|---|---|---|
| Postgres | `127.0.0.1:5432` | user/pass `reachlock`; databases `reachlock` (dev) and `reachlock_test` |
| Redis | `127.0.0.1:6379` | Sessions, rate limiting, presence |
| Mailpit | http://localhost:8025 | SMTP on 1025. **This is what makes the verify / reset / 2FA flows testable** — every email is visible in the web UI |

Other targets: `make db-down`, `make db-reset` (**destroys volumes and your
dev data**), `make db-psql`, `make db-test` (live-Postgres battery against the
*separate* test database).

Migrations run automatically at startup from
`reachlock-server/migrations/`. Setting `REACHLOCK_DB` is the single switch
that selects the Postgres stores.

### Key environment variables

`.env.example` is fully commented; the essentials:

| Variable | Purpose |
|---|---|
| `REACHLOCK_DB` | Postgres URL. Unset ⇒ in-memory |
| `REACHLOCK_REDIS_URL` | Optional; falls back to in-memory |
| `REACHLOCK_SECRET_KEY` | 32 bytes hex. Encrypts TOTP secrets at rest — **2FA enrollment returns 503 without it** |
| `REACHLOCK_BYOK_KEY` | 32 bytes hex. Encrypts player-supplied LLM API keys |
| `REACHLOCK_ADMIN_KEY` | Bearer for `/admin/*` as `Authorization: Admin <key>`. Empty ⇒ admin disabled entirely |
| `REACHLOCK_BYOK_ALLOWED_HOSTS` | SSRF allowlist for player-supplied LLM endpoints |
| `REACHLOCK_BIND` | Default `127.0.0.1:40711` |
| `REACHLOCK_AUTH=1` | Require a session token on the WebSocket handshake |
| `REACHLOCK_TRUSTED_PROXIES` | Only trust `X-Forwarded-For` from these. Unset ⇒ never |
| `REACHLOCK_ALLOWED_ORIGINS` | CORS. Unset ⇒ same-origin only |
| `REACHLOCK_SMTP_URL` / `_FROM` | Unset ⇒ emails are written as files to `data/emails` |

Generate fresh key material with `make dev-secrets`. **The values in
`.env.example` are committed to a public repo** — treat anything signed or
encrypted with them as public.

### Endpoints

| Group | Routes |
|---|---|
| Health | `GET /health`, `GET /metrics` |
| Game | `ANY /ws` (the real protocol), `POST /seed/discover`, `GET /content/system/{id}`, `POST /content/publish` |
| Auth | `register`, `verify-email`, `login`, `logout`, `forgot-password`, `reset-password`, `delete-account`, `cancel-deletion`, `oauth/token`, `dev` |
| 2FA | `2fa/enable`, `2fa/verify`, `2fa/challenge`, `2fa/disable` |
| Billing | `POST /billing/checkout`, `POST /billing/portal`, `POST /stripe/webhook` |
| BYOK | `POST /byok` |
| Admin | `/admin/dashboard`, `/players`, `/players/{id}` (+`/ban`, `/unban`, `/role`), `/audit`, `/auth-config`, `/log-level`, `/universes`, `/tick/trigger`, `/content/purge` |

> **Known:** auth token and TOTP state live in a `Mutex<HashMap>` on
> `AppState` with no Postgres path, so **2FA does not survive a server
> restart**. Single-player and local dev are unaffected.

---

## 7. The authoring loop, end to end

```sh
# 0. Bring up the editor from the repo root.
cd /path/to/reachlock && cargo run -p reachlock-editor

# 1. Author. File → New → (group) → type.  Ctrl+S.

# 2. Check the file itself.
reachlock content validate mods/reachlock/dialogues/first_contact.ron

# 3. Check the tree — this is the one that catches references to ids
#    nothing defines.
reachlock content check mods/reachlock

# 4. Eyeball geometry without launching the game.
reachlock content preview mods/reachlock/stations/sorrow.ron --out /tmp/s.svg

# 5. Full gate before you commit.
make check
```

### Two content trees exist — know which one you mean

| Tree | Read by | Contains |
|---|---|---|
| `mods/reachlock/` | The **client** | The real, gated tree. 71 definitions / 69 references, clean |
| `content/` | The **server** | `factions/`, `souls/`, `storylines/`, `stations/`, `gate_network/`, `dungeons/`, `events/`, `schemas/` |

They have **diverged** — `content/souls/tove.ron` contradicts `docs/LORE.md`.
`content check` only walks the tree you point it at. Picking one is on the
list in `docs/CONTENT-READINESS.md` §5.

---

## 8. Gates

`make check` runs six things. A step isn't done until it passes.

| Target | What it proves |
|---|---|
| `fmt` | `cargo fmt --all --check` |
| `clippy` | `-D warnings`, `--all-targets` — tests and benches too, so a test file can't rot silently |
| `test` | The whole workspace |
| `check-purity` | Four checks: the engine names no specific content; content imports no engine code; core has no cross-crate `include_str!`; core's dependency tree contains no rendering/async/HTTP crate |
| `check-features` | Type-checks `postgres`, `redis`, and both together — off by default, so they rot unnoticed. No database needed |
| `check-content` | Whole-tree reference integrity |

Opt into running it on every commit:

```sh
git config core.hooksPath .githooks
```

### The lesson worth internalizing

The settings-consumer gate required every settings field to name a consumer —
and checked only that the *string* was present, never that the named
`module/symbol` existed. Four accessibility settings shipped in the menu,
persisted to disk, completely inert, all certified green for months. The code
being gated even contained `let _cb = settings...` bindings commented *"satisfy
consumer registry (side-effect free for now)"* — the gate being fed on purpose.

**Ask what a gate can distinguish, not what it asserts.** A discard binding
introduced to please a check is a defect in the check.

---

## 9. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `reachlock: command not found` | `export PATH="$HOME/.cargo/bin:$PATH"` — not on PATH in fresh shells |
| Panic at `winit .../wayland/window/state.rs:694`, `NonZeroU32::new(...).unwrap()` | winit 0.30.13: the compositor didn't send a configure event before first render. Prefix with `WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11`. `make run` already does; the editor does not. Remove when Bevy's winit dep moves past 0.30.13 |
| Editor writes files somewhere unexpected | Content root is CWD-relative. Launch from the repo root or set an absolute path in Preferences |
| RON error `Expected opening '('` | A fixed-size array serializes as a tuple: `(1, 2, 3)`, not `[1, 2, 3]` |
| RON error naming a struct, on a line that looks fine | A newtype needs parens: `field: (65535)` |
| A content file is silently ignored by the game | It parses as no known payload — usually a missing `ContentFile` envelope. `content check` lists it as `UNPARSEABLE` |
| Schema rejects valid content | The schema may be the wrong one. Two were: `soul.schema.json` had no `look` property, `career_path.schema.json` demanded a string `faction_id` when the type is `Option<String>` |
| Career fails at load | `conflicting_paths` are symmetric and validated at load. `a → b` without `b → a` is a hard failure, not a warning |
| Seed rejected by the gate | Must be ≤ 2^53−1 so it survives a JSON round-trip |
| Editor lost content in another entry | Should not happen — 20 editors save each dirty entry to its own path. If a single-entry `save(path)` ever collapses all entries onto the tab path, that's the bug |
| Comments vanished from a content file | RON cannot round-trip comments. Don't open commented files in the editor |
| `cargo fmt --all` churns dozens of untouched files | rustfmt version skew. Format only your crate |
| 2FA enrollment returns 503 | `REACHLOCK_SECRET_KEY` unset — it encrypts TOTP secrets at rest |
| Admin routes 401/404 | `REACHLOCK_ADMIN_KEY` empty disables admin entirely. Header is `Authorization: Admin <key>` |
| `make db-test` destroys dev data | It shouldn't — it targets `REACHLOCK_TEST_DB`, a separate database. If it does, check your `.env` |
| Bevy query filter trips clippy `type_complexity` | `#[allow(clippy::type_complexity)]` on the system fn is the accepted pattern here |

---

## 10. Where everything lives on disk

| Path | What |
|---|---|
| `mods/reachlock/` | The client's content tree — the one the editor edits by default |
| `content/` | The server's content tree (diverged from the above) |
| `mods/reachlock/schemas/` | 26 JSON schemas, used by editor validation and AI prompts |
| `save/settings.ron` | Game settings and keybinds |
| `save/player.ron` | The save file ("Continue" reads this) |
| `save/onboarding_completed.flag` | First-run onboarding marker |
| `save/editor-settings.ron` | Editor AI config: endpoint, model, key, token budget |
| `save/editor-preferences.ron` | Editor theme, zoom, autosave, content root, recent files |
| `data/emails/` | Emails when no SMTP is configured |
| `reachlock-server/migrations/` | SQL migrations, run automatically at startup |
| `.env` | Server config (gitignored). Start from `.env.example` |
| `docs/REACHLOCK-V2-SPEC.md` | Full design spec, 24 sections |
| `docs/sprints/00-INDEX.md` | Sprint index, dependency waves, playbook, gotcha ledger |
| `docs/CONTENT-READINESS.md` | What exists, what's missing, what's deliberately unfixed |
| `docs/LORE.md` | Canon: crew, factions, the Predecessor twist |
| `.claude/rules/` | Path-scoped rules, one per crate |
