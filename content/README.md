# ReachLock Content Files

Content files go here. Each subdirectory holds `.ron` or `.json` files.

## Quick Start

```bash
# Validate a content file
reachlock-cli content validate content/stations/scaffold_station.ron

# Preview in a window (requires display)
reachlock-cli content preview content/souls/scaffold_soul.ron

# Publish to server (requires running server + auth token)
REACHLOCK_TOKEN=... reachlock-cli content publish content/souls/scaffold_soul.ron \
  --universe all --priority authoritative
```

## Directory Layout

| Directory | Content Type | Format |
|---|---|---|
| `stations/` | Station interiors | `.ron` |
| `souls/` | NPC crew definitions | `.ron` |
| `dungeons/` | Predecessor ruins | `.ron` |
| `factions/` | Faction profiles | `.json` |
| `events/` | Scripted events | `.ron` |
| `schemas/` | JSON Schema files (symlinks to `mods/reachlock/schemas/`) | `.json` |
| `gate_network/` | Jump gate definitions | `.ron` |
| `storylines/` | Faction storyline arcs | `.ron` |

## Authoritative Documentation

See `docs/REACHLOCK-V2-SPEC.md` §10 (Authored Content Pipeline) and
individual sprint briefs in `docs/sprints/` for detailed content type specs.
