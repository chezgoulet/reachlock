# M12 — "Would they play it twice?" — the second-playthrough pass ✅ (2026-07-09)

The brief asked seven questions about whether the demo is worth a second
run. This pass makes the good answers true. Beat by beat:

## The mining cold open — teaching, or occupying time?

**Now a hunt.** Every rock has a prospector's name from location data
(Tove's Maybe, The Molar, Ledger's Hope, Old Grudge, Payday…) and a seam
tier — lean / fair / rich — that changes cut speed and how the rock glows.
The hint names what you're cutting. Each unit **breaks off as a visible
chunk that arcs into the hold**; a spent rock **cracks into drifting
shards** and the HUD tells you where the scope reads the next seam and how
rich it runs. The pirate now arrives at exactly 3 ore — the moment the
claim gets interesting, someone takes it. Greed is possible (8 named rocks,
2–4 units each); the ambush interrupts it. That's the loop that makes you
want the next rock.

## The pirate attack — does the player lose something real?

**Yes, and nobody announces the number.** A hit past the shields can
breach the hold: loose cargo (good schema `loose_cargo` — raw ore, never
the mission crate) **vents as a drifting canister**. Turn around inside a
few seconds and you can grab it back — mid-firefight. Jump, and it stays
behind. The loss surfaces later, silently, at the cargo manifest and the
Sorrow market counter: fewer units, fewer credits, one fewer upgrade.
Hull damage and interior fires already persisted; now Doss's promised
repairs honestly happen (mission `hull_min`: field-patched to 70% — real
plate at the counter still costs 900).

## The cryo jump — does the game let you sit with it?

**A held beat, both ways.** Sleeping: after your pod seals, the last thing
you see through the frost is the pilot crossing to the board alone —
"settling in beside the thing that unmakes minds. A small, precise wave.
Then the cold takes the window." (4.5 seconds, unskippable, quiet.)
**Playing a synthetic character, you stay awake** — the crossing runs
nearly twice as long, and Prudence's authored transit thoughts (npc
`barks.transit_alone`) surface one held line at a time: *"I have never
told them what it looks like out here. They have never asked. Both facts
are load-bearing."* Playing as Prudence, those lines are your own inner
voice. This is the demo's strongest replay hook: pick the droid to see the
thing the crew never does. (Also fixed: the chosen character no longer
gets sealed into two pods at once.)

## The bar fight — does the choice matter three scenes later?

**Three stances, three echoes.** Each choice now leaves a flag (additive
mutations only — the tree's words are untouched): `stood_for_prudence`,
`trusted_prudence`, `bought_grissom_drink`. Then:
- **Grissom remembers** — four new dialogues (one per stance, plus one for
  meeting him **as Prudence**: "I know what I said. In front of everybody.
  You going to file that too?"). Each plays once (`grissom_said_his_piece`),
  moves his trust and your Reach standing (new `adjust_faction` mutation
  op, schema + engine).
- **Doss prices it in** — stand between a swing and a droid before you knew
  her name, and after the deal she tells you exactly what that bought.
- **Prudence closes the loop aboard**, later, mid-run prep — with a
  generated beat behind a buffer line when the captain says "it's what
  crew is."

## Doss's deal — is the shop a decision?

**2400 covers the cheapest three upgrades (920) and not seven (3230+).**
Ore you kept sells on top — mining greed and ambush spills move the
budget. The **decoy beacon is now a verb**: press G in flight and it
screams your transponder in the wrong place for eighteen seconds — every
picket in earshot bites. One charge per flight. Lucky dice still do
nothing, as the shop's honest little joke about promises.

## The run — distinct approaches?

Fight (guns/power/calibration), sneak (stealth rings), run (drive tune +
engine power), **trick** (the decoy), and now **bribe**: a picket that
catches you clean opens a hail window — six seconds, 450 credits,
"inspection fee." It stands down blind for a grace period. **Once.** The
second boat heard about the first one's good afternoon, and the fee adds
notoriety. Also fixed here: a latent signal-signature crash that fired on
every patrol engagement.

## The three endings — earned?

- **Success** gets the numbers under the prose: *"The run, by the numbers:
  41 seconds left on the window · hull at 62% · 2 damage reports still
  open below decks · 310 credits to your name."* Clever, not just relieved.
- **Time expired** and **hull lost** no longer cut straight to prose: a
  red beat lands first — **THE WINDOW CLOSES / HULL LOST** — two seconds,
  then the card.
- **Everyone dies** reads the names: the card ends with the crew, name by
  name from data, "and the ship." Not a count. The names.

## The test suite — the journey has no dead ends

`test_player_journey.gd`, 16 new tests: every goto in every tree resolves
and every node is reachable (this immediately caught **three canon
sign-off lines that had never played** — nodes literally named `end`,
which the runner treats as the terminator; repaired by mechanical rename,
zero words changed); the bar fight reaches Doss from all three choices and
every stance has an echo someone speaks later; the scene completes playing
AS its own speaker; the campaign chain is unbroken and endings exist only
at its end; the cordon timer cannot start before the crossing; the budget
covers three-not-seven; the bribe is affordable but hurts; the decoy is an
action, not a stat; mission cargo can never spill; the awake crossing has
a voice; Doss's repairs are a patch, not a gift. **136/136 total** (target
was 80).

## Verified

- 136/136 GUT tests ×2 consecutive runs, `make check` green, dsl-bridge
  green. Worst-case headless smoke (stationary ship under four pickets):
  interior damage caps at 6 (further hits deepen severity instead), both
  loose ore units genuinely lost overboard, mission crate secure, the
  everyone-dies path fires with fanfare and names.

## Gaps noted (off-limits or next budget)

- The bartender per se doesn't exist as an NPC — Grissom and Doss carry
  the memory of the fight. A named bartender is a content add for later.
- LLM-side memory of the fight rides the existing mutation memories
  (pan/ragamuffin untouched, per the rule).
- "Hide behind a moon" — there is no moon in blockade space to hide
  behind; patrols do lose you at 3× detection range, and the timer expiry
  now has its fanfare. Terrain-based hiding is a space-content pass.
