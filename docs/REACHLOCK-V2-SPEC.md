# ReachLock v2 — Comprehensive Specification

**Status:** Design Draft · **Date:** 2026-07-10  
**Revision:** 2 (incorporating adversarial review + directive: no monetization, LLM as thinner edge)  
**Stack:** Rust · Bevy · Postgres · Redis  
**Repository:** `chezgoulet/reachlock` (new workspace: `reachlock-v2/`)

---

## Preamble: What Changed from Rev 1

This revision incorporates Data's nine adversarial findings and Christopher's two directives.

**Directives applied:**
1. **Subscription/monetization fully removed.** No billing logic, no tier enforcement, no pricing. The `universe_tier` enum and per-universe seed partitions remain as architectural hooks — zero cost to keep, schema migration cost to add later.
2. **LLM surface shrunk to deterministic-tree leaf nodes.** LLM calls fire only when player-authored rules encounter a situation they cannot resolve. The contract system is the same; the frequency and UX of LLM calls change.

**Adversarial findings addressed:**

| # | Finding | Resolution |
|---|---|---|
| 1 | Seed conflict race condition | Atomic first-write-wins with UNIQUE constraint on `(universe, system_id)`. Client retries on conflict. Visual state shown during conflict resolution (`SYSTEM_RENDERED: ship appears, player sees it`) |
| 2 | Bridge layer thickness understated | Bounded explicitly — conversion layer must be thin relative to the generator output it wraps. Architecture review if any single conversion module grows too large for its concern |
| 3 | Cross-platform determinism fragile | Fixed-point math for ALL gameplay-critical values. No "where needed" loophole. Test harness that compares generator output across x86, ARM, WASM in CI |
| 4 | Server-side contract validation | Client signs contract evaluations with a hash chain. Server verifies signatures for online play. Offline mode uses unsigned local-only evaluations |
| 5 | LLM proxy latency unmodeled | Visual deliberation state added to all LLM calls. "Your crew is considering..." animation. Architecture unchanged; UX contract changed |
| 6 | Universe tick blocks WebSocket handler | Tick loop communicates via async channels. Message routing runs on a separate Tokio task. Tick skips if it takes longer than its interval |
| 7 | Content pipeline gap | **Removed** — follows subscription model |
| 8 | Classic content delay | **Removed** — follows subscription model |
| 9 | WASM build risk | Elevated to spike deliverable #1, not #7. First thing validated: `cargo build --target wasm32-unknown-unknown` with full Bevy plugin stack |

---

## 1. Concept & Philosophy

ReachLock is a procedurally-generated spacefaring MMO where:

- The **universe is generated** from seeds, not stored as assets
- **Player-authored automations** run your ship, with LLMs as fallback when rules hit the unexpected
- **Multiple parallel universes** exist with different inference contracts — fair competition without artificial caps
- **Offline, LAN multiplayer, and online persistent** modes share the same generator and the same seeds
- **Monetization is an afterthought** — the architecture supports it, but no design decisions are made for it

### Design Pillars

| Pillar | Description |
|---|---|
|| **Procedural Everything** | Ships, stations, planets, music, UI — generated from parameters + seed, not hand-authored files |
|| **Seed as Universal Key** | A seed produces the same world everywhere — offline, LAN, online. Server only stores the seed + diffs |
|| **LLM at the Edge, Not the Center** | LLMs fire at deterministic-tree leaf nodes — player-authored rules run first. Latency is deliberation, not lag |
|| **Contract-First Automation** | Players write rules for their crew and ship. The LLM fills gaps when rules can't resolve |
|| **Fail States Are Valid Outcomes** | A badly programmed robot can fail to wake the crew. A misunderstood droid order causes a supply shortage. LLM timeouts strand ships in hyperspace. These are not bugs — they are emergent stories the universe tells |
|| **Human-AI Agency Overlap** | The core question is not "can the AI do it" but "who should decide?" Every situation asks: does the human decide, the AI decide, or do they collaborate? The answer has consequences |
|| **Fair Competition by Universe** | Different inference tiers create separate universes. Players opt into their competition bracket |
|| **Server as Ledger, Not Simulator** | The server records truth (seeds, claims, signed evaluations). Clients run the simulation. Horizontally scalable |
|| **Jump Gates Are Authored, FTL Is Procedural** | Known space near the gate network is curated, scripted, designed. Deep space beyond the last gate is generated, seeded, unexplored. The boundary between them is the frontier |

### Player Modes

| Mode | State | World | Contract Evaluation | LLM |
|---|---|---|---|---|
| Offline | Local only | Your galaxy, your saves | Local (unsigned) | None or BYOK |
| LAN / Peer | Group-shared, ephemeral | Play together, no persistence | Host evaluates, peer verifies hash | Host decides |
| Online | Server-authoritative, persistent | Shared galaxy | Signed evaluations, server verifies | Tiered per universe |

---

## 2. Stack Decisions

### Confirmed Stack

| Layer | Technology | Rationale |
|---|---|---|
| Language | Rust | Performance, safety, WASM compilation, type system for procedural gen |
| Client Framework | Bevy 0.19 | ECS engine with 2D rendering, audio, UI, input, scene system, WASM target |
| Server Runtime | Tokio + Axum | Async Rust standard, WebSocket support, HTTP routing |
| Database | Postgres + JSONB | Seed ledger, player accounts, content overrides. LISTEN/NOTIFY for real-time |
| Cache / PubSub | Redis | Session tokens, live player positions, rate-limit counters |
| WASM Target | `wasm32-unknown-unknown` | Bevy compiles natively, wasm-bindgen for web build |
| Desktop Wrapper | Tauri (optional) | Native distribution via Steam/App Store via the same WASM core |

### Why Not Alternatives

| Rejected Option | Reason |
|---|---|
| TypeScript | Iteration speed is excellent, but Rust's type system, performance ceiling, and WASM story win for a long-term project where we're building everything custom anyway |
| Macroquad | Excellent simplicity, but we'd rebuild audio, UI, scene management, and camera systems that Bevy provides out of the box. Bevy's ceremony is a one-time wrapper cost |
| Unity / Unreal | Asset pipeline fights runtime generation at every turn. 200MB+ runtime is wasteful for a procedural-gen game |
| Godot | We already have 1,787 files of Godot experience. The procedural gen layer must fight Godot's scene-based resource pipeline the same way it fights Bevy, but with a smaller WASM ecosystem |

### WASM Build Risk (Acknowledged)

Bevy plugin combinations — specifically `bevy_rapier2d` + `bevy_prototype_lyon` + `bevy_audio` — have known compatibility gaps on `wasm32-unknown-unknown`. This is the highest-risk item in the entire stack and the **first thing validated in the spike**, not the last. If the full plugin stack fails to compile to WASM, the fallback is: build with a reduced plugin set for WASM (no rapier2d on web), full physics in desktop builds only. The core game loop (generator → renderer → contract engine) has no physics dependency and works identically on both targets.

---

## 3. Workspace Architecture

```
reachlock/
├── Cargo.toml                 # Workspace root
├── reachlock-core/            # Shared library — no rendering deps
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── generator/         # Procedural generation primitives
│       │   ├── mod.rs
│       │   ├── hull.rs        # Ship hull geometry
│       │   ├── station.rs     # Station interior layout
│       │   ├── planet.rs      # Planet surface generation
│       │   ├── music.rs       # Procedural audio generation
│       │   └── ui.rs          # UI widget geometry
│       ├── seed/              # Seed protocol
│       │   ├── mod.rs
│       │   ├── types.rs       # Seed, SystemId, PlayerId, etc.
│       │   └── resolver.rs    # Seed → generator parameters
│       ├── contract/          # LLM contract system
│       │   ├── mod.rs
│       │   ├── engine.rs      # Rules evaluation engine
│       │   ├── types.rs       # Trigger, Rule, Action, LLMConfig
│       │   ├── protocol.rs    # Contract serialization
│       │   └── signature.rs   # Signed evaluation hash chain (online verification)
│       ├── universe/          # Universe definitions
│       │   ├── mod.rs
│       │   ├── tier.rs        # Classic, FairPlay, Spectrum, BYOK (enum only — no billing)
│       │   └── rules.rs       # Per-tier rule enforcement
│       ├── network/           # Protocol types (shared)
│       │   ├── mod.rs
│       │   └── messages.rs    # Client↔Server message types
│       └── util/
│           ├── noise.rs       # Noise function wrappers (fixed-point aware)
│           ├── rng.rs         # Seeded RNG (cross-platform deterministic)
│           └── color.rs       # Color palettes
│
├── reachlock-client/          # Bevy game client
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── bridge/            # Core→Bevy conversion layer (the "wrapper")
│       │   ├── mod.rs
│       │   ├── mesh.rs        # GeneratedMesh → Bevy Mesh + Material
│       │   ├── texture.rs     # GeneratedImage → Bevy Image
│       │   ├── audio.rs       # GeneratedAudio → bevy_audio source
│       │   └── ui.rs          # GeneratedLayout → bevy_ui nodes
│       ├── systems/           # Bevy ECS systems
│       │   ├── mod.rs
│       │   ├── setup.rs       # World initialization
│       │   ├── input.rs       # Keyboard/mouse/touch handling
│       │   ├── ship.rs        # Ship spawning and control
│       │   ├── station.rs     # Station interaction
│       │   ├── navigation.rs  # System travel, jump gates
│       │   ├── contract.rs    # Contract engine integration (+ signing)
│       │   ├── hud.rs         # HUD rendering
│       │   ├── deliberation.rs# LLM deliberation visual state (crew thinking animation)
│       │   └── network.rs     # WebSocket client
│       ├── plugins/           # Bevy plugin registration
│       │   ├── mod.rs
│       │   ├── core_plugin.rs # ReachLock core systems
│       │   ├── render_plugin.rs # Custom render pipeline
│       │   ├── ui_plugin.rs   # UI management
│       │   └── net_plugin.rs  # Network plugin
│       └── states.rs          # AppState enum (Menu, Playing, Docked, etc.)
│
├── reachlock-server/          # WebSocket server + universe tick
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── config.rs          # Server configuration
│       ├── ws/                # WebSocket handler
│       │   ├── mod.rs
│       │   ├── handler.rs     # Accept, route messages
│       │   └── session.rs     # Per-connection state
│       ├── services/
│       │   ├── mod.rs
│       │   ├── auth.rs        # Session token validation
│       │   ├── seed.rs        # Seed ledger CRUD (atomic first-write-wins)
│       │   ├── tick.rs        # Universe tick loop (async channel dispatch)
│       │   ├── llm_proxy.rs   # LLM call routing (per-tier, visual state hooks)
│       │   └── verify.rs      # Signed contract evaluation verification
│       ├── db/
│       │   ├── mod.rs
│       │   ├── models.rs      # sqlx-compatible structs
│       │   └── queries.rs     # Prepared queries
│       └── redis/
│           ├── mod.rs
│           └── pubsub.rs      # Channel management
│
├── reachlock-cli/             # CLI tools (admin, seed testing)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── gen.rs             # Run generator from CLI
│       ├── admin.rs           # Server admin commands
│       └── determinism.rs     # Cross-platform determinism test runner
│
└── docs/
    ├── REACHLOCK-V2-SPEC.md   # This document
    ├── SEED-PROTOCOL.md
    ├── CONTRACT-SYSTEM.md
    ├── SIGNED-EVALUATIONS.md
    └── DETERMINISM.md
```

---

## 4. The Seed Protocol

### Core Concept

A **seed** is a 64-bit integer that acts as the input to a deterministic generator. Every game object — ship, station, planet, system, music track — can be regenerated identically from its seed and parameters.

```
seed = hash(discoverer_id, system_id, object_type, biome, timestamp_rounded)
```

### Seed Ledger (Postgres)

```sql
-- Universe tiers: architectural hook. No billing logic attached.
CREATE TYPE universe_tier AS ENUM ('classic', 'fair_play', 'spectrum', 'byok');

CREATE TABLE seeds (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discoverer_id UUID NOT NULL,
    universe    universe_tier NOT NULL,
    system_id   VARCHAR(64) NOT NULL,
    object_id   VARCHAR(64),       -- nullable = whole system
    seed        BIGINT NOT NULL,
    discovered  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    diffs       JSONB DEFAULT '{}', -- player modifications as deltas

    -- First-write-wins: prevents the simultaneous-discovery race condition
    UNIQUE(universe, system_id, COALESCE(object_id, ''))
);

CREATE INDEX idx_seeds_system ON seeds(universe, system_id);
CREATE INDEX idx_seeds_discoverer ON seeds(discoverer_id, universe);
```

### Discovery Flow (Race-Free)

1. Player enters a new system
2. Client generates a tentative seed from `(player_id, system_id, universe, biome, now_rounded)`
3. Client sends `POST /seed/discover { universe, system_id, seed }`
4. Server attempts `INSERT ... ON CONFLICT (universe, system_id) DO NOTHING`
5. **If inserted (first discoverer):** server returns `{ canonical_seed: seed, diffs: {} }` — client's seed is now canonical
6. **If conflicted (already discovered):** server returns `{ canonical_seed: existing_seed, diffs: existing_diffs }` — client re-renders from canonical seed
7. **Client UX during conflict:** brief visual state "Synchronizing system data..." — the ship simply appears with the canonical seed. No content is re-requested; the generator runs locally from the returned seed.

**Why this fixes the race:** The UNIQUE constraint makes Postgres the atomic arbiter. Both clients' INSERTs arrive at the database in some order — the first wins, the second gets the existing row. No two clients can see conflicting canonical seeds for the same system. The client never sees a "you were wrong, re-render" glitch — it only sees a 1-2 round-trip delay on the second discoverer, during which the system is simply not yet rendered. That delay is indistinguishable from normal network latency for entering a new system.

