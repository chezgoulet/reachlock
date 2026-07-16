# Reachlock — Ships, Crews, and Stations Design

Canon companion to `docs/LORE.md` (§III Technology, §IV The Ship). This
document specifies how ships work as *gameplay*: classes, interiors, crewed
stations, damage, and the FTL-cryo loop. Every ship class from a one-person
fighter to a capital ship works the same way at a different scale — more or
fewer systems, more or fewer complications.

The feel target: **FTL crossed with Sea of Thieves / Among Us.** A ship is a
place your crew runs around inside, operating physical stations, while the
consequences render in real time in the Star Fox flight view outside.

---

## 1. The core mechanic: stations operate systems

A ship system does nothing by itself. It is **operated** — by a player
character standing at its station, by an NPC crew member (human, android, or
robot — possibly LLM-driven), or by a ship automation system *if the hull
supports automation and the module is installed*.

- **Stations are physical consoles in the interior.** To fire the guns,
  someone must be at the tactical station. To scan, someone at the scanner
  array. To reroute power, someone in engineering. To jump, someone must
  program the jump at nav.
- **Each station has its own view.** The pilot in the cockpit sees the Star
  Fox flight view. The tactical operator sees a targeting HUD — and their
  shots render in real time in the flight view. The miner sees the beam/
  extraction interface; the scanner operator sees contacts resolve. Same
  world, different lenses, simultaneously.
- **Crew coordination is the game.** Mining an asteroid takes a pilot holding
  position, a miner running extraction, and someone in the tech bay breaking
  down raw material. Combat takes helm + tactical + engineering (power) at
  minimum. Solo players rely on automation modules, NPC crew, or accept
  operating one station at a time.

### Operators

| Operator | Notes |
| --- | --- |
| Player character | Walks to the station, interacts, gets the station view. |
| NPC crew | Ordered to a station; competence varies; LLM-driven crew converse, improvise, and *fail in character* — a first-class source of emergent gameplay. |
| Automation module | Occupies a hull slot. Handles a station without a body at it, within limits (no improvisation, degraded under damage, hackable/jammable later). |

## 2. Ship classes and slots

Hull classes differ in interior footprint, system count, crew capacity, and
**customizability slots** (weapon, module, automation). The interior layout
must always make sense within the footprint of the exterior hull — rooms fit
inside the silhouette you see in flight.

| Class | Scale | Crew | Decks | FTL | Slots (indicative) |
| --- | --- | --- | --- | --- | --- |
| Fighter | one person | 1 | 1 (cockpit only) | gate-only | 1–2 |
| Shuttle | support craft | 1–2 | 1 | none (72h life support) | 1 |
| Class-J (Loup-Garou) | smallest jump-capable hull | 4–8 | 2 | self-jump | 4–6 |
| Freighter | cargo hauler | 6–12 | 2–3 | gate-only or jump variant | 6–8 |
| Capital | fleet backbone | 20+ | many | self-jump | 10+ |

Class-J is the design anchor: everything below it is a subtraction (fewer
rooms, fewer stations), everything above it an addition (more of each, plus
complications like multiple engineering bays and internal travel time).

## 3. FTL and cryosleep (the jump loop)

Per `docs/LORE.md` §III: gate transit is safe awake; **self-generated jumps
are lethal to conscious biological crew**. This is a gameplay loop, not
flavor:

1. Someone programs the jump at the nav station (vector, window, wake
   conditions) — or programs the **jump automation** if installed.
2. Every *human* crew member must reach a cryo pod and enter stasis before
   the window opens. Living characters not in cryo when the window opens
   die or are ruined (vascular + psychological damage — no partial credit).
3. Androids and robots are unaffected. They run the ship through the
   crossing — Prudence and Risc's canonical role. An android/robot operator
   can be a human player or an LLM.
4. On emergence, the synthetic crew (or automation) revives the sleepers.

The tension: the jump clock vs. bodies reaching pods. A crew that cuts it
close is gambling. A ship with no synthetic crew and no automation module
must trust the automation timer completely — and automation doesn't
improvise when something goes wrong mid-countdown.

Cryo capacity is a hard constraint: **one pod per human aboard or someone
stays behind / dies.** The Loup-Garou carries 10 pods for a crew of 4 humans
— headroom for passengers, rescues, and prisoners; also a resource.

## 4. Damage, fires, and repair

Damage in the flight view lands on the interior:

- **System damage.** Hits degrade the systems in the compartment struck —
  a breached engineering deck means degraded power; a hit near the scanner
  array blinds it. Damaged systems operate below capacity or not at all
  until repaired *at the system* by crew (or a repair automation).
- **Fires.** Bad hits (or overloaded systems) start compartment fires that
  spread room-to-room through open doors, damage systems in the room, and
  hurt biological crew. Fighting a fire is a crew task (extinguisher /
  sealing the compartment / venting it — venting a zero-g compartment is
  fast and brutal and ruins anything unsecured in it).
- **Power allocation.** Engineering's power station divides reactor output
  among systems (FTL charge, weapons, engines, gravity, life support…).
  Crises force triage: divert power from the guns to keep the med bay up,
  or the reverse. Diverting power *is* the "stop what you're doing" moment
  — the miner's beam dies because engineering needed its power for the
  point-defense.
- **Crew interruption.** Fires and breakages pull crew (and automation) off
  their stations. A ship on fire is a ship not shooting back. This is the
  Among Us / Sea of Thieves loop: run, fix, get back to your post.

## 5. Gravity and movement

Ships can have mixed gravity profiles (power-hungry artificial gravity, per
`docs/LORE.md` §III). On the Loup-Garou:

