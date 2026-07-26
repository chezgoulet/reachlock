#!/usr/bin/env python3
"""Engine/content decoupling guard (iron rule #1, content half).

ReachLock v2 is a character-creation game: the ship you fly, the crew who
narrate it, and the story you play are things a player picks or a modder
authors. None of them may be named in engine code.

They were. The engine shipped with one authored ship compiled into the flight
setup, the interior loader, the crisis model and the onboard consoles, and one
authored crew compiled into the jump, cryo, combat, contract and onboarding
systems. Character creation offered a single hardcoded origin while ten sat
authored on disk. The result: whatever you chose, you flew the Loup-Garou and
a fixed crew spoke over it.

What this checks
----------------
No production line in an engine crate may contain a canonical ship or crew
identity. Systems that need "the pilot" ask the roster for the ROLE:

    let pilot = roster.voice_of("pilot");   # a name, or "the pilot"

Deliberately NOT flagged:
  * `#[cfg(test)]` modules — fixtures may use any names.
  * comments — including the ones explaining this very history.
  * `mods/` — that IS the content; naming the Loup-Garou there is the point.
  * origin/ship *ids* read from content at runtime.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CRATES = [
    "reachlock-core/src",
    "reachlock-client/src",
    "reachlock-server/src",
    "reachlock-cli/src",
    "reachlock-editor/src",
]

# The canonical ship, and the canonical crew as *string literals* — a bare
# word like `boris` in an identifier is fine; `"Boris"` as a value is not.
SHIP = re.compile(r"loup.?garou", re.IGNORECASE)
CREW = re.compile(
    r'"[^"]*?(?<![a-zA-Z])(Tib|Tove|Boris|Prudence|Risc|Keene|Bardo)(?![a-zA-Z])[^"]*?"',
    re.IGNORECASE,
)

# Known, deliberate exceptions. Each must say why, and each is a debt, not a
# design: an entry here means content is missing, not that the engine is
# allowed to name people.
EXEMPT = {
    # Synthetic inputs to the determinism manifest: these strings are hashed,
    # never shown, and never reach gameplay. Renaming them would move golden
    # checksums for no behavioural gain, so they stay — but the gate names
    # them here rather than quietly ignoring the file.
    ("reachlock-core/src/determinism.rs", "manifest"),
    # Hardcoded origin for Loup-Garou veteran. Tied to one-authored content,
    # not engine architecture. Needs to become data-driven (S81).
    ("reachlock-client/src/systems/character_creation.rs", "get_available_origins"),
    ("reachlock-client/src/systems/character_creation.rs", "confirm"),
    # Hardcoded "loup_garou" ship interior loader. The ship path is pinned
    # to one authored hull; untying it is a pre-requisite for multi-ship.
    ("reachlock-client/src/systems/crew.rs", "load_loup_garou_interior"),
    ("reachlock-client/src/systems/interior.rs", "enter_interior"),
    ("reachlock-client/src/systems/interior.rs", "spawn_props"),
    ("reachlock-client/src/systems/interior.rs", "cryo_wake_spawn"),
    ("reachlock-client/src/systems/interior.rs", "cockpit_seat_spawn"),
    ("reachlock-client/src/systems/interior.rs", "builtin_crew_config"),
    ("reachlock-client/src/systems/crisis.rs", "deck_layouts"),
    ("reachlock-client/src/systems/setup.rs", "PLAYER_HULL_ID"),
    ("reachlock-client/src/systems/setup.rs", "spawn_player_ship"),
    ("reachlock-client/src/systems/setup.rs", "spawn_loup_garou_model"),
    ("reachlock-client/src/systems/onboard.rs", "onboard_ship_consoles"),
    ("reachlock-client/src/systems/onboarding.rs", "demo_deliberation_stage"),
    ("reachlock-client/src/systems/soul.rs", "init_souls"),
    # Combat system hardcodes Tove as the engineer. Role-based roster
    # resolution was not wired through combat during the first pass.
    ("reachlock-client/src/systems/combat.rs", "damage_control_contract"),
    ("reachlock-client/src/systems/combat.rs", "damage_control"),
    ("reachlock-client/src/systems/combat.rs", "repair_worst_room"),
    # Cryojump and jump systems pin the navigator/pilot to specific crew.
    # These need role-based resolution.
    ("reachlock-client/src/systems/cryojump.rs", "arm_jump"),
    ("reachlock-client/src/systems/cryojump.rs", "jump_clock"),
    ("reachlock-client/src/systems/cryojump.rs", "revive"),
    ("reachlock-client/src/systems/jump.rs", "default"),
    ("reachlock-client/src/systems/jump.rs", "try_gate_jump"),
    ("reachlock-client/src/systems/jump.rs", "self_jump"),
    # Contract evaluation uses `crew_member: "Boris"` for the deliberation
    # state — the deliberation crew field is set to the contract's author
    # (the pilot), not derived from the roster. Role-based once the
    # contract system carries crew ids.
    ("reachlock-client/src/systems/contract.rs", "evaluate_contracts"),
    # Deliberation renderer uses a hardcoded Tove reference for relationship
    # delta in the stage-progression overlay logic.
    ("reachlock-client/src/systems/deliberation_renderer.rs", "handle_interjection_input"),
    # Generator name-pool for procedurally-generated dilemmas.
    ("reachlock-core/src/generator/dilemma.rs", "NAMES"),
    # Landed combat companion archetype display name pinned to Tib.
    ("reachlock-client/src/systems/landed_combat.rs", "companion_archetype"),
    # Soul editor pins a default identity id. Content tooling debt.
    ("reachlock-editor/src/editors/soul.rs", "generate_from_seed"),
}

COMMENT = re.compile(r"^\s*(//|/\*|\*)")


def exempt_fn_ranges(path: Path, text: str):
    """Line ranges of exempted functions/consts in this file."""
    rel = str(path)
    names = [fn for (p, fn) in EXEMPT if rel.endswith(p)]
    ranges = []
    if not names:
        return ranges
    lines = text.splitlines()
    for i, line in enumerate(lines):
        for fn in names:
            if re.search(rf"\bfn {re.escape(fn)}\b", line):
                depth, j = 0, i
                started = False
                while j < len(lines):
                    depth += lines[j].count("{") - lines[j].count("}")
                    started = started or "{" in lines[j]
                    if started and depth <= 0:
                        break
                    j += 1
                ranges.append((i + 1, j + 1))
            elif re.search(rf"\bconst {re.escape(fn)}\b", line):
                depth, j = 0, i
                started = False
                while j < len(lines):
                    depth += lines[j].count("{") - lines[j].count("}")
                    depth += lines[j].count("[") - lines[j].count("]")
                    started = started or "[" in lines[j] or "{" in lines[j]
                    if started and depth <= 0:
                        break
                    j += 1
                ranges.append((i + 1, j + 1))
    return ranges


def production_lines(path: Path):
    """Yield (lineno, text) for non-comment lines before `#[cfg(test)]`."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    skip = exempt_fn_ranges(path, text)
    for n, line in enumerate(text.splitlines(), start=1):
        if line.lstrip().startswith("#[cfg(test)]"):
            return
        if COMMENT.match(line):
            continue
        if any(lo <= n <= hi for lo, hi in skip):
            continue
        yield n, line


def main() -> int:
    violations = []
    for crate in CRATES:
        base = ROOT / crate
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            for n, line in production_lines(path):
                if SHIP.search(line) or CREW.search(line):
                    rel = path.relative_to(ROOT)
                    violations.append(f"  {rel}:{n}: {line.strip()[:96]}")

    if violations:
        print("DECOUPLING VIOLATION: engine code names specific content.")
        print()
        print("\n".join(violations))
        print()
        print("The engine may not hardcode a ship or a crew member. Resolve the")
        print('role from the live roster instead — roster.voice_of("pilot") —')
        print("or read the id from the character's origin.")
        return 1

    print("decoupling OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
