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
CREW = re.compile(r'"(Tib|Tove|Boris|Prudence|Risc|Keene|Bardo)"')

COMMENT = re.compile(r"^\s*(//|/\*|\*)")


def production_lines(path: Path):
    """Yield (lineno, text) for non-comment lines before `#[cfg(test)]`."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    for n, line in enumerate(text.splitlines(), start=1):
        if line.lstrip().startswith("#[cfg(test)]"):
            return
        if COMMENT.match(line):
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
