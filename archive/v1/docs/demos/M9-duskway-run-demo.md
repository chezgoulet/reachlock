# M9 — "The Duskway Run" demo campaign ✅ (2026-07-08)

The playable demo spine: one blockade-run story, six beats, four chained
missions, from waking mid-mining-op on the Loup-Garou to a gravel field in
Charlevoix inside Earth's blockade. Fresh boot lands you On Board at Ledger
Drift with the campaign started; every beat advances on the same events the
mode scenes already report. Verified headless against the live stack (pan +
reachlock-simd + ragamuffin memory store); all 66 GUT tests green, `make
check` + dsl-bridge green.

## The beats → the missions

| Beat (brief) | Mission / stage | Mechanism |
|---|---|---|
| 0 Cold open | act1: `shift_briefing`, `take_the_stick`, `work_the_claim` | Crew figures stand at their stations in the walkable interior and are talkable (DialogueRunner aboard, new). Tove's galley briefing gates the launch; mining counts `ore_mined` events. |
| 1 Pirates | act1: `survive_the_ambush` | Existing ambush (Vex, reaver skiff) reports `survived_ambush`. |
| 2 Emergency jump | act1: `limp_home` | NEW CryoTransit sequence: location `self_jump` routes, organics pod up (npc `synthetic`/`jump_pilot` fields), Prudence flies, tunnel, wake. J key in space. |
| 3 Sorrow arrival | act2: `limp_in` | Dock → `docked` event; docking saves (checkpoint). |
| 4 Bar fight | act2: `the_interval` | NEW auto dialogues: `prudence_bar_fight` plays itself on dock (dialogue `auto` + `speaker_names` for Grissom/Doss lines). Wrecks the commercial deck; sets `bar_fight_done`. |
| 5 The offer | act3: `see_doss` → `outfit_and_lift` | Doss's deal pays **2400 cr (tunable on the act3 mission file)** + loads the medical cache (new mission `cargo` reward). NEW UpgradeShop mounts at `shipyard` service: 7 upgrades across ship/equipment/gadget, incl. a do-nothing lucky charm. |
| 6 The run | act4: jump → `run_the_cordon` (300s timer) → `the_handover` | Self-jump into blockade space (4 hostile pickets, `engagement: engage`). Timer starts while you're still waking — cryo recalibration buys 60s back. Stealth (detection_mult), guns (damage_bonus + patrol combat), speed (speed_mult), or raw nerve. Noor takes the crate; you're stranded. Epilogue cards for success / time-expired / everyone-dies. |

## New framework surface (all additive, contracts updated)

- **Missions**: `event` completion type with counts, `next` chaining
  (campaign = linked list, manifest `start.mission` opens it), `epilogue`
  cards with per-reason failure overrides, `cargo` rewards, progress
  persisted in the save's new `mission` block (restored via
  `universe_loaded`, counted progress and timers included).
- **MissionHud autoload**: objective banner + countdown across all modes,
  drives `MissionManager.tick`, presents epilogues; failure rewinds to the
  last checkpoint save.
- **CryoTransit** (`scripts/framework/cryo_transit.gd`): the §6.3 ritual as
  a data-driven scene; fires a perceive at the jump pilot's soul so live
  minds experience the crossing.
- **UpgradeShop** + `upgrade` schema/kind: effects are a flat bag —
  `hull_bonus`, `damage_bonus`, `detection_mult`, `speed_mult`,
  `timer_bonus_seconds` known to the engine; unknown keys are mod food.
- **Dialogue**: `auto` scenes, `speaker_names` per-node voice override,
  `set_player_flag`/`clear_player_flag` mutation ops (story gates live on
  the player, not the npc soul).
- **Space flight**: flies `GameState.player.current_space` (new save field)
  instead of always the start system; `self_jump` routes; patrols are now
  shootable and shoot back (`fired` signal replaces the destroyed-signal
  hack), hostile engagement escalates straight to weapons, detection ranges
  respect stealth gear; dock beacons only exist where `services` has
  `dock`.
- **Ship interior**: crew figures at stations, talkable; Disembark hidden
  in open space; launching from a berth performs the undock bookkeeping.

## Content

- Locations: `the_drift` (beats 0-2), `earth_landing` (blockade space +
  Charlevoix Field planet surface), Sorrow Station reworked (Doss, Grissom,
  Prudence present; shipyard; Duskway self-jump; gate removed — the
  Compact's gate is lore, the drive is the exit).
- NPCs: Doss (station master, Tib history), Noor (Duskway receiving
  contact), Grissom (the bar's bad night). **Prudence realigned to canon**:
  droid, jump pilot, cockpit — the brief's crew table and beat 4 require
  it; Risc takes engineering. All seven crew now board (`ship` +
  `aboard`).
- 8 new dialogues (bar fight, Doss offer/post, Noor delivery/after, galley
  briefing, Doc Keene's cryo primer, Boris's pod check) mixing authored
  and generated nodes; 4 chained missions; 7 upgrades; the medical cache.

## What a tester does

```sh
./scripts/dev_stack.sh          # pan + simd + ragamuffin (offline also works)
rm -f ~/.var/app/org.godotengine.Godot/data/godot/app_userdata/REACHLOCK/saves/slot0.json
make godot                      # play: WASD walk, R interact, J jump, F mine, CTRL fire
```

## Verified

- Fresh headless boot: mods load, act1 autostarts, all 7 crew seed to
  stations, `current_space` set (fixed a boot-order bug: `mods_loaded`
  fired before GameState/CrewRoster connected; and CrewRoster now reads
  freeform `rooms`).
- Headless scene smokes: the drift space, blockade space (timer ticked
  300→294 and persisted mid-motion), Sorrow dock (bar fight auto-started,
  grissom's soul decided via live pan-serve), Charlevoix planet scene.
- `test_demo_campaign.gd`: full four-act chain by events, cordon-timer
  failure ending, ship-destroyed ending, cryo-recalibration timer bonus,
  upgrade aggregation, save/restore mid-mission.
- `test_cryo_transit.gd`: sequence runs to `finished`; content guarantees
  a synthetic jump pilot and organic sleepers.
- Fixed pre-existing flake: jump-gate emergency test asserted first-try
  success against a 20% random malfunction.

## Known gaps (tracked, non-blocking)

- The self-jump route is available before the story asks for it — jumping
  to the blockade from Sorrow before taking Doss's deal strands the
  mission at an earlier stage (recoverable via checkpoint reload).
- Docking at Earth saves over the Sorrow checkpoint; harmless today (the
  handover stage can't fail) but worth a save-slot ring later.
- Patrol "three approaches" are stat-driven (stealth/guns/speed); no
  bespoke decoy-launch verb yet — the decoy beacon is a passive
  detection_mult.
- Bar fight is a scripted dialogue scene, not a playable brawl; the beat's
  narration carries it.
- Beat 0's station minigames are the existing station interactions; the
  "each station is a minigame" depth is future work.
