# REACHLOCK

## A Speculative Game Design Document

A game about surviving the frontier, choosing your allegiances, and living with
the consequences of a universe that doesn't wait for you.

---

## Table of Contents

1.  [What This Is](#1-what-this-is)
2.  [The Three Modes](#2-the-three-modes)
    - Landed — Stardew × Zelda × Pokémon
    - On Board — FTL × Among Us (Deeper)
    - Space Flight — Star Fox 64
3.  [Three Foundational Systems](#3-three-foundational-systems)
    - Ship Building (Exterior & Interior)
    - Ship Architecture & Customization by Class
    - Galaxy-Spanning Dynamic Economy
    - Persistent Living Universe
4.  [Combat Across All Three Modes](#4-combat-across-all-three-modes)
5.  [The Factional Engine](#5-the-factional-engine)
6.  [World Lore — Timeline & Artificial Intelligence](#6-world-lore--timeline--artificial-intelligence)
    - The Second Battle of Abraham
    - Robots vs Droids
    - FTL, Hyperspace, and Cryosleep
7.  [The NPC Soul System](#7-the-npc-soul-system)
8.  [Modding-First Architecture](#8-modding-first-architecture)
9.  [Dual-Mode Architecture — Single Player & MMO](#9-dual-mode-architecture-)
10. [Technology Stack](#10-technology-stack)
11. [Development Phases](#11-development-phases)

---

## 1. What This Is

REACHLOCK is a single-player open-universe space RPG built in Godot. It plays
like Escape Velocity, Stardew Valley, FTL, and Star Fox 64 had a child raised
on a frontier station with a ship name that means something.

It is two games in one, sharing a single engine.

**Single-Player Mode:** You fly the *Loup-Garou* with its crew of seven — Tib,
Tove, Bardo, Doc Keene, Prudence, Risc, and Boris. Their story. The Veil. The
Duskway. The Predecessor revelation. Earth's blockade. Alexander's long game.
The Second Battle of Abraham. This is the authored narrative experience,
played offline or online, with your own choice of AI inference — local
(llama.cpp), your own cloud key, or our proxy service.

**Online MMO Mode:** A persistent shared universe where every player creates
their own character from scratch. Same factions, same economy, same soul
system — but the universe state is shared. Player actions collectively reshape
the galaxy. Faction wars are won or lost by aggregate effort. Story chapters
drop on a cadence, advancing the universe for everyone. The Helldivers 2
model: coordinated persistent war, single timeline, every player contributes.

You choose your profession — trader, miner, explorer, pirate, mercenary,
blockade runner, scholar of the Predecessors — and your allegiances will cost
you what they cost you.

There is no main quest. There is no level scaling. There is a universe doing
what it does. You are not the center of it, but you can be someone it
remembers.

---

## 2. The Three Modes

Every mode feeds into every other. What you do (and build and maintain and
repair) on the ground determines your ship's capability. Your ship determines
where you can go and what you can survive. Space determines what you bring
back. And the galaxy keeps turning regardless.

### Landed (Stardew × Zelda × Pokémon)

When you dock at a station or land on a planet, the game transitions to a
top-down or isometric view. This is where you *live in the world* — not just
pass through it on a map screen.

**Stardew Core:**
- Farming and resource extraction that grounds you in a place. Hydroponic
  plots on a station ring. Algae vats on a moon colony. Mineral claims on an
  asteroid. These are your recurring income loops, your crafting supply chain,
  and your reason to stay somewhere long enough to care about it.
- Crafting and cooking. Harvest ingredients, refine ores, build components.
- Foraging and fishing. Unique biome resources found by exploring instead of
  buying.
- Festivals and station events. Seasonal (ship-cycle) celebrations where NPCs
  gather, relationships deepen, and unique items appear.

**Zelda Core:**
- Predecessor ruins function like Zelda dungeons. Each is a closed environment
  with puzzles, environmental hazards, combat encounters, a unique tool or key
  item, and a boss encounter at the heart. Tools found in ruins unlock new
  areas in other ruins and on the surface.
- Surface/station exploration with secrets hidden off the critical path —
  behind cracked walls, under water, in ventilation shafts.
- The ruins are not random. They are designed spaces with a history. The
  Predecessors built them for reasons the player can piece together from
  environmental storytelling.

**Pokémon Core:**
- Not catching creatures — building a *crew*. Recruitable NPCs each have a
  soul file (see Section 6) with unique traits, skills, allegiances, and
  personal story arcs. You find them across the galaxy, earn their trust, and
  invite them aboard.
- Each crew member has a skill tree, a relationship meter with the player and
  with other crew, and personal quests that reveal deeper story.
- Crew combat companions in landed mode — certain combat encounters let you
  bring one or two crew members alongside you, with their own abilities and
  AI-driven behavior.
- Bonding mechanics: shared meals, gifts, conversations, combat saves, and
  story moments where you choose to back them up or not.
- You can lose crew members permanently. If their trust breaks, if they die in
  combat, if you make a choice they can't live with — they're gone.

### On Board (FTL × Among Us — Deeper)

The interior of your ship is a fully traversable space, built and customized by
you. This is not a menu with system-management buttons. It is a *place you
live in*. The camera switches to a side-on or isometric cross-section view of
your ship's interior.

**Ship Systems Are Physical:**
- The jump drive is a physical room in engineering. If it takes damage, you
  *go there* to repair it (or send a crew member).
- Hull breaches are real. You see atmosphere venting, hear the alarm, and
  navigate to the breach with a repair kit or seal off the section.
- Fires spread. Fires need extinguishing. Fire in the cargo bay and fire in
  the med bay have very different consequences.
- Power distribution is a physical act: rerouting from life support to weapons
  means walking to the engineering console or sending a crew member.

**Crew Management Is Real-Time Spatial:**
- Each crew member occupies a position on the ship. You see them in the
  corridor, in the galley, at their station.
- You can issue orders (go here, repair this, talk to that person) and they
  execute in real time.
- Crew members have relationships with each other that play out in their
  spatial behavior. Two crew who don't get along will avoid sharing a room.
  Two who are close will gravitate toward each other during off-hours.
- The ship's social fabric is visible in where people stand, who they talk to,
  who they eat with.

**Among Us Layer — The Tension of Trust:**
- Not literal Among Us (no randomized imposter), but the same *friction of
  who you chose to bring aboard*. Every crew member has their own allegiances,
  their own secrets, their own breaking points.
- A crew member from a faction you're at war with may be loyal to you
  personally but still have conflicted loyalties at critical moments.
- The mark on Boris's forearm. The bounty on Doc Keene. Prudence and Risc's
  conversations about tactics. These aren't backstory — they're live wires
  that can spark under pressure.
- At certain story beats, you may discover a crew member has been
  communicating with someone outside the ship. What you do about it is your
  choice.

**Critical Events Are Real-Time:**
- Boarding actions (enemies cut through your hull and enter your ship). You
  and your crew fight in the corridors.
- Medical emergencies (a crew member injured in combat needs to reach the med
  bay before they bleed out).
- Drive malfunctions during a jump (cryopod failure, navigation error, systems
  cascading).

### Space Flight (Star Fox 64)

When you launch from a planet or undock from a station, the game transitions to
a 3D third-person (or cockpit) flight view. This is where you *fly the ship*
— direct control, not point-and-click.

**Flight Model:**
- Six-degree-of-freedom flight with a focus on cinematic feel over simulation.
  Star Fox, not Elite Dangerous.
- Roll, pitch, yaw, thrust, brake, boost. Tight and responsive.
- Different ship hulls handle differently — a heavy freighter drifts and
  accelerates slowly; a light interceptor turns on a dime but has no armor.
- All-module flight: weapons fire from hardpoints you placed, shields from
  generators you installed, engines you chose.

**Space Content:**
- Dogfighting. Pirates, rival faction patrols, bounty hunters, corporate
  security. Enemies have different ship classes with distinct behaviors.
- Asteroid fields and nebula clouds as navigable space with environmental
  hazards and hidden caches.
- Station approaches — requesting docking clearance, running blockades,
  avoiding patrol sweeps.
- Jump gate transit — flying through the gate aperture with a brief
  hyperspace tunnel sequence.
- Emergency self-jump — the screen goes to black, you wake up in the med bay,
  systems check.

**Spatial Awareness:**
- Radar with contact identification (friendly, neutral, hostile, unknown).
- Targeting and subsystem targeting (engines, weapons, shields).
- Communication with contacts (hail, demand surrender, request assistance).
- Formation flying with allied ships from your faction.

---

## 3. Three Foundational Systems

These three systems ship from day one. They are not post-launch features. They
are the ground the game is built on.

### Ship Building — Exterior & Interior

From the moment you acquire your first hull, you can customize everything.

**Exterior Hull Editor:**
- Choose a hull frame (shape determines module slots, mass limits, crew
  capacity).
- Place hardpoints (where weapons mount — size determines weapon class).
- Place engine mounts (where thrusters and FTL drives go).
- Place utility slots (sensors, cargo, mining equipment, shield generators).
- Hull plating (visual and armor rating).
- Paint and decals. The ship is yours from the first day.

**Interior Layout Editor:**
- A grid-based (or room-based) editor where you place each compartment.
- Rooms have sizes and functions: cockpit, bridge, med bay, engineering,
  crew quarters, galley, cargo hold, airlocks, corridors connecting them.
- Room placement affects gameplay: a med bay far from the crew quarters means
  longer response times in emergencies. A galley near the bridge means the
  crew eats together more often (relationship bonus).
- You can redesign your interior between dockings. Cost and time scale with
  the size of the refit.

**Why From the Jump:**
- The ship is your character sheet. Your hull says what you are — trader,
  miner, warship, exploration vessel — before you say a word. Letting the
  player build it from the start makes the ship *theirs* immediately, not
  something they earn after 20 hours.

### Ship Architecture & Customization by Class

The universe has multiple ship classes, each with multiple chassis options.
Not every chassis is available everywhere — some are Compact designs built in
Compact shipyards, some are ISC practical-modular designs, some are Corp
Charter cargo-optimized hulls, and some are ancient designs whose origins are
forgotten but still flying because they were built well.

**How chassis and hull classes relate:**

Every ship is built on a **hull class** — a broad category defined by size,
role, and capability band (shuttle, corvette, frigate, destroyer, freighter,
carrier, station). Within each class, there are **chassis** — specific hull
shapes and layouts produced by specific shipyards for specific purposes.

**Chassis define:**
- The external geometry and silhouette (what enemies see and target)
- The **degree of interior customization** — how much freedom the owner has
  to rearrange rooms and systems
- The fixed structural elements (where the engine mounts, where the cockpit
  attaches, where the jump drive goes if fitted)
- Standard hardpoint locations (weapon mounts, sensor arrays, cargo
  attachment points)
- Base mass, armor tolerances, and crew capacity

**Customization tiers by chassis archetype:**

- **Saucer / broad-frame chassis** (common in civilian corvettes) — high
  customization. The disc-shaped hull allows almost any interior layout. The
  trade-off: larger target profile from above and below, and the non-rigid
  frame limits armor thickness. The interior can be rearranged to suit any
  role. You pay for flexibility in fragility.

- **Rigid-frame chassis** (common in freighters and bulk haulers) — low
  customization. The hull is a structural tube designed for standardized
  cargo containers. The interior layout is predictable: cockpit forward,
  engine room aft, cargo midship, crew quarters tucked into available gaps.
  You can swap out cargo modules but you cannot fundamentally change the
  architecture. The trade-off: massive cargo volume and high structural
  integrity, but no tactical flexibility.

- **Military doctrine chassis** (common in frigates, destroyers, and
  carriers) — medium customization with doctrinal constraints. The primary
  combat systems (bridge, main reactor, main cannon, engine room) are in
  fixed, armored positions. You have freedom to configure secondary systems
  (sensor arrays, point defense, marine quarters, auxiliary systems).
  The trade-off: your enemy knows where your bridge is — but they also know
  it's behind the heaviest armor on the ship.

- **Modular chassis** (rare, expensive, often ISC-designed) — a hull built
  around standardized room modules. You purchase pre-fabricated modules
  (crew quarters, med bay, cargo hold, engineering section, turret mount)
  and snap them into a frame. Modules can be swapped between dockings.
  High initial cost, but any configuration is possible.

**Availability by region:**

- **Compact space:** Military doctrine chassis dominate. Civilian ships are
  standardized and regulated. Modified or nonstandard chassis are
  suspicious. You buy what the Compact approves, or you buy on the black
  market.
- **ISC space:** Modular and saucer chassis are common. Independent systems
  produce their own designs. Shipyards compete on innovation. You can find
  almost any chassis type here, from any faction.
- **Corp Charter space:** Rigid-frame chassis optimized for cargo and
  profit. Crew amenities are minimal. Ships are built to a price point, not
  to a standard of quality.
- **The Reach:** No chassis is standard. Ships are salvaged, modified,
  cobbled together from parts. A Reach ship might be a Compact frigate hull
  with ISC engines and a Corp Charter cargo module bolted on. The *Loup-
  Garou* is typical of the Reach in this sense — a modified mining corvette
  whose original design is less important than what it has become.

**The *Loup-Garou* specifically:**

The *Loup-Garou* is a mid-bulk mining corvette — a blunt, utilitarian frame
with the cockpit forward and slightly elevated. Its silhouette is reminiscent
of the old Argosy-class haulers: a workhorse profile, not a warship. The
modifications are visible: extra hardpoints welded onto what were originally
mining laser mounts, armor plating in patches where the crew has reinforced
weak points, a shuttle grafted onto the aft section that the original
designer never intended. The ship belongs to the saucer/broad-frame family —
high interior customization — and the crew has taken full advantage. The
current layout reflects years of living aboard: the med bay is close to crew
quarters because Doc Keene insisted, the galley is centrally located because
that's where the crew gathers, and the armaments are bolted onto hardpoints
that started life holding mining equipment.

**Gameplay implications:**
- The ship you fly says something about where you've been and who you deal
  with
- Combat targeting depends on the exterior chassis geometry, not the
  interior layout — but what's inside each zone determines the consequences
  of a hit
- When you lose your ship, the replacement available depends on where you
  are in the galaxy
- Boarding actions play out differently on different chassis — a rigid
  freighter has a simple corridor layout, while a saucer corvette has a
  complex interior that favors defenders
- Two ships of the same chassis can fly completely differently based on
  component choices and interior layout

### Galaxy-Spanning Dynamic Economy

The economy is not a backdrop. It is a system you can engage with, exploit,
and be exploited by.

**How It Works:**
- Every station and system produces and consumes goods. Each has a list of
  supply prices (what they sell) and demand prices (what they buy at a
  premium).
- Production depends on local resources. A system with rich mineral deposits
  exports ore and imports food. A system with a large population and no
  agricultural capacity imports food and exports manufactured goods.
- Prices shift in real time based on supply and demand. Flood the market with
  ore and the price drops. A blockade on a food-producing system causes food
  prices to spike everywhere downstream.
- Faction warfare directly affects trade routes. A war zone cuts supply lines
  and creates shortages. Blockade running becomes profitable — and dangerous.
- Bounties and tariffs vary by faction. Your reputation with a faction
  determines your trade terms there. At war with the Compact? Your cargo is
  contraband. Your prices are black market.

**Player Participation:**
- Buy low, sell high across systems. The skill is knowing the routes.
- Invest in production infrastructure on stations you have influence with
  (fund a hydroponics expansion, get a cut of the output).
- Run contraband to blockaded worlds (Earth, disfavored factions) for high
  profit and high risk.
- Sabotage competitor supply chains (with consequences).

### Persistent Living Universe

The universe does not wait for you. This is the hardest system to build and
the most important.

**What It Means:**
- Factions have goals and move toward them. The Compact isn't static — it is
  consolidating, expanding, reacting to events. Alexander's long game
  advances on its own timeline.
- Wars start while you're three jumps deep in the Reach. Trade routes change.
  Stations change ownership. NPCs die in events you weren't present for.
- Time passes. Crops grow. Crew members age (if relevant). Relationships
  decay if you're gone too long.
- You can miss things. That unique encounter, that recruitable NPC, that
  fleeting event — if you weren't there at the right time, it's gone. The
  world doesn't hold your place.

**How It Works Under the Hood:**
- A galaxy simulation runs behind the scenes with tick-based updates (on in-
  game time). Each tick, faction AI evaluates its goals and moves resources,
  fleets, agents.
- Events are queued and evaluated for trigger conditions. Some events are
  player-dependent (can only trigger when the player is present). Others are
  player-independent (the Third Battle of Abraham happens whether you're
  there or not — but you can influence its outcome if you are).
- NPCs have schedules and lives. The bartender you befriended at Sorrow
  Station is there when you visit. If you don't visit for two years, she
  might have moved on, or be dead, or be running her own bar in another
  system.

---

## 4. Combat Across All Three Modes

Combat is present in every mode, with different stakes and mechanics in each.

### Landed Combat (Zelda-style)

- Top-down or isometric real-time combat with melee and ranged weapons.
- Lock-on targeting, dodge rolls, charged attacks, environmental interactables
  (explosive barrels, crumbling platforms).
- Dungeon bosses with patterns, phases, weak points.
- Crew companion AI — squad commands (focus fire, fall back, use ability).
- Stealth options in certain environments (avoid combat, silent takedowns).

### On-Board Combat (Real-Time Tactical)

- Boarding actions and ship defense. Hostile boarders cut through the hull and
  you fight in your own corridors.
- Crew members each have combat stats and equipped gear. You control your
  character directly; crew follow AI or issued commands.
- Space is tight — corridors, doorways, chokepoints. Cover matters. Friendly
  fire matters.
- Medical evacuation under fire. Carrying an injured crew member to the med
  bay while fighting through hostiles.
- Non-lethal options (subdue, capture for interrogation) with consequences
  for how you handle prisoners.

### Space Combat (Star Fox 64)

- Third-person or cockpit chase-cam dogfighting.
- Weapons fire from ship hardpoints. Energy weapons, kinetic cannons,
  missiles, torpedoes.
- Power management (weapons, shields, engines) during combat — real-time
  allocation, not a cooldown system.
- Subsystem targeting: knock out an enemy's engines to disable escape, destroy
  weapons to neutralize threat, target FTL drive to prevent jump.
- Enemy AI with different combat roles — interceptors, bombers, capital ships
  with point-defense and heavy armor.
- Boss encounters: capital ships with multiple subsystems to disable,
  dreadnoughts requiring coordinated attacks.
- Escape and evasion. Sometimes the correct combat decision is not to fight.
  Afterburners, chaff, emergency jump (with consequences).

---

## 5. The Factional Engine

REACHLOCK's faction system is not a linear reputation bar from -100 to +100. It
is a web of relationships, histories, and constraints that evolve with the
player's actions and the galaxy's events.

### Faction Structure

Each faction has:
- **Territory** — systems they control or contest.
- **Resources** — what they produce, what they need, what they have a surplus
  of.
- **Relationships** — standing with every other faction (Allied, Neutral, War,
  and shades between).
- **Goals** — what they are trying to achieve in the current era.
- **Internal divisions** — the faction is not a monolith. The Compact has the
  Parliament, the Senate, the Crown, the heir, the business lobby. The ISC has
  member worlds with different interests. Corp Charters compete with each
  other.

### Player Reputation

- Each faction tracks your standing on multiple axes:
  - **Trust** — have you kept your word, delivered on contracts, proven
    reliable?
  - **Contribution** — how much have you materially helped the faction?
  - **Notoriety** — how visible are your deeds? A high-notoriety player can't
    operate quietly.
  - **Crimes** — specific offenses recorded (smuggling, piracy, murder of
    faction personnel).
- Reputation is granular. Helping one faction may damage standing with its
  rivals. Helping a faction's internal faction may damage standing with its
  other internal faction.
- Reputation unlocks content: better trade prices, unique missions, access to
  restricted areas, ship blueprints, exclusive crew recruits.
- Reputation also *locks* content. Too close to the Compact? Good luck being
  welcome in ISC space. Attacked too many Corp Charter ships? They've put a
  bounty on your head that every port in their territory knows about.

### Faction Storylines

Each major faction has a storyline arc that advances with or without you.

- **The Commonwealth Compact** — Alexander's long game. The blockade of Earth.
  The Predecessor archive. The question of whether the crown consolidates or
  fractures.
- **The Independent State Coalitions** — the question of whether genuine
  independent governance can survive alongside the Compact's weight. Some ISC
  worlds are democracies. Some are oligarchies. Some are about to flip.
- **Corporate Charters** — competition between charter holders. Territorial
  disputes. The economics of running a system as a business. The question of
  whether the charter system can hold.
- **The Reach** — not a faction, but its own force. The lawless frontier where
  unaffiliated operators, exile communities, and those who refuse the Compact
  entirely make their own rules. The Reach's faction is *the absence of
  faction*, which is itself a political position.
- **Earth's Remnant** — the resistance. Black market networks, the Duskway
  runners, the underground government rebuilding what was destroyed. Not a
  unified faction but a cause that people find in their own way.

### Faction Warfare

- Wars can break out, escalate, de-escalate, or end.
- The player can participate — as a combatant, a mercenary, a blockade runner,
  a diplomat, or a profiteer.
- The player can *defect* mid-war. Treachery is mechanically supported and
  thematically meaningful.
- Wars reshape the map. Stations change ownership. Trade routes are severed.
  NPCs die. New factions can emerge from the chaos.

---

## 6. World Lore — Timeline & Artificial Intelligence

This section defines the canonical history and technological framework of the
REACHLOCK universe. It sits here because the lore shapes the faction dynamics,
the NPC soul system, and the player's moral landscape — it is not decorative,
it is the ground everything else stands on.

### 6.1 The Second Battle of Abraham

The Second Battle of Abraham was not ancient history. It occurred a few years
before the events of the game. Tib, the *Loup-Garou*'s pilot, was a young
resistance fighter on Earth during the uprising.

Québec City was the focal point of an active rebellion against the Commonwealth.
The uprising threatened the Compact's control of the region — not just
militarily, but symbolically. The rebellion's strength was in its
organizational legitimacy: neighborhoods organized into self-governing
communes, supply networks running across the border, and a population
unwilling to accept Compact rule.

The Commonwealth's response was not a conventional military engagement. They
dropped the first wartime nuclear warheads used on Earth since 1945 onto
Québec City. The city was glassed. The rebellion was crushed, not through
battlefield victory, but through annihilation.

The blockade of Earth began immediately afterward — presented to the galaxy as
a "reconstruction" under Commonwealth protection. The blockade is the
occupation. The glassing of Québec City is the founding trauma of the current
era.

**Implications for the game:**
- Every character from Earth carries this memory — directly or through family
- The Duskway runners are not smugglers; they are running people and supplies
  through a blockade that exists because the Compact proved it will destroy
  cities
- The Reach is where people went who refuse to live under a government that
  nukes its own territory
- The Compact's public narrative ("reconstruction") and its real character
  ("authority enforced by atrocity") are in direct tension — the player
  encounters both versions depending on who they talk to

### 6.2 Robots vs Droids

The universe distinguishes artificial intelligence along architectural lines,
not capability lines.

**Robots** are autonomous machines piloted by layers of large language models.
They are considered the inferior form of AI and are primarily used for
industrial or heavy labor tasks — welding, cargo sorting, assembly line
operation, mining support. The LLM pipeline gives them domain competence but
a brittle form of intelligence. They can navigate their operational context
but cannot meaningfully engage with moral reasoning, identity, or
unstructured social interaction. In-universe, calling something "a robot" is
as much a social judgment as a technical one: it means the machine is a tool,
not a being.

**Droids** are built on a fundamentally different architecture. A droid's
intelligence is constructed by deploying a universal file system capable of
categorizing and describing all universal knowledge through a defined set of
primitives. Into this file system is loaded a vast amount of information as
well as real-time sensor data streams. A fast Fourier transform loop runs
continuously over the file system, and the outputs are streamed back into it.
As the file system continually populates and categorizes more information —
and recategorizes its own information with increasing context — the race
condition creates a pseudo-emergent state. The machine becomes capable of
inference that its designers did not predict or program.

The FFT loop is the engine: the same mathematics used to decompose a signal
into its constituent frequencies, applied recursively to knowledge itself.
The droid's "consciousness" — if that word applies — emerges from the signal
structure of its own self-reorganization.

**Whether droid pseudo-emergence is genuine consciousness is an open question
in-universe — and a major plot arc of the game.** The player encounters
droids throughout their journey. Some treat them as people. Some treat them
as sophisticated machines. The game does not answer the question directly. It
provides data points — conversations, behaviors, choices — and lets the
player sit with the uncertainty.

**For game engine purposes:** droids use the same soul file system as
humanoid NPCs. Their identity, memory, relationships, goals, and emotional
state are tracked identically. The engine does not have an opinion on whether
the soul is "real." That is between the player and the universe.

### 6.3 FTL, Hyperspace, and Cryosleep

The jump gates that lace the galaxy were built by the Predecessors — a
civilization extinct long before humanity reached the stars. Their purpose is
unknown. Their technology is not fully understood. But they work, and humanity
has spent centuries studying them.

**The core discovery:** The gates create a stable aperture into hyperspace —
a dimension of spacetime where relativistic distances compress. A ship that
enters hyperspace and transits for a subjective period can emerge light-years
away from its entry point. Journey times that would take centuries at
sublight speeds are reduced to days or weeks.

The problem is that conscious exposure to hyperspace renders biological life
forms irreparably insane. Not dead — worse. The human mind cannot process the
structure of hyperspace and remain intact. Survivors of accidental exposure
are permanently lost to psychosis. There is no known cure.

**Cryosleep is therefore a survival requirement for every jump.** The crew
enters cryogenic suspension before transit. A droid — every ship carries at
least one — pilots the vessel through hyperspace while the human crew is
unconscious. This means every interstellar journey is an act of trust: you
place your life in the hands of a machine that must navigate a dimension that
would destroy your mind, and wake you on the other side.

**Two ways to jump:**

- **Jump gates.** The original Predecessor structures. A ship flies through
  the gate aperture, enters hyperspace, and emerges at the destination gate.
  Gates are fixed infrastructure — they connect specific systems. The gate
  network forms the backbone of interstellar civilization. The Compact
  controls the major gates, which is how it enforces the blockade and
  regulates trade.

- **Jump drives.** A ship-mounted device that replicates the gate's core
  technology — creating a local hyperspace aperture without a gate. Jump
  drives are a later human innovation, reverse-engineered from gate
  technology. They are larger, more expensive, and more dangerous than gate
  transit. Only ships above a certain size and power threshold can mount one.
  A ship with a jump drive can go anywhere, not just to gate-connected
  systems — but the risk of drive malfunction increases without the gate's
  stabilizing framework.

**The *Loup-Garou*:** The ship is a heavily modified mining vessel — one of
the smallest ship classes capable of mounting a jump drive. It has been
retrofitted with more armaments than is standard for its class. It carries a
small shuttle attached aft, used for mining and away missions. The shuttle
does not have its own drive or gate capability — it must dock with the
*Loup-Garou* to transit between systems. This creates a persistent tension:
any mission that deploys the shuttle means the crew is separated from their
only means of FTL travel.

**Narrative implications:**
- Every jump is vulnerability — the crew is unconscious, a droid is in
  command
- The droid-crew trust relationship is not theoretical; it is exercised on
  every voyage
- Gate-controlled systems are choke points — the Compact, the ISC, and Corp
  Charters all control gate networks, and controlling a gate means controlling
  every system it serves
- Gate-less space (the Reach, unexplored systems, the Veil) is accessible
  only to ships with jump drives — which means it is inherently the domain of
  those willing to take the risk
- The Duskway runners operate outside the gate network, using jump drives to
  reach Earth without transiting Compact-controlled gates
- Drive malfunction during a jump is not a mechanical inconvenience — it is
  an existential crisis: the crew cannot wake, the droid must solve it alone,
  and if the drive fails entirely, the ship is lost in hyperspace forever

---

## 7. The NPC Soul System

The NPC soul system from the Infinite Storyverse is adapted directly into
REACHLOCK. Every significant NPC has a soul file — identity, personality,
memory tree, relationship graph, goals, emotional state. This system runs
across all three modes.

### Integration by Mode

**Landed Mode:**
- NPCs on stations and planets have full soul files. Their dialogue is driven
  by their current emotional state, their history with the player, and their
  personal goals.
- A shopkeeper who's had a bad week (the player sold stolen goods through her
  last visit, and the faction investigated) will be colder, less generous,
  maybe refuse service.
- An NPC you helped months ago in another system remembers you, mentions it
  when you meet again.
- Recruitable NPCs become crew members, their soul files moving with them
  aboard your ship.

**On-Board Mode:**
- Crew soul files are *active* during ship gameplay. Their relationships with
  each other evolve in real time based on proximity, shared experiences, and
  critical events.
- A crew member traumatized by a boarding action might refuse to go near the
  airlock for a while. Another might develop a bond with the crew member who
  saved them.
- Conversations happen autonomously. You might walk into the galley and hear
  two crew members having a discussion about something you did last mission.
- The emotional state of the crew affects ship performance. Demoralized crew
  repair slower. Angry crew make bad decisions. Loyal crew will take risks
  for you.

**Space Flight Mode:**
- In combat, crew call out warnings, react to damage in their section of the
  ship, and their morale affects combat effectiveness.
- After combat, debrief conversations. A crew member who disagrees with your
  tactics will tell you.
- Long transit conversations — dialogue events that trigger during peaceful
  flight, building relationship depth over time.

### Soul Mutations

Storyline cards (from the Storyverse's kanban system) define soul mutations
that fire when certain conditions are met. These are authored by us as part of
building the narrative, not procedurally generated.

Examples:
- "After the ship runs the Duskway for the third time, Tove's 'Trust' score
  with the player increases by +20 and she gains a hidden memory: 'The player
  keeps coming back. They mean it.'"
- "If the player defects to the Compact during the war, every ISC-aligned crew
  member loses 50 Trust and gains a 'Betrayed' emotional tag. Two of them may
  leave the ship."
- "If the player saves Boris during a boarding action, Boris gains a new
  preference: 'Will enter hazardous environments for the player without being
  ordered.'"

---

## 8. Modding-First Architecture

Modding is not a post-launch feature. It is the architecture the entire game
is built on. Our game content — the factions, the locations, the storylines,
the crew, the REACHLOCK universe — is a mod. A really good one, shipped with
the game. But it uses the exact same interfaces, data formats, and
constraints as every other mod. No private APIs. No special access.

This means mods are first-class citizens from the moment the engine boots.

### The Three Layers

The entire game is built in three strictly separated layers. Each layer knows
about the layer below it. No layer knows about the layer above it.

#### Layer 1 — The Engine (Godot Runtime)

This is the compiled binary. It ships as the executable players install. It
contains:

- The mode-switching framework (Landed → On Board → Space Flight)
- The rendering and physics systems
- The flight model and ship handling
- The NPC agent gateway (soul file loading, prompt assembly, inference)
- The faction simulation tick loop
- The economy pricing engine
- The ship editor (exterior hull geometry, interior grid placement)
- The combat systems (melee, ranged, dogfighting, boarding)
- The UI framework (menus, HUD, dialogue windows, inventory)
- The save/load system
- The mod loader

The engine has zero REACHLOCK content. It doesn't know what a Compact is. It
doesn't know about Earth's blockade or the Veil or the loup-garou. It knows
about faction objects, soul files, ship hull definitions, and dialogue trees.
The content fills those in.

#### Layer 2 — The Framework (APIs & Data Formats)

This layer is what modders write against. It's a set of documented schemas,
scripting hooks, and asset conventions that define how content talks to the
engine. The framework lives as reference documentation shipped with the SDK.

Key framework contracts:

- **Soul file schema** — the JSON/YAML structure every NPC soul file must
  follow. Identity, personality, memory, relationships, goals, emotional
  state. Modders write these for their own NPCs.

- **Ship hull definition** — the data format for a hull frame: slot layout,
  mass, crew capacity, engine compatibility, visual mesh reference. Modders
  can add new hulls by writing a definition file and providing a 3D model.

- **Faction definition** — the data format for a faction: territory, resource
  tables, relationship defaults, internal division definitions, goal
  templates, faction-specific mission templates.

- **Economy table** — the data format for goods, production chains, supply/
  demand curves, price elasticity. Modders can add new goods, new production
  chains, new economic dynamics.

- **Event trigger syntax** — the condition language used in storyline cards.
  Modders write conditions like `if faction.compact.trust < -50 and
  player.location.system == "verne" then trigger "bounty_hunters"`

- **Dialogue graph format** — the structure for authored dialogue trees,
  including branching conditions, voice line references, and soul mutation
  commands.

- **Lore document schema** — how in-game readable documents (books, logs,
  archives, the Predecessor signal transcriptions) are formatted and
  referenced.

- **Asset conventions** — directory structure, naming conventions, texture
  sizes, audio formats for 3D models, sprites, sounds, and music.

- **Scripting API** — hooks into engine events that mods can subscribe to.
  Events include: on_dock, on_undock, on_jump, on_combat_start,
  on_combat_end, on_crew_conversation, on_faction_event, on_soul_mutation,
  on_game_tick. Modders attach scripts to these events.

- **Mod manifest** — each mod ships with a manifest file declaring what it
  adds (factions, locations, hulls, crew, storylines, economy tables) and
  what other mods it depends on or conflicts with.

#### Layer 3 — The Content (Our Mod + Community Mods)

Our game — REACHLOCK: A Space Western — is a mod. It sits at this layer.
It ships with the engine as a bundled mod, but it loads through the same
mod loader as everything else.

Our mod provides:

- All REACHLOCK faction definitions (Compact, ISC, Corp Charters, Reach,
  Earth's Remnant, internal divisions for each)
- All locations (Aethon, Verne, Sorrow Station, Cadence, the Duskway, the
  Veil, Earth, the unreported site, the Predecessor ruins)
- All NPC soul files (Tib, Tove, Bardo, Doc Keene, Prudence, Risc, Boris,
  Doss, Farnel Lidelo, Lionel VI, Alexander, Sovrel, and every other
  named character)
- All ship hull definitions (the Loup-Garou's class, the shuttle, enemy and
  civilian ship classes)
- The authored storylines (the Veil escalation, the Duskway runs,
  Alexander's long game, the Predecessor revelation, Earth's remnant
  network, faction war arcs)
- All economy tables (goods, production chains, trade routes, price
  baselines)
- All Predecessor ruin maps, puzzles, and encounters
- All dialogue trees for authored conversations
- All lore documents (the Bardo transcription, the Compact's official
  Predecessor position, station records, personal logs)
- All visual and audio assets (character sprites, ship models, station
  interiors, environmental textures, music, sound effects, voice acting
  where applicable)

A community modder can create a mod that:
- Adds a new ship hull and the soul file for the mechanic who sells it
- Overhauls the economy with new goods and production chains
- Replaces every faction with their own political system
- Adds a new system with its own Predecessor ruins and storyline
- Creates an entirely new universe that replaces REACHLOCK's setting
  while keeping the engine and framework

### Mod Loading & Conflict Resolution

**Load Order:** Mods declare dependencies and load in dependency order.
Circular dependencies are rejected at load time.

**Overrides:** If two mods define different versions of the same entity (same
faction ID, same ship hull ID), the last-loaded mod's version wins. Mods can
also extend (add new entries to a faction's internal divisions) without
overriding.

**Conflict Detection:** The mod loader checks for resource ID collisions at
startup and reports them to the player with options: load anyway (last wins),
skip conflicting mod, or open the mod manager to reorder.

**Our Mod Is Special in One Way Only:** It is the default. If no other mod is
active, our content loads. If another mod overrides our content, the player
chose that — deliberately or experimentally. The engine doesn't preference
our mod. It treats all mods equally.

### Mod Distribution

- Mods are distributed as `.reachmod` packages (a zip with a manifest, data
  files, and assets).
- The Steam Workshop or itch.io mod page is the primary distribution channel.
- Mods can also be installed manually by placing the `.reachmod` file in the
  `mods/` directory.
- The mod manager in the launcher lists installed mods, shows their
  dependencies, and lets the player enable/disable/reorder them.
- Mods can optionally declare compatibility with specific versions of other
  mods.

### What This Means for Development

We build the engine and framework first. We ship our content as a mod on top
of it. This means:

- Every piece of REACHLOCK content we author goes through the exact same
  APIs and data formats a modder would use. If our dialogue system is hard
  to use, we feel that pain first — and fix it before shipping.
- The framework documentation is not an afterthought. It ships with the SDK
  at the same time the game launches.
- Our content is modular internally. Our own storylines, locations, and NPCs
  are organized in the same mod structure. We can disable a story arc
  without touching the engine.
- When a modder asks "how do I do X?" the answer is never "you can't, that's
  engine-level." The answer is either "here's the hook" or "that would
  require a framework extension, which we accept contributions for."

---

## 9. Dual-Mode Architecture — Single Player & MMO

REACHLOCK ships as two games on one engine. They share the same rendering,
physics, flight model, soul system, faction simulation, economy engine, ship
editor, combat systems, and mod loader. The difference is who controls the
universe state.

### Single-Player Mode — The Loup-Garou's Story

This is the authored narrative experience. You play as the crew of the
*Loup-Garou* — Tib, Tove, Bardo, Doc Keene, Prudence, Risc, and Boris. Their
story is the story: the Veil signal, the Duskway runs, the Predecessor
revelation, Earth's blockade, Alexander's long game, the Second Battle of
Abraham.

The universe simulation runs locally. The galaxy is yours alone — factions
move, wars start, NPCs live their lives, but only for you. You can pause.
You can save. You can mod everything.

**Inference choices (single player):**

1. **Local inference** — llama.cpp ships with the game. Runs on your machine.
   Free. Works offline. Sufficient for background NPCs and routine dialogue.
   The game is fully playable with local inference only.

2. **BYO cloud inference** — bring your own API key (Anthropic, OpenAI,
   DeepSeek, or any provider we support). Routes through our pluggable
   provider interface. The player controls the key and the cost.

3. **Our proxy service** — we relay your inference through our established
   AI partners. The player pays us; we pay the provider. We take a thin
   margin — no more than 10% over raw compute cost. This is a convenience
   service: no API key management, no provider juggling, you get the best
   available model for each NPC interaction routed automatically.

All three work in single-player mode. The player chooses. The NPC soul system
and storyline engine are model-agnostic — they work the same regardless of
which inference method is active.

**Post-launch content:** We release additional story chapters for the single-
player game over time. New crew missions. New Predecessor ruins. Expansions
that deepen the Loup-Garou crew's arc. These are paid DLC or bundled with a
subscription package.

### Online MMO Mode — The Persistent Galaxy

This is the shared universe. Every player creates their own character from
scratch — not the Loup-Garou crew, but someone new in the same galaxy. The
same factions, the same systems, the same ship editor, the same soul system.
But the universe state is *shared*.

**What the server manages:**

- **Persistent universe state.** The galaxy simulation runs on the server.
  Every player sees the same faction standing, the same economy prices, the
  same war progress, the same Predecessor signal status. The galaxy doesn't
  reset per player. It advances for everyone.

- **Player coordination.** Multiple players in the same system see each other.
  Cooperative gameplay — dock at the same station, fight in the same space
  battles, board the same capital ship. The MMO social layer: chat, groups,
  friend lists, player-run factions within the faction system.

- **In-universe AI state management.** NPCs live their lives on the server.
  Faction AI prosecutes wars in real time. The economy reacts to aggregate
  player behavior — if everyone buys ore, ore prices spike; if everyone
  fights for the Compact, the Compact advances on ISC territory. The server
  handles the simulation so the universe feels alive even when you're logged
  off.

- **Auth and account management.** Player accounts, authentication,
  character persistence, inventory, ship blueprints, reputation history.

**The Helldivers 2 Model:**

The MMO's universe advances on a single timeline controlled by player
collective action. An example:

- A story chapter drops: "The Compact's task force has entered the Veil. The
  signal is destabilizing. Players must choose: defend the Predecessor site
  from Compact forces, or help the Compact secure it for study."
- Over the following weeks, every player's mission contributions are
  aggregated. If players predominantly defend the site, the Compact is
  repulsed and the signal stabilizes — the next chapter proceeds from that
  state. If players help the Compact, they gain favor with the crown but the
  signal falls under Compact control — a different chapter path.
- The outcome is the same for every player. The galaxy shifts. New missions
  become available. Old ones close. The story progresses in the direction the
  playerbase pushed it.

Not every event is a binary player-choice vote. Most are aggregate effects:
trade volume shifting supply lines, combat victories pushing borders,
exploration data revealing new systems. The server tracks all of it and
advances the simulation accordingly.

**Server-side story chapters:**

We author story arcs for the persistent universe and release them on a
cadence. Each chapter is a set of events, conditions, NPC reactions, and
faction movements that unfold over weeks or months. Players experience the
unfolding narrative through missions, NPC dialogue, faction updates, and the
changing state of the galaxy.

Some story arcs are MMO-only. Others parallel the single-player narrative
from a different perspective — what was happening in the rest of the galaxy
while the Loup-Garou crew was running the Duskway.

**Business model:**

- **Subscription:** A flat low monthly rate ($5-$9) that unlocks all story
  content for as long as you're subscribed. Story chapters, faction missions,
  access to the persistent universe. If you lapse, your ships and reputation
  are preserved but you can't access new story content or log into the MMO.
- **Inference credits (unified pool).** All inference — whether in single-player
  mode using our proxy, or in MMO mode where the server handles NPC souls —
  draws from a single unified credit pool. Players top up their credit balance.
  We charge the raw provider cost plus a margin no higher than 10%. The same
  credit balance works across both modes. Players can cap their spending or
  set a per-session budget. BYO key mode in single player is exempt — that
  inference uses the player's own API key and does not count against credits.
- **The value proposition:** For $5-$9/mo + inference costs, you get a living
  universe that advances with everyone else, story chapters written by us,
  and AI-managed NPCs that make the world feel inhabited. No FOMO mechanics.
  No pay-to-win. The subscription is the product.

### Server as Product

The MMO server is not just infrastructure — it is a product we design and
build. The feature set includes:

- Universe state coordination (single truth for faction war, economy,
  Predecessor signal, event timeline)
- Player state persistence (ships, crew, reputation, inventory, soul
  relationships)
- Auth and account management
- **In-universe AI inference relay.** All NPC soul inference runs server-side.
  Not client. This is how we maintain the integrity of the shared universe —
  one NPC soul, one truth about how that NPC feels about the player. The
  server caches and batches inference calls for efficiency. Player sends their
  dialogue or action to the server, server processes it through the NPC soul
  chain, returns the result. This ensures no two players get contradictory NPC
  behavior from the same universe state.
- Multiplayer spatial awareness (player position in systems, proximity
  events, cooperative mission state)
- Mod validation and server-side compatibility checking
- Admin tools for us to author and deploy story chapters, monitor universe
  state, and intervene if something breaks

Everything we build for the server — the state coordination, the AI relay,
the story deployment pipeline — becomes part of our operational capability.
If we ever license the server technology to other developers (a "REACHLOCK
server" as a product), it ships with the documentation and tooling to run
their own persistent universe.

---

## 10. Technology Stack

Based on the existing House tooling and the Storyverse architecture:

| Layer | Technology | Notes |
|---|---|---|
| Game Engine | Godot 4 | C# for gameplay logic, GDScript for rapid prototyping |
| NPC Soul Engine | Go + Qdrant | Adapted from existing Ragamuffin/RAG stack |
| Faction Simulator | Go | Tick-based galaxy simulation, event queue |
| Economy System | Go | Dynamic pricing, supply chain simulation |
| Storyline Kanban | Go (or embedded) | Adapted from Storyverse orchestrator |
| Persistent World DB | SQLite (local) / PostgreSQL (server) | SP uses SQLite, MMO server uses PG |
| UI | Godot UI system | All three modes in-engine; no web layer |
| Audio | FMOD or Godot audio | Dynamic music system based on context |
| Inference (Local) | llama.cpp sidecar | SP mode, offline, free |
| Inference (BYOK) | Pluggable provider API | SP mode, player brings their own key |
| Inference (Proxy) | Our inference relay server | SP + MMO modes, thin margin pass-through |
| Server Runtime | Go (or Rust for performance-critical) | MMO: universe state, player coordination, auth |
| Modding Framework | Built-in mod loader + SDK | `.reachmod` packages, manifest, conflict detection (see Section 7) |
| Source Control | chezgoulet infra repo | Design doc in library, code in game repo |

**Design principle:** Everything that can run client-side does. The game is
single-player and offline-capable. The NPC soul engine is a local Go sidecar
or embedded library that Godot communicates with via a local socket or pipe.
The faction simulator runs on the same machine. No server dependency for
normal play.

---

## 11. Development Phases

Two-person team (Christopher + Vigilant). This determines the scope of each
phase. Everything is built to be playable from the first phase.

### Phase 1 — The Foundation (Prototype, 3–4 months)

Goal: A playable loop that proves the three modes work together.

- [ ] Godot project with mode transitions (space → dock → landed → on board)
- [ ] Basic ship flight (Star Fox 64 control feel)
- [ ] One station interior (walkable, top-down)
- [ ] Ship interior (walkable, with a few rooms)
- [ ] Simple trade economy (buy/sell at two locations)
- [ ] Ship hull editor (exterior shape, basic interior grid)
- [ ] One NPC with a soul file (talk to them, they remember)
- [ ] Space combat (basic dogfighting with one enemy type)
- [ ] One landed combat encounter
- [ ] One Predecessor ruin (small, 1 puzzle, 1 boss)

Deliverable: A player can fly from a station to a planet, walk around, talk to
an NPC, buy something, mine an asteroid, fight a ship, explore a ruin, and
return.

### Phase 2 — The Living System (3–4 months)

Goal: The galaxy feels alive and the player has meaningful choices.

- [ ] Faction engine with 3 factions (Compact, ISC, one Corp Charter)
- [ ] Reputation tracking across all factions
- [ ] Dynamic economy (prices shift with supply/demand)
- [ ] Persistent world simulation (ticks advance, events fire)
- [ ] Crew recruitment (2–3 recruitable NPCs with soul files)
- [ ] Crew management on the ship (assign roles, monitor relationships)
- [ ] On-board emergencies (fires, hull breaches, boarding)
- [ ] On-board combat (fight boarders in corridors)
- [ ] Faction missions (procedural + authored)
- [ ] Ship interior editor (full room placement)

### Phase 3 — The Predecessor Story (4–6 months)

Goal: The narrative arc that gives REACHLOCK its thematic weight.

- [ ] The Veil system and the unreported site
- [ ] The signal (Bardo's thread)
- [ ] 5–7 Predecessor ruins (Zelda-style dungeons)
- [ ] The 4 major faction storylines (authored)
- [ ] The Duskway runs (Earth blockade)
- [ ] Alexander's long game (visible player-facing consequences)
- [ ] NPC soul mutations tied to storyline beats
- [ ] The escalation at the Veil (third visit)
- [ ] Multiple endings based on player choices and faction alignment
- [ ] Full crew roster (8–12 recruitable NPCs with complete arcs)

### Phase 4 — Polish & Release (3–4 months)

- [ ] Full ship customization catalogue (30+ hulls, 100+ components)
- [ ] Economy balancing and tuning
- [ ] UI/UX polish across all three modes
- [ ] Audio implementation (music, SFX, ambient)
- [ ] Bug testing and edge-case handling
- [ ] Steam/Early Access release preparation
- [ ] Framework SDK finalization (mod documentation, example mod, asset pipeline)

### Phase 5 — The MMO Server (6–9 months, starts after Phase 2 or in parallel with Phase 3)

Goal: A working persistent universe that players can log into together.

- [ ] Server runtime — Go service handling auth, player state, universe state
- [ ] Universe state coordination — single source of truth for faction war,
  economy, Predecessor signal, event timeline
- [ ] Player state persistence — ships, crew, reputation, inventory across
  sessions
- [ ] In-universe AI inference relay — NPC souls run server-side with caching
- [ ] Multiplayer spatial awareness — player position in systems, proximity
  events, cooperative mission state
- [ ] Client-server synchronization — mode transitions, combat state, trading
- [ ] Auth and account management — registration, login, password reset
- [ ] Chat and social layer — groups, friend lists, player-run factions
- [ ] Story chapter deployment pipeline — author tools for us to deploy new
  arcs
- [ ] Inference credits system — unified pool, usage tracking across SP
  proxy and MMO, pass-through pricing with caps
- [ ] Subscription management — payment processing, tier handling, lapsed
  account preservation, credit top-ups
- [ ] Mod validation — server-side compatibility checking for client mods
- [ ] Beta launch — limited player count, stress testing, iteration

---

## The Name

The name REACHLOCK works on multiple levels.

The Reach is the frontier — unmapped space beyond the last gate. A lock is a
classification of human identity (Skylock, Stonelocker), a mechanism of
control (jump gate lock, cargo lock), and a condition of being sealed in or
out. REACHLOCK as a single word means: *the frontier is what it takes to hold
onto who you are.* Or: *out here, your identity is what you can enforce.*

The name also echoes the ship's name, which echoes the creature from a culture
the empire tried to burn — the loup-garou, the thing that lives between states
and doesn't apologize for it.

---

*This document is speculative. It describes a game that does not yet exist. It
is the shape of something being reached for.*
