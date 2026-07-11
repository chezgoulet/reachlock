# Playing REACHLOCK — The Duskway Run

One file, double-click, play. Everything below the first section is
optional depth.

## Getting aboard

1. Download the build for your machine (CI artifacts, or a release):
   `reachlock.x86_64` (Linux) or `reachlock.exe` (Windows).
2. Run it. Linux: `chmod +x reachlock.x86_64 && ./reachlock.x86_64`.
3. **New Game**, pick who you are — any of the seven crew, or the unnamed
   captain — and fly the run.

Your save lives in the engine's user directory
(`~/.local/share/godot/app_userdata/REACHLOCK/` on Linux) and the title
screen manages it: **Continue** resumes, **New Game** asks before it
writes over a story in progress. Settings (volume, fullscreen, text
speed) persist separately from the save.

## The keys

| key | does |
|---|---|
| WASD | move (walking and flying) |
| R | interact / continue dialogue |
| 1–9 | pick a dialogue choice |
| F | mine the rock under your reticle |
| Space / Shift | brake / boost |
| Ctrl | fire |
| G | launch the decoy beacon (once per flight, if you bought it) |
| H | answer a hail |
| J | jump |
| B | board / dock |
| V (hold) | speak to the crew with your own voice — see below |
| Esc | pause |

## Two machines, one boat (LAN co-op)

The whole architecture points at this: **co-op crew on one deck**. One of
you flies, the other works the power grid, both of you argue with the
crew.

**Host**: play normally, press Esc → **Open the Hatch**. The boat now
listens on port 40710.

**Join**: title screen → **Join a Ship** → type the host's LAN address
(e.g. `192.168.1.20`) → pick a crew member. The seat you claim is yours —
that character stops being an NPC while you're in them.

Not on the same LAN? Put both machines on the same
[Tailscale](https://tailscale.com) or ZeroTier network and use that
address — the game deliberately contains zero NAT-traversal code.
Versions must match; a mismatched build is told so in plain words.

## Minds and voice are optional; here's what the full stack adds

The demo is **complete offline** — authored dialogue, the full campaign,
all endings, no error spam, nothing greyed out. Each daemon you run adds
a layer:

| sidecar | port | adds |
|---|---|---|
| `pan serve` (the mind daemon) | 40707 | crew members *think*: generated dialogue beats, reactions, memory-informed improvisation behind the authored trees |
| `ragamuffin` (the memory store) | 8000 | the crew *remember* across sessions — conversations distill into recallable facts |
| `reachlock-simd` (the universe) | 40708 | prices drift, factions move, news happens while you fly |
| `reachlock-eard` (the ear) | 40709 | **push-to-talk**: hold V and say it — *"I'd do it again, she's crew"* — and the matching choice fires; free speech reaches a live mind as itself |

Developers: `./scripts/dev_stack.sh` brings the stack up. For voice,
`reachlock-eard` wants a whisper.cpp CLI and a model
(`~/.local/share/reachlock/models/ggml-base.en.bin` by default — base or
small; the mind daemon shares your RAM). No daemon, no model, no mic →
the V key and its hint simply do not exist. Nothing pretends.

Voice is an input method, not a feature gate: everything you can say has
a clickable equivalent, and what you say enters the game as text (in
multiplayer only the transcript travels — never audio).

## When something's wrong

- **The game won't start after an update** — your settings file is
  independent of saves; deleting neither is ever required for an update.
  Check the version stamp on the title screen against your friend's.
- **"Could not open the hatch"** — port 40710 is in use (another host on
  this machine?). Close the other instance or set `REACHLOCK_SHARE_PORT`.
- **Voice button missing** — that's the design: the speech daemon isn't
  running or has no model. See the table above.
