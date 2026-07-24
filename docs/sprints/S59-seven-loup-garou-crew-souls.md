# S59 — 7 Loup-Garou Crew Souls

**Spec:** §10 (soul content), §15 (soul system) · **Wave 16 (Content Authoring) · Depends on:** S58

## Outcome

The canonical crew of the Loup-Garou — the player's default ship — exists as seven authored `.ron` soul files in `content/souls/`. Each crew member has a name, backstory, personality profile, voice parameters, goals, secrets, breaking points, and starting contracts. These are the foundational authored content — every story arc, dialogue, and narrative event references these souls.

## Context

- The spec (§15) defines `SoulDefinition` with: identity (name, id), personality (FIVE trait scores, emotional volatility), backstory (narrative text, key events), secrets (1-3 per soul), goals (1-3 per soul), breaking points (triggers that cause emotional spiral), voice parameters (pitch, speed, accent), contracts (references to contract definitions this soul understands).
- The Loup-Garou crew was defined in the GDD but never authored as content files. The souls exist as concepts and brief descriptions in various design documents.
- These files will be read by the soul system (`core/src/soul/`), the dialogue system (S16), the crew dynamics system (S33), and every storyline (S60).

## Freeze first

1. Each soul file follows the `SoulDefinition` schema validated by `schemas/soul.schema.json` in S53.
2. All files use `.ron` format with the exact key ordering the struct uses for minimal diff noise on edits.

## Deliverables

- [ ] **`content/souls/alexandre_dubois.ron`** — Captain, Compact loyalist, former ISC officer turned privateer. Personality: high conscientiousness, moderate emotional stability, low openness to new experiences (set in his ways). Goal: protect the crew, complete the current contract. Secret: was involved in the Charlevoix Incident. Breaking point: harm to a crew member under his command.
- [ ] **`content/souls/boris.ron`** — Droid engineer, cryo pilot, ship's second-in-command during transits. Personality: high agreeableness, moderate intelligence, but literal-minded — interprets contracts exactly as written. Goal: maintain the ship's systems, prove a droid can be more than its programming. Secret: has pre-wipe memories of a previous crew's fate. Breaking point: being forced to violate a direct order from the dispatch.
- [ ] **`content/souls/tib.ron`** — Robot mechanic, physical comedy relief. A clunky utility robot with unexpected competence. Personality: high openness, low conscientiousness, high neuroticism (panics easily but recovers). Goal: be useful (and maybe be seen as a "real" crew member). Secret: once accidentally caused a cargo bay decompression. Breaking point: being ignored or treated as furniture.
- [ ] **`content/souls/tove.ron`** — Human comms officer, ex-ISC defector. Left the ISC after a crisis of conscience. Personality: high extraversion, high openness, moderate neuroticism. Goal: build a new life outside the ISC, make amends. Secret: still has contacts inside ISC intelligence. Breaking point: being forced to choose between the crew and her old ISC network.
- [ ] **`content/souls/doss_grey.ron`** — Voidborn navigator, elder, cryptic. Born and raised on deep-space stations; has never set foot on a planet. Personality: low extraversion, high openness, very high neuroticism (paranoid but often right). Goal: find the source of a mysterious signal he's been tracking for years. Secret: the signal may be of Predecessor origin. Breaking point: having his navigational autonomy overridden.
- [ ] **`content/souls/grissom.ron`** — Human mercenary, muscle, pragmatic. Hired muscle who's been with the crew long enough to become family. Personality: low neuroticism, low openness, high conscientiousness. Goal: get paid, keep the crew alive. Secret: deserted from the Compact Marines and is wanted for assault on an officer. Breaking point: seeing civilians get hurt in a fight.
- [ ] **`content/souls/yael.ron`** — Xenotype medic, secret past. A skilled physician of mysterious origin (the Xenotype backstory — part of a secret eugenics program). Personality: moderate openness, very high emotional stability, low extraversion. Goal: keep the crew alive while hiding from her creators. Secret: she is the product of an illegal genetic engineering program and is being tracked. Breaking point: discovery of her origins by anyone outside the crew.

### Each soul file includes:

- [ ] `id` — unique identifier, snake_case (e.g., `alexandre_dubois`)
- [ ] `display_name` — in-game name
- [ ] `portrait_id` — seed for procedural portrait (S25 sprite generator)
- [ ] `species` — Human, Synthetic, Robot, Voidborn, Xenotype
- [ ] `personality` — FIVE scores (all 0-1024 fixed-point), emotional volatility (0-1024)
- [ ] `voice_params` — pitch (0-1024), speed (0-1024), accent (string)
- [ ] `backstory` — 2-3 paragraph narrative text with key_event list
- [ ] `goals` — 1-3 goals with `description`, `priority` (1-5), `milestone` tick or trigger
- [ ] `secrets` — 1-3 secrets with `description`, `revealed` (bool, default false), `revelation_consequence` (text)
- [ ] `breaking_points` — 1-2 breaking points with `trigger` description, `effect` (text describing emotional/behavioral change)
- [ ] `contract_refs` — contract IDs this soul references (e.g., `["cryo-pilot", "sensor-sweep"]`)
- [ ] `starting_relationship` — initial opinions of other crew (affinity -100..100 per crew member)

## Acceptance gates

```
reachlock-cli content validate content/souls/alexandre_dubois.ron
reachlock-cli content validate content/souls/boris.ron
reachlock-cli content validate content/souls/tib.ron
reachlock-cli content validate content/souls/tove.ron
reachlock-cli content validate content/souls/doss_grey.ron
reachlock-cli content validate content/souls/grissom.ron
reachlock-cli content validate content/souls/yael.ron
# All pass — every soul file validates against the schema
cargo test -p reachlock-core soul::
# Soul system can load and parse all 7 files
make check
```

## Non-goals

Writing full crew dynamics (S33) or dialogue trees for these souls (future story work). Voice recording or TTS synthesis for the voice params — `voice_params` are authored as metadata for future use. Contracts referenced by these souls may not exist yet as authored content — they reference the procedural contract generator.

## Gotchas

- The `starting_relationship` map must include all other crew members. Each soul expresses an opinion of every other soul. The relationship values should form a coherent social graph (not all 0, not all 100). Create relationships that suggest history and tension.
- Secret revelations are story hooks. Each secret should have a plausible gameplay trigger (e.g., "revealed when ISC board the ship" or "revealed when the crew visits Charlevoix"). These are authored as text for now — the story engine triggers them.
- The `portrait_id` is a seed for the procedural sprite generator (S25). Pick 7 distinct seeds. Preview each portrait with `reachlock-cli determinism gen_sprite <seed>` to verify the portrait looks appropriate.
- Backstories must be consistent with faction lore: Compact, ISC, Corporate Charters, The Reach, Earth's Remnant (detailed in S60). Alexandre's past, Tove's defection, and Grissom's desertion all reference specific faction events — coordinate these with the faction profiles.