- **Upstairs (zero-g):** cockpit, tech bay/processing, shuttle pad. Humans
  need mag boots / flight suits to move — slower, deliberate movement. A
  character without them can get stuck floating; Boris will come rescue
  them, with commentary (robot humor: vague and extremely precise at the
  same time).
- **Downstairs (gravity):** bridge, engineering, med bay, cryo, quarters,
  galley. Normal movement.
- **Robots are built heavy:** fastest movers in zero-g, slow under gravity.
  Boris prefers Upstairs. Androids are human-baseline in both.

Movement speed is therefore a property of (body kind × deck gravity):

| | Gravity deck | Zero-g deck |
| --- | --- | --- |
| Human | 1.0 | ~0.7 (mag boots) |
| Android | 1.0 | 1.0 |
| Robot | ~0.5 | ~1.6 |

## 6. The Loup-Garou layout (canonical, authored)

Two decks joined by a ladder (a hatch and a ladder — the threshold between
the work and the life, per lore). Fore is up.

```
UPPER DECK (zero-g)                LOWER DECK (artificial gravity)
┌────────────────┐                 ┌────────────────┐
│    COCKPIT     │  fore           │     BRIDGE     │  tactical · nav · comms
└───────┬────────┘                 └───────┬────────┘
        │ spine                   ┌────────┼────────┐
        │  [ladder]               │SCANNER │ MED BAY│
┌───────┴────────────┐            ├────────┤(ladder)├──┐
│      TECH BAY      │            │ENGINEER│  CRYO  │  │ fusion core · FTL ·
│ processing · pad   │            │  -ING  │10 pods │  │ power allocation
│ shuttle docks here │            ├────────┼────────┤  │
└────────────────────┘            │QUARTERS│QUARTERS│◄─┘ shared + officers'
                                  ├────────┼────────┤    + guest berth
                                  │ GALLEY │AIRLOCK │  aft — board/disembark
                                  └────────┴────────┘
```

- **Cockpit** (upper, fore): the pilot seat — take the helm, get the Star
  Fox view. Prudence's domain.
- **Tech bay** (upper): the large processing room — raw materials broken
  down for transport — and the shuttle landing pad. Boris's territory.
- **Bridge** (lower, fore, under the cockpit): tactical, stellar nav, and
  comms stations.
- **Scanner array**: its own console in its own room.
- **Engineering**: fusion core, FTL engine control, power allocation, repair
  lab. Tove's, organized by a logic only she follows.
- **Med bay**: Doc Keene's. Trauma and surgery.
- **Cryo chamber**: 10 pods, one room. Where the jump loop ends and begins.
- **Quarters**: shared crew berths + officers' cabins + a guest berth.
- **Galley**: the social center. Bardo plays here.
- **Airlock** (aft): boarding and disembarking when docked.

## 7. Scaling the pattern

Every hull expresses the same grammar: an entry point (airlock/canopy), a
control position (seat/cockpit/bridge), systems rooms sized to the hull, and
crew accommodation matching endurance. A fighter collapses the whole grammar
into one cockpit (its "stations" are all within arm's reach — one operator,
hence its weaknesses). A capital ship multiplies rooms until internal travel
time itself becomes a coordination cost, and adds redundancy (two
engineering bays, port/starboard tactical) that keeps it fighting while
half-broken.

## 8. Implementation status

- [x] Authored two-deck Loup-Garou interior (`generator/ship.rs`), ladder
  deck transit, room-appropriate stations, cryo pod props, zero-g movement
  rules (S09c).
- [x] Station *views*, first slice (S09d): leaving the helm mid-flight (`B`)
  keeps the space scene alive and simulating under the interior
  (`SceneRegistry::space_alive`); opening the gunner/scanner/miner console
  in flight swaps the screen to the live flight scene with that console's
  overlay. The gunner gets the aiming reticle and the real trigger (F —
  bolts fly and land in the live world), the scanner gets the pulse (T),
  the miner runs the beam (G). Walking off the console (or Esc) returns to
  the interior; the ship coasts on momentum while nobody holds the stick.
  *Still to come:* turret aim independent of the nose + target lock (S19),
  scanner contact list, station operation by NPC/LLM crew (S13+), and
  multi-body play (a pilot *and* a gunner needs S23).
- [ ] Power allocation as a real constraint (engineering station budget
  feeding system effectiveness).
- [ ] Damage → compartment fires → spread/fight/vent loop.
- [x] The jump loop, first slice (S09e): `J` at the NAV console programs +
  arms a self-generated jump (destination derived from the seed protocol);
  a 30s window opens; every human crew member is auto-ordered to cryo and
  the player must physically reach a pod (`E` climbs in). Window opens
  with the player awake → the vascular/psych ruin is the death/respawn
  beat; a human crew member awake → a formative trauma soul event and a
  trust drop. Sleepers cross with **Prudence running the ship** (the
  dispatch routes the crossing to her; she considers it, per S15), the
  transit anomaly deliberates in her name, and revival wakes everyone in
  the cryo chamber — the walk back to the cockpit is part of arrival.
  Gate transits stay awake at the helm (lore: stable windows). The awake
  emergency self-jump (`J` in flight) still exists and now costs flesh.
  *Still to come:* wake conditions on the plan (fuel/hostile triggers),
  automation modules as the no-synthetic-crew fallback, cross-deck crew
  routing to pods (crew on the inactive deck still freeze), pod capacity
  as a hard constraint for passengers/prisoners.
- [ ] Automation modules occupying hull slots; slot/customization economy.
- [ ] LLM-driven crew operating stations (souls, S13) — including being
  trusted with the jump.
- [ ] Additional hull classes expressing the same grammar (fighter first —
  it's the degenerate case that proves the model).
