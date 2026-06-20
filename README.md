# REACHLOCK

> *A game about surviving the frontier, choosing your allegiances, and living with the consequences of a universe that doesn't wait for you.*

REACHLOCK is a dual-mode space game built in Godot 4. It plays like Escape Velocity, Stardew Valley, FTL, and Star Fox 64 had a child raised on a frontier station.

**Single-Player:** You fly the *Loup-Garou* with its crew of seven — Tib, Tove, Bardo, Doc Keene, Prudence, Risc, and Boris. Their story. The Veil. The Duskway. The Predecessor revelation. Played offline or online, with your own AI inference — local (llama.cpp), a cloud key, or our proxy service.

**Online MMO:** A persistent shared universe. Same factions, same economy, same soul system — but the universe state is shared. Player actions collectively reshape the galaxy. The Helldivers 2 model: coordinated persistent war, single timeline, every player contributes.

## Architecture

Three modes, each feeding into the next:

- **Landed** — Stardew × Zelda × Pokémon. Stations, planets, farming, dungeons, crafting, and the people who make a place home.
- **On Board** — FTL × Among Us (Deeper). The ship as a physical space. Rooms, subsystems, crew management, boarding actions.
- **Space Flight** — Star Fox 64. Arcade-style dogfighting with meaningful consequences.

## Technology Stack

| Layer | Language | Role |
|---|---|---|
| Game client | GDScript / C# (Godot 4) | Rendering, input, UI |
| NPC soul engine | Rust (Pan) | Inference decisions, behavior trees |
| MMO server | Go | State relay, coordination, matchmaking |
| Knowledge layer | Ragamuffin | Persistent memory, lore, faction state |

## Modding-First

REACHLOCK is built to be modded from the ground up. Three layers:

- **Ring 0 — Core:** The engine, netcode, and base systems. Changed rarely.
- **Ring 1 — Content:** Ships, weapons, factions, missions. Loaded from data files. Anyone can add.
- **Ring 2 — Soul:** NPC personality, dialogue, decision weights. Plain text prompts and markdown skills.

## License

AGPL-3.0 — see [LICENSE](LICENSE).

---

*Built by chezgoulet / The House.*
