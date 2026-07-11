# Ship-Share — v0

The LAN multiplayer contract: **co-op crew on one boat**. The thing the
architecture has been pointing at since ShipOperation grew station occupancy
— more than one pair of hands on the same deck. Two players walk the ship
and Sorrow together, one flies while the other works the power grid
mid-cordon. That slice IS the FTL × Among Us pitch, live.

Status: **v0** (2026-07-10). `share_version` is a single integer stamped on
every join; a mismatch is refused **loudly** (the joining player sees whose
build is older) and never negotiated. Conformance: every fixture in
`share/fixtures/` validates against
[schemas/share_message.schema.json](schemas/share_message.schema.json) via
`scripts/check_ship_share.py` (runs in `make check`); seat-claiming and
intent handling get engine contract tests that run without any network
hardware (`godot/tests/test_ship_share.gd` drives the payloads straight
into the handlers).

## Transport

- **Godot high-level ENet** (`ENetMultiplayerPeer`), default port **40710**
  (`REACHLOCK_SHARE_PORT`). Players connect by IP on the LAN; the
  documented remote path is Tailscale/ZeroTier — the game contains **zero
  NAT code**, on purpose, forever.
- Payloads are Godot Dictionaries carried over reliable ordered RPC. The
  shapes below are the contract; the schema validates their JSON form.
  (This is a *payload vocabulary* riding Godot's transport, not an NDJSON
  wire — the sidecar protocols keep that job.)
- **Hosting is one button.** No lobby screens in the solo path, no boot
  regression: a solo game IS a hosted game with zero peers, minus the
  listen socket (nothing binds until the player opens the hatch).

## Authority (what host-authoritative means, written down)

The HOST is the single authority for everything that can disagree:

| domain | rule |
|---|---|
| souls (pan) | one daemon connection, the host's; clients never talk to pan |
| sim (simd) | the host's; universe snapshots flow outward as state |
| memory (ragamuffin) | the host's; transcripts/mutations funnel through it |
| missions | stage transitions, timers, event counts: host decides |
| weave resolution | proposals resolve ON THE HOST (WEAVE-CONTRACT.md), then replicate as data |
| RNG | every gameplay roll happens host-side; clients never roll |
| the save | **the save is the host's.** Clients carry nothing home but the evening |
| dialogue | the host runs every DialogueRunner; clients see lines/choices as state and answer with intents |
| voice | STT runs on each player's own eard; only the **transcript text** travels (EAR-PROTOCOL.md); matching runs at the host against the choices the host offered |

Clients send **intents** (requests to act); the host validates against the
same rules single-player uses, applies, and broadcasts **state**. A client
that disagrees with the host is wrong by definition and gets corrected by
the next state message.

## The join handshake

1. Client connects; client sends `hello {share_version, game_version, name}`.
2. Version mismatch → host answers `refuse {reason: "version_mismatch",
   host_version, yours}` and disconnects. The refusal renders as a
   sentence, not a log line.
3. Otherwise `welcome {share_version, roster, seats}` — who's aboard and
   which crew are claimed.

## Seats (character select becomes seat-claiming)

Each player IS a crew member. The lobby is the character-select screen
with company:

- `claim_seat {npc_id}` — first claim wins (host arbitrates); the crew
  member **stops being an NPC**: their soul is not instantiated (or is
  released), the stand-in substitution that single-player already does for
  the chosen character generalizes to every claimed seat.
- `release_seat {}` — leaving (or disconnecting) returns the crew member
  to the roster as an NPC, exactly where the player left them standing.
- The host's own seat is a claim like any other. Two players may not hold
  one seat; a claim for a taken seat answers `seat_denied {npc_id, held_by}`.
- Claimed seats replicate in `seats` state so the select screen shows live
  claims — a name tag appears on the portrait the moment a friend picks it.

## Message vocabulary

Client → host (intents):

| kind | body | meaning |
|---|---|---|
| `hello` | `{share_version, game_version, name}` | join handshake |
| `claim_seat` | `{npc_id}` | take a crew member |
| `release_seat` | `{}` | give them back |
| `move` | `{position: [x,y], facing, anim}` | my pawn moved (own-pawn prediction; host relays and may correct) |
| `station` | `{op: "occupy"\|"vacate", station_id}` | take/leave a ship station |
| `control` | `{station_id, axis, value}` | station input (throttle, power shares…) |
| `choose` | `{dialogue_id, index}` | answer the open dialogue's choices |
| `say` | `{text}` | free speech (typed or a voice transcript — the host cannot tell, by design) |

Host → clients (state):

| kind | body | meaning |
|---|---|---|
| `welcome` | `{share_version, roster, seats}` | you're in |
| `refuse` | `{reason, host_version, yours}` | you're not, and here's why |
| `roster` | `{players: [{peer, name, npc_id}]}` | who's aboard |
| `seats` | `{claimed: {npc_id: peer}}` | live seat claims |
| `pawn` | `{peer, position: [x,y], facing, anim}` | someone moved |
| `station_state` | `{stations, controls}` | ShipOperation occupancy + control mirror |
| `dialogue_line` | `{dialogue_id, speaker, text}` | a line landed |
| `dialogue_choices` | `{dialogue_id, choices: [{index, text}]}` | choices are up |
| `dialogue_ended` | `{dialogue_id}` | conversation closed |
| `world` | `{tick, mode, snapshot}` | periodic world state (mission stage, hull, cargo…) |
| `seat_denied` | `{npc_id, held_by}` | claim lost the race |

Additive evolution only: new kinds and new optional fields are free; any
rename or semantic change bumps `share_version`.

## What v0 deliberately leaves out

- **Space combat replication.** v0 shares the interior (walk, talk,
  stations) and lets station controls drive the host's flight sim — the
  cordon run with a friend on the power grid. Client-side prediction for
  the flight model itself is a later sprint.
- **More than one boat.** One host, one ship, one deck. The MMO server
  (`reachlock-server`) is a different animal and a different year.
- **Voice chat.** Talk over the table or the call you already have open;
  the game carries transcripts, not audio.

## Hard rules (from the sprint brief, non-negotiable)

- Multiplayer never blocks single-player: no lobby in the solo path, no
  boot-time regression, offline completes the campaign untouched.
- NPCs breathe on the host; everyone sees the same life.
- Seat-claiming and intent validation are engine logic, tested without
  network hardware. The ENet layer is a thin pipe under a tested core.