### Cross-Mode Portability

- Offline mode uses a local-unique ID as discoverer, stored in `bevy_pkv` local saves
- Online mode uses the server's canonical seed
- A player can build a ship in offline mode, bring its seed online, and it exists in the persistent universe
- The same seed produces the same geometry regardless of mode or universe tier — enforced by the determinism test harness

---

## 5. Procedural Generation System

### Generator Architecture

```
Seed + Parameters → Generator Function → GeneratedAsset (pure data)
                                                        ↓
                                              Bridge Layer → Bevy Asset
```

### Generator Primitives

All generators live in `reachlock-core` and produce plain data structures:

| Generator | Input | Output |
|---|---|---|
| `generate_hull` | seed, class, faction, damage | `GeneratedMesh { vertices, indices, uvs, colors }` |
| `generate_station` | seed, type, biome, size | `GeneratedMesh` + `GeneratedLayout { rooms, doors }` |
| `generate_planet` | seed, size, biome | `GeneratedMesh` + `GeneratedTexture { pixels, width, height }` |
| `generate_music` | seed, mood, duration | `GeneratedAudio { samples, sample_rate }` |
| `generate_ui_panel` | seed, panel_type, theme | `GeneratedLayout { elements, positions, sizes }` |

### Override System

When a generator receives `source: "hand_crafted"` or an explicit `asset_id`, it returns the pre-authored asset instead of generating one. This allows curated content to coexist with procedural generation.

```rust
pub enum AssetSource {
    Procedural { seed: u64, params: GenerationParams },
    HandCrafted { asset_id: String },
}
```

### Determinism Guarantee (Hardened)

- All generators are **pure functions** — no randomness, no external state, no time dependence
- All randomness is derived from the seed through a fixed seeded PRNG (`rand::rngs::StdRng` seeded with the 64-bit seed)
- **Fixed-point math for ALL gameplay-critical values.** No "where needed" loophole. Coordinates, distances, speeds, damage values, and any value that affects gameplay output are represented as fixed-point integers. Floating-point is permitted only for visual-only values (color gradients, non-gameplay animation parameters)
- A **determinism test harness** in `reachlock-cli` runs every generator on `x86_64`, `aarch64`, and `wasm32` and compares output bit-for-bit. CI enforces this before any generator change merges
- **Known divergence sources accounted for:** LLVM optimization level, FMA instruction fusion, vectorization width, NaN/inf handling — all eliminated by the fixed-point mandate. Remaining floating-point operations for visual-only paths must be wrapped in `#[cfg_attr(test, ...)]` gates that the test harness can skip

### Bridge Layer Thickness (Bounded)

The bridge layer converts core data types to Bevy assets. Each conversion is a one-time implementation:

| Bridge Module | Complexity | Bevy API Used |
|---|---|---|
| `mesh.rs` | Low — one struct conversion | `Mesh::new()`, `insert_attribute()`, `Indices::U32` |
| `texture.rs` | Low — one struct conversion | `Image::new()`, `TextureFormat::Rgba8UnormSrgb` |
| `audio.rs` | Low-medium — depends on format | `AudioSource`, custom decoder if procedural format is custom |
| `ui.rs` | Medium — node tree construction | `bevy_ui` node construction from layout data |

**Architecture review gate:** If any bridge module becomes complex enough that the conversion is harder to maintain than the generator it wraps, the architecture needs a helper layer. If the total bridge layer complexity across all asset types rivals the generator system itself, the separation is wrong.

---

## 6. The LLM Contract System

### Player-Authored Automation

A contract is a set of rules + LLM fallback authority that controls a ship system or crew member:

```rust
pub struct Contract {
    pub id: String,
    pub label: String,
    pub trigger: Trigger,
    pub rules: Vec<Rule>,
    pub llm_authority: Option<LLMConfig>,
}

pub struct Rule {
    pub condition: Condition,   // Boolean expression over game state
    pub action: Action,         // What to do when condition is true
    pub priority: u8,           // Conflict resolution
}

pub enum Trigger {
    Timer { interval_secs: u32, repeat: bool },
    Event { event_type: String },           // "hostile_detected", "fuel_low", etc.
    StateChange { field: String, op: Comparison, value: i64 }, // fixed-point values
    Manual,                                 // Player-invoked
}

pub struct LLMConfig {
    pub fallback_on_timeout: bool,
    pub timeout_ms: u32,
    pub max_tokens: u32,
    pub system_prompt: String,
}
```

### Example: Boris Pilots During Cryo

```yaml
contract: "cryo-pilot"
label: "Boris takes helm during cryo transit"
trigger:
  event: "crew_cryo_activated"
rules:
  - condition: "distance_to_destination < 500"
    action: "wake_crew"
    priority: 1
  - condition: "fuel < 0.15"
    action: "wake_crew"
    priority: 1
  - condition: "hostile_detected.range < 500"
    action: "wake_crew"
    priority: 10
  - condition: "true"  # default
    action: "maintain_course"
    priority: 0
llm_authority:
  fallback_on_timeout: true
  timeout_ms: 15000
  system_prompt: "You are Boris, a dependable engineer. You are piloting 
    the Loup-Garou while the crew is in cryo. Your rules cover standard 
    situations. If you encounter something your rules don't cover, decide 
    based on: crew safety > ship integrity > mission completion. 
    Describe what you did and why."
```

### How the Engine Evaluates

1. Game state changes trigger contract evaluation
2. Rules are evaluated in priority order — pure computation, no I/O
3. First matching rule fires its action. Action is signed with a hash chain for online verification
4. If no rule matches AND `llm_authority` is `Some`, the engine enters **deliberation state**:
   - Client shows visual indicator: "Boris is thinking..."
   - Client sends LLM call to server proxy (WebSocket message with contract + context)
   - Server routes per tier, returns result
   - Client displays result as crew comm: "Boris: I've assessed the situation..."
5. On timeout/failure, fallback action fires. Player sees log entry: "Boris couldn't decide — fell back to maintenance routine"
6. All decisions are logged with timestamps and context — player reviews in the ship's log UI
7. **Online mode:** Each evaluation step is signed. Server can verify the hash chain to prove the contract evaluated honestly

### LLM Deliberation UX

Every LLM call triggers a visual state. The player sees:

```
[Boris icon]   Boris is considering the situation...
               ─────────────────────────────────────
               "Unknown signal detected while crew 
               is in cryo. My rules don't cover this.
               Consulting ship's AI..." 
               [spinner animation]
```

The deliberation state serves three purposes:
- Masks LLM latency (anywhere from 1-15 seconds depending on tier)
- Frames the pause as *the crew thinking*, not the game lagging
- Gives the player visibility into *why* the LLM was called (the context that no rule covered)

### Client-Side vs Server-Side (Signed Evaluations)

| Component | Location | Online Mode | Rationale |
|---|---|---|---|
| Rules engine | Client (reachlock-core) | Signs each evaluation result | Must work offline. Low latency |
| LLM call dispatch | Client → Server proxy | Proxied through server auth | Server routes per tier, enforces rate limits |
| Contract storage | Local + Server | Backed up to server | Restored on reconnect |
| Evaluation signature | Client | Sent with each action | Server verifies to prevent cheating |
| Decision log | Client + Server | Uploaded after each action | The "captain's log" feature |

**Signed evaluation protocol (online only):**

```
1. Contract engine evaluates rules → produces Action
2. Client hashes (contract_id, tick, action, previous_signature) → signature
3. Client sends { player_id, contract_id, tick, action, signature, prev_signature } to server
4. Server verifies: hash matches, prev_signature matches last known, action is legal in this context
5. Server records action. Rejects if signature chain is broken.
```

For offline mode, no signatures are generated — the evaluation is local-only and trusted by definition.

**Cheating vector eliminated:** A modified client could produce a contract that always fires weapons regardless of rules. The server rejects it because the hash chain doesn't match the canonical contract stored on the server. The player's action is not applied until the server verifies the signature chain.

---

## 7. Multi-Universe Tiers

### Universe Definitions

The multi-universe system exists for **fair competition**, not monetization. Tiers differentiate inference capability so players opt into their preferred competition bracket. No billing logic, no subscription enforcement, no content gating.

| Tier | Inference | LLM Model Cap | Latency | Who Plays |
|---|---|---|---|---|
| **Classic** | None — rules only | N/A | Instant | Purists, offline-first, no API key needed |
| **Fair Play** | Server-side inference | ≤8B params | ≤2s | Balanced competition, phone players |
| **Spectrum** | Cloud inference | Any model (default: current-best open) | 1-4s | Players who want smarter crew |
| **BYOK** | Player-provided API key | Any model they pay for | Varies | Power users testing different models |

### Universe Isolation

- Each universe has its own seed ledger partition (`universe` column on seeds table)
- Players cannot transfer ships or resources between universes
- Universe is selected at character creation (can create alts in different universes)
- Leaderboards are per-universe, always visible
- **No content gating between universes.** All universes receive the same content at the same time. The only difference is inference capability

### LLM Proxy Routing

```rust
async fn route_llm_call(
    player_tier: UniverseTier,
    contract: &Contract,
    context: &GameState,
) -> Result<LLMResponse, LLMError> {
    match player_tier {
        UniverseTier::Classic => Err(LLMError::NoInferenceTier),
        UniverseTier::FairPlay => {
            // Route to server-side 8B model (e.g., Llama 3.2 8B via llama.cpp)
            call_local_model(&contract, context, "llama-3.2-8b").await
        }
        UniverseTier::Spectrum => {
            // Route to best available open model (e.g., DeepSeek V3, Qwen 3.5)
            call_cloud_provider("openrouter", &contract, context).await
        }
        UniverseTier::Byok { api_key } => {
            // Route to player's chosen provider with their key
            call_bring_your_own(&api_key, &contract, context).await
        }
    }
}
```

### Future Hook: Tier Enforcement

The `universe_tier` enum and per-universe seed partitioning are designed so that future tier enforcement requires:
- Adding a `tier` column to the `players` table (currently implicit — derived from universe chosen at character creation)
- Adding a rate-limit middleware on the LLM proxy that checks player tier + usage
- Neither is implemented now. The schema is ready when/if monetization returns.

---

## 8. Server Architecture

### Service Topology

```
                    ┌─────────────┐
                    │   Postgres   │
                    └──────┬──────┘
                           │
    ┌──────────────────────┼──────────────────────┐
    │                      │                      │
┌───▼────┐          ┌─────▼─────┐          ┌─────▼────────┐
│ Seed    │          │  Auth     │          │ Verification │
│ Service │          │  Service  │          │ Service      │
└───┬────┘          └─────┬─────┘          └──────┬───────┘
    │                      │                      │
    └──────────────────────┼──────────────────────┘
                           │
              ┌────────────▼────────────┐
              │    Async Message Bus     │
              │   (tokio::sync::mpsc)    │
              └────────────┬────────────┘
                           │
              ┌────────────▼────────────┐
              │  WebSocket Handler      │◄────► Redis
              │  (Tokio task per conn)  │
              └────────────┬────────────┘
                           │
                    ┌──────▼──────┐
                    │   Clients   │
                    └─────────────┘

┌──────────────────┐     ┌──────────────────┐
│  Universe Tick   │     │  LLM Proxy       │
│  (async loop,    │     │  (per-universe)  │
│   non-blocking)  │     │                  │
└──────┬───────────┘     └──────┬───────────┘
       │                        │
       ▼                        ▼
  Postgres/Redis          Inference APIs
```

### Services Detail

| Service | Responsibility | State | Scaling | Design Note |
|---|---|---|---|---|
| WebSocket Handler | Accept connections, message routing | Per-session in memory | Horizontal (stateless) | One Tokio task per connection. Messages enqueued on async bus |
| Auth Service | Session token validation | Redis cache | Horizontal | Stateless passthrough |
| Seed Service | Seed ledger CRUD, atomic first-write | Postgres | Horizontal | UNIQUE constraint handles race |
| Verification Service | Validate signed evaluation chains | Stateless | Horizontal | Verifies hash chain; no state needed beyond last-known signature from Redis |
| Universe Tick | NPC economy, faction updates, event generation | Postgres + Redis | Single-threaded per universe | Async loop. **Does not block message routing** — communicates via `tokio::sync::mpsc` channel. If tick takes longer than its interval, it skips the next tick instead of queuing |
| LLM Proxy | Route inference calls per tier, rate-limit | Redis counters | Horizontal | Visual deliberation hooks on every call |

### WebSocket Protocol

```
Client → Server:
  { "type": "seed.discover", "system_id": "...", "seed": 12345 }
  { "type": "seed.modify", "system_id": "...", "diffs": { ... } }
  { "type": "contract.sync", "contracts": [ ... ] }
  { "type": "eval.submit", "contract_id": "...", "action": "...", "signature": "..." }
  { "type": "llm.call", "contract_id": "...", "context": { ... } }
  { "type": "player.position", "system_id": "...", "position": [x, y] }

Server → Client:
  { "type": "seed.canonical", "system_id": "...", "seed": 12345, "diffs": { ... } }
  { "type": "eval.verified", "eval_id": "...", "accepted": true }
  { "type": "eval.rejected", "eval_id": "...", "reason": "signature_mismatch" }
  { "type": "llm.deliberating", "call_id": "..." }        // "Boris is thinking..."
  { "type": "llm.response", "call_id": "...", "action": "wake_crew", "reasoning": "..." }
  { "type": "player.entered", "player_id": "...", "system_id": "...", "universe": "..." }
  { "type": "universe.event", "event": { ... } }
```

---

## 9. Client Architecture (Bevy)

### Application States

