# M11 — "Whose boots are you in?" — the crew pass ✅ (2026-07-09)

Sprint 3 of the Duskway Run demo: the player becomes one of the crew, the
ship becomes two decks with real physics between them, conversation gets
its own UI, and combat damage follows you inside the hull.

## Character select + the scrawl

- **A fresh game opens on the select screen**: all 7 crew (npc `playable`
  block — schema additive) with portrait, tagline, the five framework
  stats as pips (piloting / engineering / medicine / grit / savvy),
  advantages/disadvantages, and locomotion quirks. Keyboard or mouse.
- **The opening text scrawl** rolls the manifest's shared scenario
  paragraphs (`start.scrawl`) plus the chosen character's own — Boris's
  boot log reads nothing like Tove's ledger. Enter skips.
- **Stats are gameplay**, read generically by the engine
  (`GameState.player_stat`): piloting = top speed/turn; engineering =
  repair speed, seam width, damage-penalty trim; medicine = triage (fewer
  interior casualties per hit); grit = hull soak + zero-G steadiness;
  savvy = outfitting prices. Upgrades ride on top via `stat_<name>`
  effect keys.
- **Playing a scene's own speaker**: dialogues the player IS the npc of
  can't run — `playable.self_dialogue_summaries` plays a second-person
  narration card instead, applies the scene's story mutations (flags land
  the moment the card shows — a mid-card save can't strand a stage), and
  reports `dialogue_end` so missions advance. Verified live: Tove's
  briefing and Prudence's bar fight both complete their acts.
- Headless/CI: select skips; `REACHLOCK_CHARACTER=<id>` picks a seat.

## The two-deck Loup-Garou

- Hull schema additions: `decks` (independent gravity per deck),
  `rooms[].deck`, `ladders`. **Upper deck (zero-G)**: cockpit, landing
  bay (the grafted shuttle, drawn large), ore processing, cargo hold.
  **Lower deck (grav plates)**: bridge, galley, quarters, med bay,
  engineering, cryo, airlock. One main ladder connects them (R to climb).
- **Zero-G is momentum**: thrust adds drift, walls stop you, grit bleeds
  drift faster; the sprite floats, sways, loses its ground shadow.
- **Boris**: `locomotion` npc block — `zero_g: magnetic` walks the upper
  deck like a corridor; `gravity_speed_mult: 0.3` crawls downstairs. His
  station moved to ore processing; he *lives* upstairs. Mag boots
  (upgrade, `magnetic_soles` effect) buy organics the same trick.

## Interior damage — the fight follows you inside

- Hits that get past the shields can start a **fire / arcing conduit /
  hull breach in a real room** (save schema `ship.damage`), announced by
  role on the intercom with the room's name.
- Aboard, damage **renders where it burns** (flickering two-frame decals,
  glow, scorch) and is **repairable**: R opens the repair rig — a
  timing-bar minigame where engineering widens the seam and the plasma
  welder cuts the strikes needed. Slips feed the fire.
- **Crew fix things themselves**: npcs flagged `repairs` (Boris, Risc)
  walk to damage, cross decks by the ladder (Boris descends *slowly*),
  and seal it over time. The suppression-net upgrade burns fires down
  unattended.
- **Unrepaired damage costs the next flight**: speed, gun cycle, and
  damage vulnerability all degrade (`flight_damage_penalty`), softened by
  a real engineer. An unrepaired **conduit cuts its deck's grav plates**
  until fixed — the lower deck floats.

## Dialogue UI overhaul

- **DialoguePanel** (new, the only conversation surface, all four hosts):
  isolated bottom panel with nameplate, typewriter body, in-panel choices
  (mouse or 1–9). The scrolling log keeps only system notes.
- **SNES typewriter**: every line prints character by character (55 cps),
  R fast-forwards — and generation hides inside presentation: a buffer
  line is still typing while the mind composes.
- **The mind-status lamp** answers "how long might this take" up front:
  `SCRIPTED` (instant), `MIND LINKED` (live daemon — generated beats take
  seconds), `COMPOSING…` (pulsing, working right now), `LINK OFFLINE`
  (fallback lines only).

## Widgets gamified

- **Ore processor** (new upper-deck station): the crusher press — time the
  marker into the seam band and `ratio` raw ore refines into alloys; miss
  and a unit grinds to dust. Conversion is data (`stations[].converts`).
- **Gunnery calibration range**: click the drone while the reticle bites;
  three hits calibrate the feed — **+15% fire rate, consumed by the next
  flight** (save: `weapons_calibrated`).
- **Scanner active ping**: lights every contact at once with an expanding
  pulse (and the flavor text tells you why you shouldn't love it).
- New upgrades that all do something: mag boots, plasma welder, fire
  suppression net, gyro stabilizers (turn_mult), cockpit sim module
  (stat_piloting +1). Savvy talks every price down at the counter.

## Verified

- **115/115 GUT tests** (+31: playable layer 6, ship damage 8, decks &
  locomotion 6, dialogue panel 8, upgrade effects 4… see tests/).
- `make check` green (architecture guard caught — and I fixed — an
  effect key that doubled as a content id), dsl-bridge green.
- Live-stack smokes: fresh boot as Tove (briefing card → Act 1 stage
  advances), Prudence docked at Sorrow (bar-fight card → `bar_fight_done`
  → Act 3), wounded flight (damage penalty + calibration consumed + new
  damage spawned mid-fight), Boris fresh boot (ore-processing station,
  magnetic locomotion).

## Known gaps (tracked, non-blocking)

- Crew AI beyond damage control is stationary (no idle wander/schedules).
- The shuttle in the landing bay is set dressing — not yet flyable.
- Breaches don't vent atmosphere; conduits cut gravity but nothing else.
- Zero-G crew without mag-locks move hand-over-hand (slower walk + float
  pose) rather than true drift pathing.
- The select screen uses generated sheet portraits; an artist replaces
  PNGs file-by-file as before.