```rust
#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum AppState {
    Loading,        // Asset loading, generator warmup
    MainMenu,       // Title screen, mode selection
    CharacterSelect,// Choose/create character
    Playing,        // Active game loop
    Docked,         // At station (separate UI mode)
    JumpTransition, // System-to-system travel
    Deliberation,   // LLM is thinking (overlay on Playing)
    Paused,         // Menu overlay
}
```

### Plugin Registration

```rust
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy_prototype_lyon::ShapePlugin)
        .add_plugins(bevy_rapier2d::RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new())
        .add_plugins((
            reachlock_core_plugin,
            reachlock_render_plugin,
            reachlock_ui_plugin,
            reachlock_net_plugin,
            reachlock_contract_plugin,
        ))
        .insert_state(AppState::Loading)
        .init_resource::<DeliberationState>()  // Visual state for LLM calls
        .init_resource::<SignatureChain>()      // Online mode evaluation signatures
        .add_systems(Startup, setup_world)
        .add_systems(Update, (
            input_handler,
            ship_controller,
            contract_evaluator,
            deliberation_renderer,  // Renders "Boris is thinking..." overlay
            signature_collector,    // Hashes and signs evaluation results
            network_sync,
            hud_updater,
        ).run_if(in_state(AppState::Playing)))
        .run();
}
```

### Deliberation State

```rust
#[derive(Resource, Default)]
pub struct DeliberationState {
    pub active_calls: Vec<LLMCallInProgress>,
}

pub struct LLMCallInProgress {
    pub contract_label: String,
    pub crew_member: String,
    pub context_summary: String,
    pub started_at: Instant,
    pub expected_max_ms: u32,
}

// Rendered as an overlay on the game screen:
//   [Boris icon] Boris is considering the situation...
//                "Unknown signal detected. My rules don't cover this."
```

### Bridge Layer (Wrapper Pattern)

The bridge layer converts core data types to Bevy assets. Written once per type, thin by design:

```rust
// reachlock-client/src/bridge/mesh.rs
impl From<GeneratedMesh> for Mesh {
    fn from(gen: GeneratedMesh) -> Self {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            gen.vertices.iter()
                .map(|v| [v.x.to_f32(), v.y.to_f32(), 0.0])
                .collect::<Vec<_>>()
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, gen.uvs);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_COLOR,
            gen.colors.iter().map(|c| [c.r, c.g, c.b, c.a]).collect::<Vec<_>>()
        );
        mesh.insert_indices(Indices::U32(gen.indices));
        mesh
    }
}
```

---

## 10. Authored Content Pipeline

**Status:** First-class system (not an override afterthought)

The authored content pipeline is the mechanism by which hand-crafted, designed content — stations with authored layouts, crew with written backstories, Predecessor ruins with designed puzzles, faction storylines, scripted events — enters the same rendering and distribution system as procedurally generated content.

### Core Principle: The Bridge Doesn't Know the Difference

The bridge layer (`reachlock-client/src/bridge/`) converts data structures — vertices, indices, UVs, colors, layouts, audio samples — into Bevy assets. It does not care whether those data structures came from a generator function or from an authored asset file.

```
Generator                  Authored Content File
     │                              │
     │  GeneratedMesh               │  GeneratedMesh (from deserialized JSON)
     │                              │
     └──────────┬───────────────────┘
                │
       Bridge Layer (identical path)
                │
         Bevy Mesh + Material
```

This means:
- Authored content and generated content share the **exact same rendering pipeline**
- Authored content is validated against the **same schemas** the generator produces
- Authored content gets a **canonical seed** just like generated content (derived from its content_id + system_id)
- Content can be mixed: a hand-crafted station floating in a procedurally-generated system

### Content Types

Each content type has a corresponding data structure in `reachlock-core` and a JSON Schema for validation:

| Content Type | Core Struct | Authoring Format | Schema |
|---|---|---|---|
| Hull | `GeneratedMesh` | RON / JSON / binary | `schemas/hull.schema.json` |
| Station Interior | `GeneratedMesh` + `GeneratedLayout` | RON + room definitions | `schemas/station.schema.json` |
| Planet Surface | `GeneratedMesh` + `GeneratedTexture` | PNG + heightmap | `schemas/planet.schema.json` |
| Crew Soul | `SoulDefinition` | RON (name, portrait_id, voice_params, backstory) | `schemas/soul.schema.json` |
| Dialogue / Contract | `Contract` + triggers | YAML (same format as contracts) | `schemas/contract.schema.json` |
| Location Definition | `Location` | JSON (rooms, NPCs, connections, events) | `schemas/location.schema.json` |
| Predecessor Dungeon | `DungeonLayout` + encounter table | RON (room graph, puzzles, rewards) | `schemas/dungeon.schema.json` |
| Faction Profile | `Faction` | JSON (relations, territory, economy) | `schemas/faction.schema.json` |
| Event Script | `ScriptedEvent` | RON or Lua (if we want scripting) | `schemas/event.schema.json` |

### Content Format

Authored content files live in `reachlock/content/` as `.ron` (Rusty Object Notation), JSON, or YAML files. They are **not compiled into the binary** — they are served by the server's content service and cached locally.

```ron
// reachlock/content/stations/sorrow_station.ron
(
    id: "sorrow_station",
    display_name: "Sorrow Station",
    asset_type: "station",
    seed: 0x4A7B3C2D,              // canonical seed — must match client generation for same ID
    universe: "all",                 // appears in all universe tiers
    priority: authoritative,         // always renders this version, never procedural
    
    mesh: GeneratedMesh(
        vertices: [
            // Hand-authored vertex data — same format generator produces
            Vec3 { x: -128.0, y: 0.0, z: 0.0 },
            Vec3 { x: 128.0, y: 0.0, z: 0.0 },
            // ...
        ],
        indices: [0, 1, 2, 0, 2, 3],
        uvs: [Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 1.0, y: 0.0 }],
        colors: [ColorRgba { r: 0.3, g: 0.5, b: 0.8, a: 1.0 }],
    ),
    
    layout: GeneratedLayout(
        rooms: [
            Room { id: "hangar", position: (0, 0), size: (256, 128), connectors: ["main_hall"] },
            Room { id: "main_hall", position: (0, 128), size: (512, 64), connectors: ["bar", "quarters", "shipyard"] },
            Room { id: "bar", position: (-128, 192), size: (128, 64), connectors: ["main_hall"], npcs: ["doss", "grissom"] },
            Room { id: "shipyard", position: (256, 192), size: (128, 64), connectors: ["main_hall"] },
        ],
        npc_spawns: [
            NpcSpawn { npc_id: "doss", position: (10, 20), dialogue_tree: "doss_bar_intro" },
            NpcSpawn { npc_id: "grissom", position: (30, 15), dialogue_tree: "grissom_repair_offer" },
        ],
    ),
    
    contracts: [
        Contract("cryo-pilot"),    // References contract by ID from contracts table
        Contract("doss_deal"),     // Player can author their own, or authored contracts ship with content
    ],
)
```

### Priority System

The `content_overrides` table has a `priority` column that determines which version of an asset renders when multiple sources exist. Priority is evaluated in this order:

| Priority Level | Value | Meaning |
|---|---|---|
| `authoritative` | 100 | **Always renders.** Canonical hand-crafted content (story stations, Predecessor ruins, faction capitals). Players in all universes see this version. Overrides procedural generation unconditionally |
| `curated` | 50 | **Prefer this version.** Hand-crafted content that should render but can be replaced by procedural generation if the content service is unreachable. Falls back gracefully |
| `event` | 75 | **Temporary authoritative.** Event content (seasonal, story arc) that replaces the standard version for a limited time. Has an `expires_at` column. After expiry, falls back to authoritative or procedural |
| `procedural` | 0 | **Default.** No authored content exists. The generator produces this asset from seed + parameters |

```sql
-- Revised content_overrides with priority
CREATE TABLE content_overrides (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id     VARCHAR(64) NOT NULL,
    object_id     VARCHAR(64),
    universe      universe_tier,          -- NULL = all universes
    asset_type    VARCHAR(32) NOT NULL,
    seed          BIGINT NOT NULL,          -- canonical seed, required
    priority      SMALLINT NOT NULL DEFAULT 50,  -- 0=procedural, 50=curated, 75=event, 100=authoritative
    expires_at    TIMESTAMPTZ,             -- NULL = permanent, set for event content
    content       JSONB NOT NULL,           -- the full authored asset data
    available_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(system_id, object_id, asset_type, COALESCE(universe, 'all'))
);
CREATE INDEX idx_overrides_system ON content_overrides(system_id, universe);
CREATE INDEX idx_overrides_active ON content_overrides(universe, asset_type) 
    WHERE available_at <= NOW() AND (expires_at IS NULL OR expires_at > NOW());
```

### Authoring Pipeline

The authoring pipeline converts creative work into ReachLock content files:

```
Tool → Content File → CLI Validation → Server Import → Client Download
```

**Stage 1: Authoring Tools**

Content is authored in any tool that can produce the target format:

| Content Type | Authoring Tool | Output |
|---|---|---|
| Station / Ship geometry | Blender + custom export script | `.ron` (GeneratedMesh) |
| Planet surfaces | GIMP / Aseprite → heightmap | PNG + palette JSON |
| Crew souls | Text editor | `.ron` (SoulDefinition) |
| Dialogues / contracts | Text editor / YAML | `.ron` / `.yaml` |
| Locations / dungeons | Text editor or Tiled | `.ron` (location schema) |
| Faction profiles | Text editor | JSON |
| Event scripts | Text editor | RON / Lua |

**Stage 2: CLI Validation**

```bash
reachlock-cli content validate path/to/sorrow_station.ron
# Output: Validates against schema, checks seed uniqueness, previews mesh in terminal

reachlock-cli content preview path/to/sorrow_station.ron
# Output: Launches Bevy window showing the authored asset with no server connection
```

Validation checks:
- Schema validity (JSON Schema against `schemas/*.schema.json`)
- Seed uniqueness (no existing content with same seed in the `content_overrides` table)
- Mesh integrity (no degenerate triangles, UVs in 0-1 range, vertex count within limits)
- Layout connectivity (all door connectors reference valid rooms)
- NPC references (all npc_ids exist in souls table)

**Stage 3: Server Import**

```bash
reachlock-cli content publish path/to/sorrow_station.ron \
    --universe all \
    --priority authoritative \
    --available-at 2026-08-01T00:00:00Z
```

This:
1. Validates the content file
2. Inserts into `content_overrides` (upserts on seed conflict)
3. Broadcasts a `content.update` message to connected clients via WebSocket
4. Logs the deployment in a `content_deployments` table for rollback

**Stage 4: Client Distribution**

When a player enters a system:
1. Client checks local cache for content overrides (`bevy_pkv`)
2. If cached and not expired, renders from cache (no network call)
3. If not cached, requests `GET /content/system/{system_id}` from server
4. Server returns all overrides for that system
5. Client caches them and renders through the same bridge layer

### Seed Integration

Authored content receives a canonical seed just like generated content. The seed is derived from:

```
seed = hash("content_override", system_id, object_id)
```

This ensures:
- The seed is deterministic and reproducible
- Every player who visits the system sees the same authored content
- The authored content has a first-write-wins claim on the seed ledger, just like a generated discovery
- If the content override is later removed, the seed ledger still records it, and players who saw the authored version can reference it

### Content Lifecycle

```
Author → Draft → Validate → Preview → Publish → Active → Expire/Archive
                 │                          │            │
                 │                    Content available   │
                 │                    on server, pushed   │
                 │                    to connected        │
                 │                    clients             │
                 │                                        │
            Schema checks,                         Event content expires,
            seed uniqueness,                       permanent content lives
            mesh integrity                         until replaced
```

- **Draft:** Local file on author's machine. Not visible to any player
- **Validate:** Schema + integrity checks pass. Ready for preview
- **Preview:** Author can see the asset in-game on their local client, no server needed
- **Publish:** Content pushed to server. Online players see it (either immediately via WebSocket push or on next system entry)
- **Active:** Content renders for players. Checked against `available_at` and `expires_at`
- **Expire:** Event content auto-removes. Archived in a history table

### Mixing Authored and Generated Content

The game world can mix both freely:

| Scenario | System | Station | Ships | Works? |
|---|---|---|---|---|
| Purely generated | Seed A | procedural | procedural | ✅ Default |
| Authored station in generated system | Seed A | Sorrow Station (auth) | procedural | ✅ Mixed |
| Authored system (everything hand-crafted) | Seed A - all content IDs | authored | authored | ✅ Content override at system level |
| Authored system with generated fill | Seed A - station authored, rest procedural | authored | procedural | ✅ Partial override |
| Event replaces station temporarily | Seed A - Sorrow overridden by event version | event version | procedural | ✅ Event priority 75 |

The rule: **content_overrides is a sparse table.** Most systems have no entries. When a system has entries, only the specified object_ids are replaced — everything else generates normally.

### SPIKE Integration

The spike scope (Section 11) already includes P1 deliverable "One override: hand-crafted ship replaces generated one by ID." This validates:
- The override data flows from file → server → client → bridge → Bevy
- The priority system works (override beats procedural)
- The cache layer works (client doesn't re-request on subsequent visits)

After the spike, adding new authored content is just: write a content file, validate, publish.

### File Organization in Workspace

```
reachlock/
├── content/                          # Authored content files
│   ├── stations/
│   │   ├── sorrow_station.ron
│   │   ├── charlevoix_field.ron
│   │   └── ledger_drift_mining.ron
│   ├── souls/
│   │   ├── boris.ron
│   │   ├── tib.ron
│   │   ├── tove.ron
│   │   └── doss.ron
│   ├── dungeons/
│   │   └── predecessor_vault_alpha.ron
│   ├── factions/
│   │   ├── corporate_charter.json
│   │   ├── reach_remnant.json
│   │   └── earth_remnant.json
│   ├── events/
│   │   └── pirate_week_2026.ron
│   └── schemas/                     # JSON Schemas for validation
│       ├── hull.schema.json
│       ├── station.schema.json
│       ├── soul.schema.json
│       ├── dungeon.schema.json
│       └── event.schema.json
│
└── reachlock-cli/
    └── src/
        ├── content_validate.rs      # Schema + integrity validation
        ├── content_preview.rs       # Preview in Bevy window
        └── content_publish.rs       # Upload to server
```

---

## 11. Database Schema (Revised)

```sql
-- Universe tiers: architectural hook only. No billing, no subscriptions.
CREATE TYPE universe_tier AS ENUM ('classic', 'fair_play', 'spectrum', 'byok');

-- Players
CREATE TABLE players (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username        VARCHAR(32) UNIQUE NOT NULL,
    display_name    VARCHAR(64),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login      TIMESTAMPTZ
);
-- Note: No tier column on players table yet. Tier is implicit from
-- the universe column on their characters. When monetization returns,
-- add: tier universe_tier NOT NULL DEFAULT 'classic'

-- Characters (one player can have alts in different universes)
CREATE TABLE characters (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   UUID NOT NULL REFERENCES players(id),
    name        VARCHAR(64) NOT NULL,
    universe    universe_tier NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed ledger (first-write-wins)
CREATE TABLE seeds (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discoverer_id   UUID NOT NULL,
    universe        universe_tier NOT NULL,
    system_id       VARCHAR(64) NOT NULL,
    object_id       VARCHAR(64),       -- nullable = whole system
    seed            BIGINT NOT NULL,
    discovered      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    diffs           JSONB DEFAULT '{}',

    -- Atomic first-write-wins: prevents simultaneous-discovery race
    UNIQUE(universe, system_id, COALESCE(object_id, ''))
);

-- Contract evaluation signatures (online mode audit trail)
CREATE TABLE eval_signatures (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    character_id    UUID NOT NULL REFERENCES characters(id),
    contract_id     VARCHAR(64) NOT NULL,
    tick            BIGINT NOT NULL,
    action          JSONB NOT NULL,
    signature       VARCHAR(128) NOT NULL,
    prev_signature  VARCHAR(128),
    verified        BOOLEAN NOT NULL DEFAULT false,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_signatures_chain ON eval_signatures(character_id, contract_id, tick);

-- Universe events
CREATE TABLE universe_events (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    universe    universe_tier NOT NULL,
    event_type  VARCHAR(64) NOT NULL,
    payload     JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BYOK provider keys (encrypted)
CREATE TABLE byok_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id   UUID NOT NULL REFERENCES players(id),
    provider    VARCHAR(64) NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    is_active   BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Content overrides (hand-crafted assets, revised with priority)
CREATE TABLE content_overrides (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id     VARCHAR(64) NOT NULL,
    object_id     VARCHAR(64),
    universe      universe_tier,          -- NULL = all universes
    asset_type    VARCHAR(32) NOT NULL,
    seed          BIGINT NOT NULL,          -- canonical seed
    priority      SMALLINT NOT NULL DEFAULT 50,  -- 0=procedural, 50=curated, 75=event, 100=authoritative
    expires_at    TIMESTAMPTZ,             -- NULL = permanent
    content       JSONB NOT NULL,           -- the full authored asset data
    available_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(system_id, object_id, asset_type, COALESCE(universe, 'all'))
);
CREATE INDEX idx_overrides_active ON content_overrides(universe, asset_type) 
    WHERE available_at <= NOW() AND (expires_at IS NULL OR expires_at > NOW());

-- Player contracts (server-side backup)
CREATE TABLE contracts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    character_id UUID NOT NULL REFERENCES characters(id),
    label       VARCHAR(128),
    contract    JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Content deployment history
CREATE TABLE content_deployments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    override_id UUID NOT NULL REFERENCES content_overrides(id),
    deployed_by VARCHAR(64),
    version     INTEGER NOT NULL DEFAULT 1,
    checksum    VARCHAR(64) NOT NULL,
    rollback_to UUID REFERENCES content_deployments(id),
    deployed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 12. Spike Scope (Week 1, Revised)

### Goal
Validate the highest-risk items first: WASM compilation, generator determinism, seed protocol, and the authored content pipeline end-to-end.

### Deliverables (Priority Order)

| Priority | Deliverable | Risk Addressed | Verification |
|---|---|---|---|
| **P0** | WASM Build: `cargo build --target wasm32-unknown-unknown` with full Bevy plugin stack | #9 — Plugin compatibility on WASM | CI passes. If rapier2d fails, switch to reduced-plugin WASM config |
| **P0** | Determinism test harness: same generator output on x86, ARM, WASM | #3 — Cross-platform determinism | CLI test compares output bit-for-bit across targets |
| **P1** | Cargo workspace with `reachlock-core`, `reachlock-client`, `reachlock-server` | Foundation | `cargo build` succeeds |
| **P1** | Seed protocol: generator takes seed + hull class + faction, renders ship via Bevy + Lyon | Core | Ship appears on screen, matches between native and WASM |
| **P1** | Seed ledger: Postgres table + `POST /seed/discover` with atomic first-write-wins | #1 — Race condition fix | Two concurrent INSERTs: second one gets existing row, not error |
| **P1** | One override: hand-crafted ship replaces generated one by ID | Override pattern | Hand-crafted asset renders instead of generated |
| **P1** | Content validation CLI: `content validate` passes for a hand-authored `.ron` file | Authored pipeline | CLI exits 0, reports schema and integrity checks passed |
| **P2** | One contract: "Boris pilots during cryo" with rules only (no LLM) | #4 — Contract engine | Rules evaluate, actions fire, signature chain generated |
| **P2** | WebSocket: client sends seed discover, server records, client re-renders on reconnect | Network | Server restart → client reconnects → system renders identical |
| **P2** | Deliberation visual state mock: "Boris is thinking..." overlay without actual LLM call | #5 — Latency UX | Visual state appears, player sees it as deliberation, not lag |

### Risk-Based Reordering Rationale

The original spec listed WASM build as deliverable #7. That was wrong. If WASM fails, the entire web distribution story collapses. It must be the first thing validated. Similarly, determinism is not a "nice to have" — if the generators produce different output on different platforms, the seed-as-universal-key guarantee is broken from day one. The test harness must exist before any generator is committed.

The authored content pipeline is P1, not P2, because Christopher explicitly wants authored content as a first-class system from the start. The CLI validation tool proves the pipeline works without needing a server.

### Non-Goals (Week 1)

- No actual LLM integration (mock only)
- No universe tick
- No multi-universe tier system (single universe for spike)
- No auth beyond session tokens
- No physics in WASM build (rapier2d excluded if it blocks compilation)
- No audio
- No UI beyond bare minimum
- No content authoring GUI tools (hand-written `.ron` files are the authoring format for the spike)

---

## 13. Development Principles (Revised)

| Principle | Rule |
|---|---|
| **Core is pure** | `reachlock-core` has zero rendering, zero I/O dependencies. Testable in isolation. Compiles to WASM independently |
| **Bridge is bounded** | The wrapper layer is thin by design — conversion logic only, no game logic. Architecture review if any module's complexity rivals the generator it wraps |
| **Seed determinism is absolute** | Fixed-point for all gameplay-critical values. Test harness compares output bit-for-bit across x86, ARM, WASM in CI |
| **LLM is a mechanic, not a feature** | Every LLM call has a visual deliberation state. No silent LLM calls. The player always knows the crew is thinking |
| **Offline is first-class** | The game must be fully playable without a server connection. Online features add, never replace |
| **Contracts are data, signed for trust** | Contracts are serializable, shareable, and versioned. Online mode signs every evaluation. Offline mode skips signing |
| **One wrapper per type** | The bridge pattern maps core types to Bevy types. Write the conversion once, generate forever |
| **Race conditions are schema problems** | Use Postgres UNIQUE constraints as the atomic arbiter. Never use application-level locks for seed conflicts |
| **Monetization is an afterthought** | Build the mechanics. Schema hooks for future tier enforcement are free. Everything else waits |

---

## 14. Three-Mode Gameplay

**Relationship to V2 infrastructure:** This section defines the player experience across three interconnected modes. Each mode builds on the Bevy client architecture (§9), the procedural generation system (§5), the LLM contract system (§6), the authored content pipeline (§10), and the seed protocol (§4). The infrastructure described in earlier sections provides the foundation; this section describes how the player interacts with it.

### Design Philosophy

ReachLock is three games in one engine. Every mode feeds into every other. What you do in Landed mode determines your ship's capability. Your ship determines where you can go in Space Flight. Space determines what you bring back to Landed. And the galaxy keeps turning regardless.

The three modes are not separate games bolted together. They share the same data, the same economy, the same crew, the same persistent universe state. Mode transitions are seamless within the Bevy ECS — the rendering pipeline shifts, but the underlying entities and components remain.

### Mode States (Bevy ECS)

```rust
#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum GameMode {
    // Menu states
    MainMenu,
    CharacterCreate,
    
    // Landed — top-down/isometric on stations and planets
    Landed { location_id: String },
    
    // On-Board — side-on/isometric ship interior
    OnBoard { is_docked: bool },  // docked at station or in-flight
    
    // Space Flight — third-person/cockpit space view  
    SpaceFlight,

    // Transitions
    Docking { system_id: String, station_id: String },
    Undocking,
    
    // Special states
    Hyperspace,         // jump gate transit sequence
    Emergency,          // combat, boarding, crisis
    Deliberation,       // LLM thinking overlay (reserved)
    Paused,
}
```

### Mode 1: Landed — Stardew × Zelda × Pokémon

**When it activates:** Player docks at a station or lands on a planet. Ship is parked. Crew can disembark.

**Camera:** Top-down or isometric 2D. Station interiors, planet surfaces, ruin interiors all use the same camera system with configurable zoom.

**Authoring:** Every station, planet, and ruin is an authored content file (§10). The generator produces terrain and filler buildings; authored content overrides define unique locations, NPCs, and quest hooks. The `GeneratedLayout` struct in `reachlock-core` defines room geometry; authored `.ron` files populate it with NPC spawns, interactive objects, and story triggers.

**Core loop:**

1. **Arrive.** Docking cutscene (Bevy camera transition from space to interior). Crew assigned to stations by their contracts. You walk off the ship into the station concourse.

2. **Explore.** Top-down movement. Interact with NPCs, read signs, find hidden areas. The station layout is generated from seed or loaded from authored content. Each station has unique rooms: markets, bars, repair bays, admin offices, hidden compartments.

3. **Economy.** Buy, sell, invest. Supply and demand are driven by the universe tick (§8). Station prices reflect faction control, local production, and trade route status. Rich biomes produce luxury goods. Blockaded stations pay premium for contraband.

4. **Forage & Gather.** Planet surfaces have harvestable resources. Biome-specific plants, minerals, salvage. These feed into crafting and the ship's supply chain.

5. **Craft & Build.** Using harvested materials and purchased components. Craft consumables (medkits, repair kits, ammunition) and ship components (upgraded sensors, weapons, engines). Crafting recipes are authored content in `content/schemas/`.

6. **Socialize.** NPCs have soul files. Their dialogue is driven by emotional state, personal goals, faction reputation, and history with the player. The LLM contract system fires at the edges — when an NPC reacts to something unexpected, the deliberation state shows them considering the player's words.

7. **Recruit.** Find recruitable NPCs with unique soul files. Earn their trust through missions, conversation, shared combat. Each recruit adds a soul file to the crew roster and a contract to the ship's automation system.

8. **Dungeon.** Predecessor ruins are authored spaces with puzzles, combat, environmental hazards, a unique tool or key item, and a boss encounter. These are designed, not generated. The content override system (§10) makes them authoritative — every player sees the same ruin layout. Tools found in ruins unlock access to other ruins.

**Integration with V2 infrastructure:**

| GDD Feature | V2 Mechanism |
|---|---|
| Station layout | `GeneratedLayout` from core + authored `content/stations/*.ron` overrides |
| NPC dialogue | Soul file (§15) + `Contract` engine with LLM fallback at deterministic tree edges |
| Economy interaction | `POST /economy/price` → universe tick updates supply/demand tables |
| Foraging resources | `GeneratedResource` from seed + biome parameters |
| Crafting recipes | Authored content in `content/schemas/crafting.schema.json` |
| Predecessor dungeons | Authoritative `content/dungeons/*.ron` with override priority 100 |

### Mode 2: On-Board — FTL × Trust

**When it activates:** Player is aboard their ship, either docked at a station or in flight. Accessed via a mode switch from Landed (walk to your ship and board) or during Space Flight (reorient camera to interior).

**Camera:** Side-on cross-section or isometric view of the ship interior. The camera follows the player character but can be scrolled to view the full ship layout.

**Authoring:** Ship interiors are **not authored** — they are generated from the player's hull frame and room placement. The `GeneratedLayout` from core defines the room grid. The player places and reconfigures rooms during docked refits. Authoring applies to *components* (pre-defined room templates, furniture, system modules) which are authored content files.

**Core loop:**

1. **Walk the ship.** Full interior traversal. The ship is a physical space with corridors, rooms, and systems. Walk to engineering, the cockpit, crew quarters, the cargo hold.

2. **Crew management.** Each crew member occupies a position on the ship. You see them in the corridor, in the galley, at their station. Issue orders: go here, repair this, talk to that person. Crew execute in real time, guided by their contracts (§6).

3. **System interaction.** Physical consoles for ship systems: navigation, weapons, shields, power distribution. Walk to a console to operate it. Crew can operate systems independently if their contract authorizes it.

4. **Social observation.** Crew relationships manifest spatially. Two crew who don't get along avoid sharing a room. Close crew gravitate toward each other during off-hours. The ship's social fabric is visible in who stands where and who talks to whom.

5. **Crisis response.** Fires, hull breaches, boarding actions. These are real-time events that require physical navigation to address. A fire in cargo requires someone to reach the cargo hold with a fire extinguisher. A hull breach in crew quarters requires someone to reach the breach with a repair kit and seal the section.

6. **Ship log.** Review LLM decision logs from the contract system (§6). See what your crew decided while you were away. Read the deliberation reasoning.

**Integration with V2 infrastructure:**

| GDD Feature | V2 Mechanism |
|---|---|
| Ship interior layout | `GeneratedLayout` from hull frame + player room placement |
| Room templates | Authored `content/hulls/room_templates.ron` |
| Crew spatial behavior | `Contract` engine evaluates crew positions each tick |
| System consoles | Bevy UI nodes mapped to ship system entities |
| Crisis events | Event trigger → contract evaluation → deliberation if LLM needed |
| Ship log | Contract evaluation signatures stored in `eval_signatures` table |

### Mode 3: Space Flight — Star Fox 64

**When it activates:** Player undocks from a station or launches from a planet surface. Ship is in a system's space volume.

**Camera:** Third-person chase-cam or cockpit view. The camera follows the ship with configurable distance. Mode switch from On-Board by walking to the cockpit and taking the helm.

**Generation:** Space environments are procedurally generated from the system seed. Star field, planet positions, asteroid fields, station positions, jump gate location. Authored content overrides specific celestial bodies (a hand-crafted station replaces a generated one).

**Core loop:**

1. **Fly.** Six-degree-of-freedom flight with cinematic feel. Roll, pitch, yaw, thrust, brake, boost. Different hulls handle differently based on their `ship_handling` parameters in core.

2. **Navigate.** System map shows jump points, stations, planets, points of interest. Fly toward them or use autopilot (if a crew contract allows it). Approach jump gate for system transit.

3. **Combat.** Dogfighting with hostile ships. Weapons fire from hardpoints placed on the hull (§15 Ship Editor). Power management: allocate energy to weapons, shields, or engines in real time. Subsystem targeting on enemies.

4. **Trade.** Meet cargo ships in transit. Haul goods between stations. Use the ship's cargo capacity for arbitrage between systems.

5. **Scan.** Use sensors to identify contacts, discover hidden caches, analyze celestial bodies. Sensor range and fidelity depend on equipped components.

6. **Jump.** Transit through a jump gate to an adjacent system. Brief hyperspace sequence — crew enters cryo, a droid pilots the transit. The contract system handles the droid's decisions during the jump.

7. **Emergency self-jump.** If no gate is available, trigger the ship's jump drive. Higher risk — drive malfunction can cause mission failure, crew injury, or loss of ship.

**Integration with V2 infrastructure:**

| GDD Feature | V2 Mechanism |
|---|---|
| System space generation | `generate_system(seed)` produces starfield, planets, stations, asteroids |
| Ship handling | Hull parameters in core: mass, thrust, turn rate, drift |
| Weapon hardpoints | Player-defined slots from ship editor |
| Jump gate transit | Seed protocol records system entry; contract engine handles cryo pilot |
| Sensors | Equipped component data in ship state |
| Enemy AI | Behavior tree in core (outside contract system — enemies aren't crew) |

### Mode Transitions

Transitions are the hardest part architecturally. Each mode switch requires the rendering pipeline to shift while preserving game state.

```
Space Flight ──dock──▶ Landed      
    │                      │
    │ (walk to cockpit)    │ (walk to ship)
    ▼                      ▼
  On-Board ◀──────────────┘
```

- **Space → Dock:** Ship approaches station → proximity check passes → camera transitions from chase-cam to interior establishing shot → player character appears on station concourse → GameMode set to `Landed { location_id }`. The ship entity persists in the background; crew contracts continue evaluating.
- **Dock → Undock:** Player walks to ship → "Launch" interaction → camera transitions to space flight → GameMode set to `SpaceFlight`. The station interior unloads; the system space loads from seed.
- **Landed → On-Board (docked):** Player walks from station interior to ship airlock → mode switch to side-on ship interior → GameMode set to `OnBoard { is_docked: true }`. Station remains in memory; player can walk back.
- **On-Board → Space Flight:** Player walks to cockpit → "Take Helm" interaction → camera transitions to chase-cam → GameMode set to `SpaceFlight`. Ship interior unloads.
- **Space Flight → On-Board (in flight):** Player walks from cockpit to ship interior → camera transitions to side-on → GameMode set to `OnBoard { is_docked: false }`. Ship continues flying under autopilot or crew contract.

All transitions are handled by Bevy's state machine (`app_state.rs`). The bridge layer ensures the rendering pipeline switches correctly while game entities remain in the ECS world.

---

## 15. NPC Soul System

**Relationship to V2 infrastructure:** The NPC soul system is the narrative engine that connects the LLM contract system (§6) to authored characters. Souls are data, not live LLM connections — they define *who an NPC is*, not *how they respond*. The contract system handles the how; the soul system handles the what.

### Core Concept

Every significant NPC has a **soul file** — a data structure that defines their identity, personality, memory, relationships, goals, and emotional state. The soul file is authored content (`.ron` in `content/souls/`). It does not contain dialogue text — it contains the *parameters* that drive dialogue generation.

The contract system (§6) evaluates what an NPC does. The soul system defines *who they are while doing it*.

### Soul File Structure

```rust
// reachlock-core/src/soul/types.rs

pub struct SoulFile {
    pub id: String,
    pub name: String,
    pub species: Species,           // Human, Droid, Robot
    pub portrait_id: String,       // References generated or authored portrait asset
    
    // Identity
    pub identity: Identity,
    pub personality: Personality,
    
    // State
    pub emotional_state: EmotionalState,
    pub physical_state: PhysicalState,
    
    // Memory
    pub memory_tree: Vec<Memory>,
    pub relationship_graph: Vec<Relationship>,
    
    // Agency
    pub goals: Vec<Goal>,
    pub allegiances: Vec<Allegiance>,
    pub breaking_points: Vec<BreakingPoint>,
    
    // Contract integration
    pub contracts: Vec<String>,     // Contract IDs this soul can execute
    
    // Lore
    pub backstory: String,         // Narrative reference, not LLM prompt
    pub secrets: Vec<Secret>,      // Hidden from player until revealed
}

pub struct Identity {
    pub origin: String,             // "Earth resistance", "Compact military", "ISC trader"
    pub faction_affiliation: String,
    pub role: String,               // Engineer, pilot, medic, captain
    pub public_bio: String,         // What they'll tell you
}

pub struct Personality {
    pub traits: Vec<Trait>,         // Brave, Cautious, Curious, Loyal, Greedy...
    pub values: Vec<Value>,         // Freedom, Profit, Knowledge, Survival...
    pub speaking_style: SpeakingStyle,  // Terse, Elaborate, Technical, Lyrical, Sarcastic
    pub quirks: Vec<String>,        // "Hums while working", "Refuses to discuss Earth"
}

pub struct EmotionalState {
    pub dominant_mood: Mood,        // Happy, Tense, Grieving, Suspicious, Grateful
    pub intensity: f32,             // 0.0 (calm) to 1.0 (overwhelming)
    pub triggers: Vec<Trigger>,     // Conditions that shift mood
    pub history: Vec<MoodShift>,    // Log of emotional changes
}

pub struct Memory {
    pub id: String,
    pub event_type: String,         // "conversation", "combat", "trade", "betrayal", "rescue"
    pub player_involved: bool,
    pub emotional_weight: f32,      // 0.0 (forgettable) to 1.0 (traumatic/formative)
    pub timestamp: GameTick,
    pub summary: String,            // For LLM context assembly
}

pub struct Relationship {
    pub target_id: String,          // Player or another NPC
    pub trust: f32,                 // -1.0 (enemy) to 1.0 (unquestioning)
    pub familiarity: f32,           // 0.0 (stranger) to 1.0 (intimate)
    pub history: Vec<String>,       // Key event IDs that shaped this relationship
}
```

### Soul Authoring Example

```ron
// content/souls/boris.ron
(
    id: "boris",
    name: "Boris",
    species: Human,
    portrait_id: "portraits/boris",
    
    identity: (
        origin: "Earth — Compact industrial sector",
        faction_affiliation: "Unaligned (former Compact engineer)",
        role: "Ship Engineer",
        public_bio: "Boris keeps the Loup-Garou running. He doesn't talk much about Earth.",
    ),
    
    personality: (
        traits: [Dependable, Quiet, Protective, TechnicallyObsessive],
        values: [CrewSafety, ShipIntegrity, HonestWork, DontAskAboutEarth],
        speaking_style: Terse,
        quirks: ["Checks engine seals twice before every jump", 
                 "Refuses all questions about the mark on his forearm"],
    ),
    
    emotional_state: (
        dominant_mood: Stable,
        intensity: 0.3,
        triggers: [
            Trigger { condition: "ship_damage > 0.3", mood: Anxious, priority: 5 },
            Trigger { condition: "crew_member_injured", mood: Protective, priority: 8 },
            Trigger { condition: "asked_about_mark", mood: Defensive, priority: 10 },
        ],
    ),
    
    memory_tree: [],
    relationship_graph: [],
    
    goals: [
        Goal { id: "keep_ship_running", priority: Constant, description: "Loup-Garou never below 70% operational" },
        Goal { id: "protect_crew", priority: Situation, description: "Crew members don't die on my watch" },
        Goal { id: "avoid_earth_past", priority: Constant, description: "Never talk about what happened" },
    ],
    
    allegiance: [Allegiance { faction: "crew", loyalty: 0.9, last_updated: GameTick(0) }],
    breaking_points: [
        BreakingPoint { trigger: "captain_abandons_crew", reaction: LeaveShip },
        BreakingPoint { trigger: "compact_found_him", reaction: BetrayToProtectShip },
    ],
    
    contracts: ["cryo-pilot", "engine_maintenance", "emergency_repair"],
    backstory: "Boris was a Compact military engineer during the Earth uprising...",
    secrets: [
        Secret { id: "the_mark", reveal_condition: "trust > 0.8 AND asked_about_mark", content: "..." },
    ],
)
```

### Soul → Contract Bridge

The NPC soul system does not generate dialogue. It feeds data to the contract system, which determines action:

```
Event → Soul file loaded → Emotional state updated → 
    Goals evaluated → Contract triggered (if applicable) → 
    Rules evaluated → LLM only if rules don't cover → 
    Action executed → Memory recorded → 
    Relationship graph updated
```

For example, when the player talks to Boris:
1. Soul file `boris.ron` loaded — current emotional state is `Stable`
2. Player asks about the mark on his forearm
3. Trigger `asked_about_mark` fires → emotional state shifts to `Defensive`, intensity 0.7
4. Contract "conversation_boris" evaluates: 
   - Rule 1: if `emotional_state == Defensive` → action: `deflect_conversation`
   - LLM authority: true (edge case — player insists)
5. If player insists, LLM fires in deliberation state: "Boris is visibly uncomfortable..."
6. Memory recorded: player asked about mark, Boris deflected
7. Relationship `trust` decreased by 0.05

### Soul Mutations

Authored narrative events can mutate a soul permanently. These are defined in content files alongside storylines:

```ron
// content/storylines/boris_arc.ron
Mutation {
    trigger: "player_showed_trust_during_crisis",
    soul_id: "boris",
    changes: [
        AddTrait("Devoted"),
        RemoveTrait("Guarded"),
        SetRelationship { target: "player", trust: 0.8, familiarity: 0.6 },
        UnlockSecret("the_mark"),
        AddGoal("protect_player_specifically"),
    ],
}
```

Soul mutations are authored content, keyed to story events. They are the mechanism by which the written narrative intersects with the procedural world. The mutation system reads from the `eval_signatures` table — when the server detects that a player has accumulated enough trust-buiding events with Boris, it queues the mutation.

### Integration with V2 Infrastructure

| Soul System Component | V2 Mechanism |
|---|---|
| Soul file storage | Authored `content/souls/*.ron`, loaded via content pipeline (§10) |
| Emotional state triggers | Rule evaluation in contract engine (§6) |
| Memory recording | Contract evaluation log in `eval_signatures` table |
| Dialogue generation | Contract system with LLM fallback at decision tree edges |
| Soul mutations | Authored `content/storylines/*.ron` with trigger conditions |
| Deliberation UX | `DeliberationState` resource in Bevy (§9) |

### Soul File vs LLM Contract — Division of Responsibility

| Concern | Owned By | Stored In |
|---|---|---|
| Who the NPC is | Soul file | `content/souls/*.ron` |
| How the NPC reacts | Contract system | `contracts` table + in-memory engine |
| What the NPC says | LLM (at decision tree edges) | API response to `POST /llm/call` |
| Story consequences | Soul mutations | `content/storylines/*.ron` + `eval_signatures` |

The soul file answers *who*. The contract answers *how*. The LLM answers *what to say*. The mutation answers *what changed*. Four systems, one integrated pipeline.

---

## 16. Gear, Augmentations & Crafting

**Relationship to V2 infrastructure:** This section defines the equipment ecosystem — every item in ReachLock is procedurally generated from a seed + type + tier, or hand-crafted as an authored override. Items live in `reachlock-core` as data structures with no rendering dependency. The generator produces item icons and in-world models; the bridge layer converts them to Bevy sprites.

### Design Philosophy

ReachLock's procedural generation system makes equipment variety a first-class output. Because sprites, icons, and models are generated from parameters rather than hand-drawn, the game can support thousands of distinct items without an art team. Every weapon variant, every hull paint job, every cybernetic implant has a unique visual generated from its seed and parameters.

**The constraints on equipment are mechanical, not artistic.** If a weapon type can exist in the game systems, its visual exists — the generator produces it.

### Item Type Hierarchy

```
Item
├── Equipment (equippable, affects stats/abilities)
│   ├── Weapons
│   │   ├── Energy (laser, plasma, tachyon)
│   │   ├── Kinetic (cannon, railgun, autocannon)
│   │   ├── Missile (torpedo, missile, decoy)
│   │   ├── Melee (blade, baton, arc-welder)
│   │   └── Boarding (breaching charge, suppression tool)
│   ├── Armor (negates damage types, weight affects mobility)
│   ├── Shields (absorbs damage, recharge rate, type coverage)
│   ├── Engines / Thrusters (speed, turn rate, fuel efficiency)
│   ├── Sensors (range, resolution, stealth detection)
│   ├── Mining Tools (extraction rate, material type)
│   ├── Repair Tools (repair speed, damage type coverage)
│   ├── Cybernetics (permanent implants, stat modifiers)
│   ├── Augmentations (lineage-specific, unlocks new abilities)
│   └── Spacesuits (pressure, temperature, radiation, damage resistance)
├── Consumables (used once, then gone)
│   ├── Medkits, repair packs, ammunition
│   ├── Fuel cells, battery packs
│   ├── Boosters, temporary stat enhancements
│   ├── Grenades, mines, deployable cover
│   └── Data shards (one-time skill unlocks)
├── Components (installed in ship or base)
│   ├── Ship hardpoints (weapon mounts, utility slots)
│   ├── Hull plating, armor segments
│   ├── Power plants, capacitors
│   ├── Jump drive components
│   └── Crafting materials, refined ores
├── Implants (cybernetic, permanent, soul-affecting)
│   ├── Neural lace (enhances LLM contract evaluation speed)
│   ├── Droid interface (direct neural link to ship's dispatch)
│   ├── Memory upgrades (improves crew relationship tracking)
│   └── Faction-specific (Compact loyalty chip, ISC freedom protocol)
└── Cosmetic (no stat effect, pure visual)
    ├── Costumes, hats, ship paints, decals
    ├── Crew outfits, portrait frames
    └── Ship interior decorations
```

### Generation Model

```rust
pub struct ItemSeed {
    pub seed: u64,
    pub item_type: ItemType,
    pub tier: u8,           // 1-10, determines stat range
    pub faction: String,    // Determines visual theme, availability
    pub biome: String,      // Determines materials, for craftable items
}

// Every item has:
pub struct GeneratedItem {
    pub id: String,
    pub seed: u64,
    pub display_name: String,     // Generated from seed + type
    pub description: String,      // Generated flavor text
    pub icon: GeneratedTexture,   // Procedural icon sprite
    pub stats: ItemStats,
    pub weight: f32,
    pub rarity: Rarity,
    pub origin: ItemOrigin,       // Crafted, looted, purchased, quest reward
}
```

- **Name generation:** Each item type has a name template. `{adjective}_{material}_{base}` → "Scorched Ferrite Autocannon", "Cryo-Treated Titanium Plating", "Bleached Bone Neural Lace"
- **Icon generation:** Each type has a procedural icon formula. Energy weapons have glowing cores; kinetic weapons have angular barrels; shields have concentric hex patterns. The seed determines arrangement, color, wear
- **Stat ranges:** Determined by tier + seed variance. Two Tier-4 kinetic cannons with different seeds have different damage/range/fire-rate tradeoffs, but both fall within Tier-4's statistical band
- **Cosmetic variance:** Two of the same item always differ visually — wear patterns, color shifts, minor geometry differences — because the seed affects non-stat parameters

### Crafting System

Crafting is the primary path for obtaining specific, high-tier gear. It operates through authored recipes:

```ron
// content/recipes/autocannon_tier4.ron
(
    id: "autocannon_tier4",
    output: ItemType::Weapon(WeaponType::Autocannon),
    tier: 4,
    materials: {
        "refined_ferrite": 12,
        "energized_coils": 3,
        "coolant_cell": 1,
    },
    skill_required: Weaponsmith(3),
    station_type: OrbitalWorkshop,
    time_to_craft: GameTicks(240),  // 4 hours in-game
    on_craft: [
        // Optional: soul mutation on the crafter
        SoulMutation { target: "player", add_trait: "Gunsmith", chance: 0.3 },
    ],
)
```

- Recipes are authored content — designed, balanced, placed in specific stations or unlocked through faction reputation
- Materials come from foraging, mining, salvaging, and trading
- Critical success odds scale with skill level: higher skill → better stat rolls on the generated item
- Failure consumes materials but yields partial refund or salvage

### Augmentation Implants

Implants are permanent, soul-affecting cybernetic modifications. They blur the line between equipment and character progression:

| Implant | Effect | Acquisition | Risk |
|---|---|---|---|
| Neural Lace Mk2 | +15% LLM contract evaluation speed | ISC black market, 5K credits | Detection by Compact customs |
| Droid Empathy Matrix | Communicates with ship dispatch without terminal | Predecessor ruin loot | None permanent; alien tech unease |
| Compact Loyalty Chip | Faction reputation gains +20% in Compact space | Compact military promotion | Irremovable; flagged in ISC space |
| Reach Survival Kit | Mining yield +25%, damage resistance +10% | Reach faction quest | None |
| Memory Weave | Crew member trust recovers 2x faster after conflict | Corp Charter R&D, 12K credits | May cause conflicting memories |

Implants are authored content, rare, and placed deliberately in the world. They are not procedurally generated at scale — each one is a designed gameplay decision point.

### LLM Integration with Equipment

Imbuement is an artifact from the Predecessors — an equipment item carrying a fragment of consciousness. A droid whose chassis mounts an Imbued core behaves differently in the contract system: its LLM deliberation state has a different system prompt, granting it personality traits derived from the Imbuement's origin.

Equipment interact with the LLM edge system in three ways:

1. **Stat modifiers on deliberation speed.** Better neural lace = faster crew thinking = shorter deliberation state for the player
2. **Contract surface expansion.** Certain implants add new evaluable conditions to the contract engine — a droid empathy matrix lets the contract evaluate crew emotional states; a threat-assessment implant adds tactical analysis conditions
3. **Soul file modifiers.** Some equipment permanently changes soul file parameters — a Compact loyalty chip shifts all `allegiance` values toward the Compact by 0.2 on equipping

---

## 17. Interstellar Travel — The Jump Gate Network & The Procedural Frontier

**Relationship to V2 infrastructure:** This section defines how travel maps to the authored content pipeline (§10) and the seed protocol (§4). The gate network is an authored content graph; deep space beyond the gates is procedural generation from seeds.

### The Two Travel Systems

| Property | Jump Gate | FTL Drive (Ship-Mounted) |
|---|---|---|
| Infrastructure | Fixed gate structures. Gate-to-gate only | Your ship. You go anywhere |
| Who controls | Factions. Compact controls major gates. ISC controls their network. Corp Charters lease access | Anyone with a drive. Permissionless |
| Destinations | Known, charted, curated systems | Unknown, uncharted, generated on discovery |
| Content | Authored. Every system on the gate network has designed content — stations, NPCs, story hooks, faction presence | Procedural + sparse overrides. Deep space systems have a seed, a generated layout, and maybe a content override if a story arc reaches there |
| Risk | Low-moderate. Pirates, blockades, customs inspections | High. Navigation errors, uncharted hazards, drive malfunction, no help for days |
| LLM role | Routine transit. Droid pilot contract mostly coasts | Critical. Long transits, unknown conditions, mission-critical decisions. The deliberation state fires more often |

### The Gate Network as Authored Content

The jump gate network is a directed graph stored as authored content:

```ron
// content/gate_network/compact_sector.ron
(
    region: "Compact Core",
    gates: [
        Gate { from: "aethon", to: "verne", status: Active, controlled_by: "compact" },
        Gate { from: "verne", to: "cadence", status: Active, controlled_by: "compact" },
        Gate { from: "aethon", to: "sorrow", status: Active, controlled_by: "isc" },
        Gate { from: "verne", to: "earth", status: Blockaded, controlled_by: "compact" },
        // Gate to the Veil — restricted military access only
        Gate { from: "verne", to: "the_veil", status: Restricted, controlled_by: "compact" },
    ],
)
```

- Every gate connection is authored — designed to support faction control, trade routes, story arcs
- Gate status changes over time based on faction actions (Active → Blockaded → Contested → Destroyed)
- The gate network defines "charted space" — systems reachable without an FTL drive
- New gates are built by factions as part of major operations or story arcs (authored content deployment)

### Deep Space as Procedural Content

Beyond the gate network lies procedural space. These systems have:

- **A seed** derived from their position in the galaxy map
- **Generated content** — star type, planet count and composition, station probability, resource abundance
- **No authored content** unless a story arc specifically reaches there (content override at event priority)
- **Named by the discoverer** — first player to visit a deep space system sets its name in the seed ledger
- **Variable fidelity** — systems far from any gate have less detailed generation (fewer hand-tuned biome parameters, more noise-driven)

The boundary between gate-connected and deep space is the **frontier**. The game has two content strategies because the universe has two travel modes.

### Discovery Flow for Deep Space

1. Player mounts an FTL drive and jumps to uncharted coordinates
2. Client generates a seed from `(galaxy_x, galaxy_y, galaxy_z, universe)`
3. Client checks content overrides — is there an authored entry for this position?
4. If no override: client generates the entire system from seed — planets, stations, NPCs (from seed-derived faction templates), resources
5. Client sends `POST /seed/discover { system_id: "uncharted_12345", seed: ... }` to server
6. Server records the discovery. Authoritative seed is set. Other players visiting the same coordinates see the same generated system
7. Player can name the system — name is stored in the seed ledger alongside the seed

### Colonization

Colonization is possible but faction-scale. The mechanics exist:

- A player (or player group) who discovers a deep space system can file a colonization claim with a faction
- The faction evaluates the claim based on: resource value, strategic position, gate proximity, the player's reputation
- If approved, the faction invests resources to build a station, establish a gate link (bringing the system into charted space), and appoint the player as administrator
- This is a server-authoritative event queue — colonization takes real time (days/weeks) and requires ongoing player investment
- Factions can fight over colonization claims. A contested claim becomes a war zone

Colonization is not a primary gameplay loop. It is an endgame expression of the persistent universe — a thing that happens because the universe remembers.

---

## 18. The LLM Agency Model — Human-AI Collaboration

**Relationship to V2 infrastructure:** This section extends the LLM contract system (§6), the soul system (§15), and the gear system (§16) into a unified philosophical and mechanical framework for how artificial intelligence functions in the ReachLock universe. It codifies the design intent that LLM success and failure are gameplay, not bugs.

### Core Philosophy

ReachLock's core question about artificial intelligence is not "can the AI do it" but "**who should decide?** "

Every situation in the game asks this question:
- **Should the human decide?** Direct control. The player issues a command manually. Fast, precise, but the player must be present and paying attention
- **Should the AI decide?** Contract automation. The crew handles it autonomously based on player-authored rules. Slow but reliable within rule boundaries
- **Should they collaborate?** Contract with LLM fallback. Rules handle the routine; the LLM handles the edge case. The human and AI act together, each at their strength

The answer differs by situation, player preference, equipment, crew composition, and (crucially) **context** — a well-programmed Boris contract handles cryo piloting flawlessly for 47 jumps and then, on jump 48, encounters a situation the rules didn't cover. The LLM fires. The player watches the deliberation state. Something might go wrong. That's the game.

### The Dispatch Relationship

Every ship has a **dispatch** — the central AI that manages the vessel's automation systems. The dispatch is not a soul file. It is a system: a set of contracts, routing rules, and priorities that govern how the ship operates when the crew is absent or indisposed.

**Robots** are under dispatch authority. They execute orders the dispatch routes to them. A cargo robot in the hold does not decide which cargo to move — it receives an order from the dispatch and executes it. Robots can fail (misread a crate label, drop cargo, take the wrong route) but they do not *decide*.

**Droids** are in relationship with the dispatch. They receive orders but can question them, refuse them, or suggest alternatives. A droid at the helm during cryo transit receives the dispatch's course plan but can deviate if it detects a hazard the dispatch missed. This deviance is the mechanical representation of the droid's pseudo-emergent consciousness (see §17 of the GDD — the FFT loop over the file system).

The dispatch-droid relationship is the game's primary LLM interaction surface:

```
┌─────────────────────────────────────────┐
│                Dispatch                  │
│  (rules engine — authored by player)     │
│  "If course deviation > 5%, alert crew"   │
│  "If fuel < 15%, reroute to nearest       │
│   refueling station"                      │
└────────────┬────────────────────────────┘
             │ orders / queries
┌────────────▼────────────────────────────┐
│              Droid Unit                  │
│  (soul file + LLM edge authority)        │
│  "Understood. But sensors show a         │
│   debris field on that route.             │
│   Recommend 3% deviation to avoid."       │
└─────────────────────────────────────────┘
```

### LLM Failure as Gameplay

Every LLM call has a set of possible outcomes:

| Outcome | Probability (baseline) | Cause | Gameplay Result |
|---|---|---|---|
| **Success** | 70% | Normal operation | AI acts correctly. Player sees deliberation, then action |
| **Timeout** | 10% | Network latency, provider congestion | Contract fallback fires. Ship survives with default behavior. Player reviews log |
| **Misinterpretation** | 10% | LLM misreads the context | AI acts incorrectly but plausibly. Player returns to unexpected results in the log |
| **Confabulation** | 5% | LLM fills a gap with invented data | AI does something creative and wrong. Could be harmless, damaging, or lucky |
| **Model collapse** | 3% | Poor-quality model, degraded output | AI produces gibberish or refuses. Ship enters safe mode until manual override |
| **Catastrophic** | 2% | Rare coincidence of multiple failure modes | AI makes a decision that causes damage, injury, or loss. Permanent consequences |

These probabilities shift based on:
- **Model quality** — a premium model has lower misinterpreation/collapse rates
- **Contract quality** — well-written rules with good edge coverage reduce LLM call frequency
- **Equipment** — neural lace upgrades reduce timeout probability
- **Crew composition** — experienced droid pilots have soul file modifiers that reduce failure rates

**The key design principle: the player should never feel cheated by an LLM failure.** Every failure is preceded by:
1. A deliberation state ("Boris is considering...")
2. A visible context summary ("Unknown signal detected. Rules don't cover this.")
3. A result notification ("Boris: I've decided to hold course. Awaiting manual confirmation.")
4. A log entry the player can review at any time

If the player returns to find their ship drifting off course and a cargo bay fire, they should be able to trace the cause: "Boris timed out during jump 47. Fallback activated. But the fallback didn't account for the debris field. The collision caused the fire." That's a story, not a bug.

### Example: The Cryo Sleep Contract (Extended)

The canonical example from the design, now with full LLM agency model:

**Situation:** Crew enters cryo for a 3-day jump through uncharted space. Boris (droid engineer) is at the helm under dispatch authority.

**Dispatch rules:**
```
Contract: "cryo-pilot"
  Trigger: crew_cryo_activated
  Rules:
    - distance_to_destination < 500: wake_crew
    - fuel < 0.15: wake_crew
    - hostile_detected.range < 500: wake_crew
    - true: maintain_course (default)
  LLM authority: edge — "unexpected sensor data not covered by rules"
```

**Normal jump (30 times out of 35):**
- Dispatch rules cover all situations
- Boris maintains course, wakes crew at destination
- No LLM call. No deliberation state. No log entry of note

**Edge case (4 times out of 35):**
- Sensor detects an anomalous reading — not a hostile, not a navigation hazard, but something the rules don't name
- Dispatch has no matching rule → LLM authority granted
- Deliberation state: "Boris is considering an anomalous gravitational reading..."
- LLM receives context: ship state, sensor data, crew status, contract rules, Boris's soul file
- LLM decides: "This appears to be a gravitational wake from a recently-transited ship. Course is safe. Holding course."
- Outcome recorded. Player sees it in the log later

**Failure case (1 time out of 35, with variance based on equipment/crew):**
- Same scenario. LLM times out (network congestion)
- Fallback fires: maintain_course
- But the anomaly was a nascent black hole — course was not safe
- Boris wakes the crew 6 hours later when hull stress reaches 40%
- Player exits cryo to a ship under strain, alarms blaring, Boris apologizing
- **This is not a bug. This is the game.** The player now has a story: "Remember when Boris almost flew us into a black hole because the network lagged?"

### Player Agency Over LLM Systems

Players can influence AI reliability through:

1. **Write better contracts.** More rules, better edge coverage → fewer LLM calls → fewer failure opportunities. This is the skill ceiling
2. **Upgrade equipment.** Better neural lace, better droid chassis, better sensors → lower failure probabilities. This is the progression path
3. **Choose better models.** Premium cloud inference costs money but reduces failure rates. BYOK lets players use their preferred model. This is the trade-off
4. **Build crew relationships.** A loyal droid with high trust makes better decisions (soul file modifier reduces misinterpretation rate). This is the narrative path
5. **Accept failure.** Some players will lean into the chaos, running cheap models with minimal contracts, embracing the emergent storytelling of constant AI mishaps. This is the sandbox path

All five paths are valid. The game does not enforce a "correct" way to manage AI — it provides the mechanics and lets the player choose their relationship with their ship's intelligence.

### Summary: LLM Agency Integration Points

| Game System | How LLM Fits | Failure Mode | Player Lever |
|---|---|---|---|
| Contract engine (§6) | Edge authority; fires when rules don't cover | Timeout → fallback action | Write tighter rules |
| Soul file (§15) | Provides personality context for LLM deliberation | Misaligned response | Build relationship, unlock secrets |
| Equipment (§16) | Stats modify deliberation speed/failure odds | Equipment malfunctions in combat | Upgrade gear |
| Ship dispatch (above) | Routes orders to droids, evaluates contract priorities | Dispatch logic error | Program dispatch better |
| Cryo transit (§14) | Droid pilots during jump under dispatch rules | Drive malfunction or LLM timeout | Choose crew loadout |
| Combat (§14) | Crew AI in tactical situations | Misread combat situation | Manual override or retreat |
| Economy (§3.3 GDD) | Droid trade negotiators | Bad deal terms | Use human negotiator |

---

## 19. Ship Editor — Exterior & Interior

**Relationship to V2 infrastructure:** The ship editor is a Bevy UI application (§9) that produces `GeneratedLayout` and `HullDefinition` data consumed by the procedural generation system (§5) and the authored content pipeline (§10). The editor is not a separate tool — it is an in-game mode accessed while docked.

### Design Philosophy

The ship is the player's character sheet. Exterior shape says what you are — trader, miner, warship, explorer — before you say a word. Interior layout determines how you live: where crew spend time, how fast emergencies are resolved, which systems are accessible under duress.

The editor exists because customization *is* progression. A player who spends 20 hours tuning their interior layout has a ship that fits their playstyle as precisely as a well-leveled RPG character.

### Exterior Editor

| System | Implementation |
|---|---|
| Hull frame selection | Selection from `content/hulls/*.ron` — each hull class has fixed structural elements (bridge position, engine mounts, hardpoint locations) and customizable zones (plating, paint, decal slots) |
| Hardpoint placement | Grid snapped to predefined slots on the hull frame. Player chooses weapon type, size class, and position. Hardpoint choice determines which weapon models attach visually |
| Engine mounts | Determined by hull class. Player chooses engine model from inventory — affects thruster plume color, size, and the ship's handling parameters in `reachlock-core` |
| Hull plating | Armor segments positioned per zone. Visual damage model degrades plating; breaches expose interior rooms |
| Paint / decals | Layer-based paint system. Primary, secondary, accent colors. Decal slots for faction insignia, crew emblem, earned badges. Colors are palette references — the generator resolves them on render |
| Preview | Real-time Bevy render of the ship as it will appear in space flight mode. Camera orbits around the hull |

All exterior editor data is stored as a `HullConfiguration` struct in `reachlock-core`:

```rust
pub struct HullConfiguration {
    pub hull_id: String,              // References content/hulls/*.ron
    pub seed: u64,                    // Derived from (player_id, ship_name, build_number)
    pub hardpoints: Vec<Hardpoint>,   // Placed weapons, utilities
    pub engine: EngineModel,
    pub plating: Vec<ArmorSegment>,
    pub paint: PaintScheme,
    pub decals: Vec<Decal>,
}
```

### Interior Editor

| System | Implementation |
|---|---|
| Room grid | Grid-based placement. Rooms snap to a tile grid of configurable resolution (default 1 tile = 4m²). Hull class determines available grid area and max room count |
| Room types | Pre-defined templates (cockpit, bridge, med bay, engineering, crew quarters, galley, cargo hold, airlock, hydroponics, workshop, armory, brig). Each template has a size, required systems, and optional furniture slots |
| Corridors | Auto-generated between room door connectors after placement. Player can add or remove corridor segments manually |
| Furniture / systems | Grid-based placement within rooms. Med bay furniture: med station, pharmacy locker, triage bed. Engineering: reactor console, repair bench, component storage. Furniture affects gameplay — a fully-equipped med bay heals crew faster; a sparse one is slower |
| Room adjacency bonuses | Rooms placed next to compatible types gain adjacency bonuses. Galley next to crew quarters → +0.1 crew relationship recovery per tick. Engineering next to cargo hold → faster repair material transfer |
| Cost and time | Each room, furniture piece, and system has a material cost and construction time. Refits happen while docked; larger refits take longer in-game |

All interior editor data is stored as a `ShipInteriorLayout`:

```rust
pub struct ShipInteriorLayout {
    pub hull_id: String,
    pub rooms: Vec<PlacedRoom>,
    pub corridors: Vec<Corridor>,
    pub furniture: Vec<PlacedFurniture>,
    pub seed: u64,
}
```

### Editor Access

- The editor is accessed from the shipyard menu while docked at a station with refit capabilities
- Refit cost scales with the scope of changes: paint-only is cheap and fast; interior reconfiguration is expensive and time-consuming
- Certain hull modifications (hardpoint relocation, engine swap) can only be performed at stations with the appropriate facilities (orbital dry dock for military hulls, independent shipyard for modular hulls)
- The editor is fully functional offline; refit timers only advance while the game is running

---

## 20. Dynamic Economy System

**Relationship to V2 infrastructure:** The economy runs as part of the universe tick (§8) in `reachlock-server`. It reads from authored data files (goods definitions, production chains) and operates on Postgres tables. Supply and demand computations are deterministic from seed + event state.

### Core Model

Every station and system in the galaxy has:

- **Supply table:** Goods it produces, base sell price, production rate, production capacity
- **Demand table:** Goods it consumes, base buy price, consumption rate, consumption ceiling
- **Storage:** Inventory of each good currently in stock (procedurally generated from seed at system creation, updated by player trade and universe tick)

Prices are computed from base prices modulated by supply/demand ratios, faction tariffs, event modifiers, and player activity:

```
current_price = base_price * demand/supply_ratio * faction_tariff * event_modifier
```

### Goods Definition

Goods are authored content:

```ron
// content/economy/goods.ron
(
    id: "refined_ferrite",
    category: RawMaterial,
    base_price: 45,           // credits per unit
    weight: 2.5,              // cargo space per unit
    rarity: Common,
    production: [              // which station types produce this
        MiningStation(Mining),
        Refinery,
        IndustrialOutpost,
    ],
    consumption: [             // which station types consume this
        OrbitalWorkshop(Manufacturing),
        Shipyard,
        ResearchStation,
    ],
    // Flavor text generated from seed + context
    description_template: "{quality} {origin} ferrite, processed for {use}",
)
```

### Faction Tariffs

Each faction sets tariffs on goods passing through their territory:

| Faction | Policy | Effect |
|---|---|---|
| Compact | Regulated — tariffs on foreign goods, subsidies on Compact-produced goods | Compact goods 15% cheaper in Compact space. Non-Compact goods 20% more expensive |
| ISC | Free trade zone — minimal tariffs | +5% on all goods (flat port fee). No faction preference |
| Corp Charter | Profit-optimized — tariffs adjust to maximize revenue | Dynamic. Tariffs increase when demand is high, decrease when demand is low |
| The Reach | No tariffs, no enforcement | 0% tariff. Higher pirate/bandit risk offsets |

### Player Participation

Players interact with the economy through:

1. **Trade routes.** Buy low in one system, sell high in another. Requires cargo capacity, knowledge of routes, and tolerance for risk (pirates, blockades, customs)
2. **Contract trading.** Dispatch-managed trade. Player programs a contract ("buy ferrite if price < 40, sell if price > 60, route between Verne and Sorrow") and the crew executes it autonomously. LLM fires if market conditions don't match any rule
3. **Infrastructure investment.** Player can invest credits in station production expansions. Diverted cargo becomes share of station output. Returns dividend income while the player is away
4. **Blockade running.** Systems under blockade have supply shortages. Goods that can reach them command premium prices. The trade-off is combat risk and faction reputation damage
5. **Sabotage.** Players aligned with one faction can disrupt competitor supply chains. Consequences include faction reputation loss with the target, bounties, and faction warfare escalation

### Universe Tick Integration

Every universe tick (§8), the economy system:

1. Reads current supply/demand for all systems within the tick's scope
2. Applies faction tariff and event modifiers
3. Recalculates prices
4. Moves goods along trade routes (NPC shipping)
5. Updates station inventories
6. Generates events (shortages, surpluses, price spikes) for player notification

The economy is deterministic from the same initial seed + event history. Two universes with the same seed and same event log have the same prices at the same tick.

---

## 21. Faction Engine

**Relationship to V2 infrastructure:** The faction engine runs as a service in `reachlock-server` (§8). Faction definitions are authored content in `content/factions/*.ron`. Player reputation data is stored in the Postgres database. The engine drives the universe tick and generates events consumed by the content service.

### Faction Definition

```rust
// reachlock-core/src/faction/types.rs

pub struct Faction {
    pub id: String,
    pub name: String,
    pub territory: Vec<SystemClaim>,
    pub resources: FactionResources,
    pub relationships: HashMap<String, DiplomaticStanding>,
    pub goals: Vec<FactionGoal>,
    pub internal_divisions: Vec<InternalDivision>,
    pub doctrine: Doctrine,     // Military, Economic, Diplomatic, Expansionist
}

pub struct DiplomaticStanding {
    pub status: RelationStatus,  // Allied, Friendly, Neutral, Hostile, War
    pub treaty: Option<Treaty>,
    pub war_goal: Option<WarGoal>,
}

pub struct InternalDivision {
    pub id: String,
    pub name: String,
    pub influence: f32,           // 0.0-1.0: share of faction decision-making
    pub agenda: DivisionAgenda,  // Hawkish, Dovish, Mercantile, Isolationist
    pub player_standing: f32,    // Separate from faction-level standing
}
```

### Player Reputation (Multi-Axis)

| Axis | Range | Description |
|---|---|---|
| Trust | -100 to 100 | Kept promises? Delivered contracts? Reliable? |
| Contribution | -100 to 100 | Material help provided to the faction |
| Notoriety | 0 to 100 | How visible are your deeds? High notoriety prevents quiet operations |
| Crimes | List of recorded offenses | Smuggling, piracy, murder of faction personnel |

Reputation is granular per faction AND per internal division. Helping the Compact's expansionist wing (building a new gate) may damage standing with the Compact's isolationist wing. Each division has its own trust and contribution trackers.

Reputation unlocks: better trade prices, unique missions, restricted area access, ship blueprints, exclusive crew recruits. Reputation also locks: too close to the Compact blocks ISC access entirely.

### Faction Storylines

Each major faction has a storyline arc that advances on the universe tick with or without the player. These are authored content files published through the content pipeline:

```ron
// content/storylines/compact_arc.ron
(
    faction: "compact",
    chapters: [
        Chapter {
            id: "the_veil_escalation",
            trigger: TickCondition(tick_count > 1000),
            events: [
                FactionMove { action: DeployFleet, target: "the_veil", strength: High },
                DiplomaticShift { faction: "isc", change: -20 },
                ContentRelease { content_id: "veil_research_station", priority: Event },
            ],
            narration: "The Compact has deployed a task force to the Veil...",
        },
        Chapter {
            id: "alexanders_gambit",
            trigger: And(ChapterComplete("the_veil_escalation"), PlayerReputation("compact", Trust > 50)),
            events: [
                MissionUnlock { mission_id: "alexander_proposal" },
                NPCUpdate { npc_id: "alexander", add_goal: "recruit_player" },
            ],
        },
    ],
)
```

### Faction Warfare

Wars have causes, conduct, and resolution:

1. **Causes:** Territory dispute, resource conflict, ideology clash, player action tipped the balance, story arc trigger
2. **Conduct:** Factions commit resources proportionally to war goal importance. Border systems change hands. Trade routes are severed. Station ownership changes
3. **Resolution:** Peace treaty (territory concessions), stalemate (ceasefire), annihilation (one faction eliminated), or player intervention tips the balance

Players can participate as combatants, mercenaries, blockade runners, diplomats, or profiteers. Players can defect mid-war — treachery is mechanically supported with reputation consequences.

### LLM Integration

Faction AI decision-making uses the LLM at the edges of the faction's doctrinal rule tree:

- Routine decisions (resource allocation, trade route management) are deterministic rules
- Strategic decisions (declare war, accept peace, respond to player diplomacy) feed context to the LLM with the faction's doctrine, current state, and history
- LLM generates the faction's response text, which appears as in-game news broadcasts, diplomatic communiques, and NPC dialogue

---

## 22. Combat System

**Relationship to V2 infrastructure:** Combat exists in all three modes (§14). It uses Bevy's physics engine (rapier2d for landed/on-board, simplified physics for space), the equipment system (§16) for weapons and damage, and the contract system (§6) for crew AI. Enemy AI for space combat uses behavior trees in `reachlock-core` — separate from the LLM contract system (enemies are not crew).

### Landed Combat — Zelda-style

**Camera:** Top-down or isometric, same as exploration

**Mechanics:**
- Real-time combat with lock-on targeting
- Light attack, heavy attack, dodge roll, block/parry
- Melee weapons (blade, baton, arc-welder) and ranged weapons (pistol, rifle, shotgun)
- Crew companions — one or two crew members accompany the player, each with their own combat AI driven by their soul file and contracts
- Environmental interactables — explosive barrels, crumbling platforms, hackable terminals
- Stealth options — avoid combat, silent takedowns, distract guards
- Dungeon bosses with patterns, phases, weak points, and damage gating

**Integration:**
- Weapon stats come from equipment system (§16)
- Crew combat behavior comes from contract system (§6) with soul file context (§15)
- Dungeon layouts come from authored content (§10) with authoritative priority
- Enemy AI is behavior tree in `reachlock-core` — no LLM involvement

### On-Board Combat — Real-Time Tactical

**Camera:** Side-on or isometric ship interior view (same as On-Board mode)

**Trigger:** Boarding action — enemies breach the hull and enter the ship

**Mechanics:**
- Player and crew fight in ship corridors, rooms, and chokepoints
- Cover matters — doorframes, crates, consoles provide partial cover
- Friendly fire is real — a poorly placed shot hits a crew member
- Medical evacuation under fire — injured crew must reach the med bay
- Non-lethal options — subdue, capture for interrogation
- Ship systems are vulnerable — combat in engineering risks reactor damage

**Consequences:**
- Crew members can be killed permanently — if a soul file reaches zero health and the player doesn't evacuate them, they're gone
- Ship damage from boarding action requires repair time and materials
- Captured enemies can be interrogated (LLM deliberation state for interrogation dialogue)

### Space Combat — Star Fox 64

**Camera:** Third-person chase-cam or cockpit view

**Mechanics:**
- Six-DOF flight with cinematic feel. Roll, pitch, yaw, thrust, brake, boost
- Weapons fire from hardpoints placed in the ship editor (§19)
- Power management: allocate energy to weapons, shields, or engines in real time via the ship's power console (On-Board mode) or quick keys (Space Flight mode)
- Subsystem targeting: target enemy engines (disable escape), weapons (neutralize threat), FTL drive (prevent jump), sensors (blind them)
- Enemy ship classes with distinct roles: interceptor (fast, light), bomber (slow, heavy), capital ship (many subsystems, heavy armor, point defense)
- Boss encounters: capital ships with multiple subsystems to disable, dreadnoughts requiring coordinated attacks
- Escape — afterburners, chaff, emergency jump (high risk of drive malfunction)

**Enemy AI:**
- Behavior trees in `reachlock-core` with states: Patrol, Engage, Evade, Retreat, RequestReinforcements
- No LLM involvement — enemy AI is pure computation for deterministic, fair combat
- Enemy difficulty scales with system threat level (authored per system or generated from seed based on faction presence)

### LLM Integration in Combat

The LLM touches combat only through crew behavior:

| Situation | LLM Role | No LLM Fallback |
|---|---|---|
| Crew combat companion | Tactical decisions — when to use abilities, when to retreat | Default: follow player, attack nearest hostile |
| Damage control prioritization | Which system to repair first under fire | Default: repair nearest damaged system |
| Evacuation decisions | Whether to abandon ship or fight on | Default: fight until hull < 10%, then evacuate |
| Enemy interrogation | Dialogue tree for questioning captured enemies | Default: scripted interrogation responses |

---

## 23. Modding Framework

**Relationship to V2 infrastructure:** The modding framework is the authored content pipeline (§10) opened to the community. Every tool the development team uses to author content is available to modders through the same CLI, schemas, and data formats.

### Architecture

The game has three layers, strictly separated:

```
Layer 3: Content (our mod + community mods)
    └── reachlock/content/ — .ron, .json, .yaml files defining everything
Layer 2: Framework (CLI, schemas, scripting API)
    └── reachlock-cli/ — content validate, preview, publish
    └── content/schemas/ — JSON Schema for every content type
Layer 1: Engine (the compiled binary)
    └── reachlock-core/ + reachlock-client/ — the game itself
```

**Our game content is a mod.** It ships with the engine as a bundled mod but loads through the same mod loader as everything else. No private APIs. No special access.

### Mod Structure

```
my_mod/
├── mod.manifest.ron          # Mod metadata
├── stations/
│   └── my_station.ron
├── souls/
│   └── my_npc.ron
├── hulls/
│   └── my_ship.ron
├── factions/
│   └── my_faction.json
├── economy/
│   └── my_goods.ron
├── contracts/
│   └── my_ai.ron
├── storylines/
│   └── my_arc.ron
└── assets/                   # Custom textures, audio (if any)
    └── my_icon.png
```

### Mod Manifest

```ron
// my_mod/mod.manifest.ron
(
    id: "my_cool_mod",
    name: "My Cool Mod",
    version: "1.0.0",
    author: "Community Modder",
    description: "Adds a new faction, three stations, and a storyline arc",
    dependencies: [],
    conflicts: ["other_mod"],
    content: [
        ContentAdd { type: Faction, id: "my_faction" },
        ContentAdd { type: Station, id: "my_station" },
        ContentAdd { type: Soul, id: "my_npc" },
        ContentOverride { system_id: "aethon", object_id: "bar", priority: Curated },
    ],
)
```

### Mod Distribution

- Mods are distributed as `.reachmod` packages (a zip of the mod directory)
- A mod manager in the launcher lists installed mods, shows dependencies, enables/disables/reorders
- Mods can be installed manually or through community platforms
- The mod loader checks for content ID collisions at startup and reports conflicts with resolution options (load anyway — last wins, skip conflicting mod, open mod manager)

### Framework Commitment

Every piece of ReachLock content the team authors goes through the same APIs and data formats a modder would use. If the dialogue system is hard to use, the team feels that pain first and fixes it before shipping. The framework documentation ships with the SDK at the same time the game launches.

---

## 24. Development Plan

**Note:** This section defines milestones and outcomes only. No time estimates, no line counts, no durations. The milestones are ordered by dependency — each builds on the previous. Fable will determine the optimal path to each outcome.

### Phase 0 — Architecture Spike

**Outcome:** A WASM build renders a procedurally-generated ship from a seed. A different seed produces a different ship. An authored override renders instead of a generated one. The server records the canonical seed with atomic first-write-wins.

| Item | Depends On |
|---|---|
| Cargo workspace, `reachlock-core` crate structure | Nothing |
| WASM build validation — full Bevy plugin stack on `wasm32` | Cargo workspace |
| Determinism test harness — same generator output across x86, ARM, WASM | Core |
| Seed protocol — one generator produces a ship sprite via Bevy + Lyon | Core, Bevy client |
| Seed ledger — Postgres table with atomic first-write-wins | Server |
| Content validation CLI — validate a hand-authored `.ron` file | CLI |
| One override — hand-crafted ship replaces generated one by ID | Bridge layer |
| One contract — "Boris pilots during cryo", rules only, no LLM | Core contract engine |

### Phase 1 — Engine & Framework

**Outcome:** A player can fly from a station to a planet, walk around, talk to an NPC, buy something, mine an asteroid, fight a ship, and return. The universe ticks whether the player is present or not.

- [ ] Three-mode game loop (Landed, On-Board, Space Flight) with transitions
- [ ] Ship editor (exterior + interior) with Bevy UI
- [ ] Flight model — 6-DOF space flight with ship class handling
- [ ] One authored station with NPCs (soul files + contracts)
- [ ] Space combat — dogfighting with one enemy type
- [ ] Landed combat — basic melee and ranged, one NPC companion
- [ ] Dynamic economy — buy/sell at two stations, prices shift
- [ ] Faction engine — 3 factions with multi-axis reputation
- [ ] Universe tick — faction AI, economy updates, event generation
- [ ] Mod loader — `.reachmod` packages, conflict resolution
- [ ] Content pipeline complete — all schemas, CLI tools, validation

### Phase 2 — The Story & The Crew

**Outcome:** A player can play through the authored narrative from the Loup-Garou's perspective, make meaningful choices, lose crew members permanently, and reach one of multiple endings.

- [ ] Loup-Garou crew roster — 7 soul files with full arcs
- [ ] Crew relationship system — spatial behavior, trust, familiarity, conflicts
- [ ] On-board crisis events — fires, hull breaches, boarding actions, medical emergencies
- [ ] Predecessor ruins — designed dungeons with puzzles, boss fights, tools (5-7)
- [ ] Faction storylines — Compact, ISC, Corp Charters, Reach, Earth's Remnant
- [ ] The Duskway runs — Earth blockade running missions
- [ ] The Veil — escalation arc with Predecessor revelation
- [ ] Alexander's long game — visible player-facing consequences
- [ ] Soul mutations tied to storyline beats
- [ ] Ship log — full decision history from contract evaluations
- [ ] LLM deliberation UX — "crew is thinking" overlay with context summary
- [ ] LLM failure outcomes — timeout, misinterpretation, confabulation handling

### Phase 3 — The MMO & The Persistence

**Outcome:** A player can log into the persistent universe, encounter other players, contribute to faction warfare, and see the universe state change based on collective action.

- [ ] Server runtime — Tokio + Axum WebSocket handler, Postgres, Redis
- [ ] Seed ledger — atomic first-write-wins, canonical seed distribution
- [ ] Universe tick server-side — faction AI, economy, events
- [ ] Signed contract evaluations — hash chain verification
- [ ] Player coordination — multiple players in the same system, proximity, chat
- [ ] LLM proxy — per-tier routing, rate limiting, deliberation UX
- [ ] BYOK integration — player-provided API keys for crew AI
- [ ] Multi-universe tiers — Classic, Fair Play, Spectrum, BYOK
- [ ] Auth and session management
- [ ] Colonization mechanics — claim filing, faction evaluation, gate construction
- [ ] Story chapter deployment pipeline — tooling for server-side content updates

### Phase 4 — Polish & Release

**Outcome:** ReachLock is playable in browser and desktop. The authored single-player story is complete. The MMO persistent universe is online. The modding framework is documented and usable.

- [ ] Full ship customization catalogue — broad hull and component selection
- [ ] Economy balancing and tuning across all systems
- [ ] UI/UX polish across all three modes
- [ ] Audio implementation — procedural music, sound effects, ambient
- [ ] LLM model integration — local inference, cloud provider routing
- [ ] Mod SDK finalization — documentation, example mod, asset pipeline
- [ ] Platform distribution preparation
- [ ] WASM distribution — the game is a URL
- [ ] Native builds — desktop, mobile
- [ ] Beta testing
