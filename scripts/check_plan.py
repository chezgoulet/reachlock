#!/usr/bin/env python3
"""Fail when authored systems and `docs/SYSTEMS-PLAN.md` disagree.

`content check` proves the tree is internally consistent. Nothing proved the
tree matched the *plan* — a system could be authored at the wrong coordinates,
with the wrong seed, under a name the plan never mentions, and every gate would
stay green. That makes a sixty-row plan aspirational: you find out it drifted
weeks later, by reading.

This closes the loop in both directions:

  * A planned system that is authored must match the plan exactly.
  * An authored system the plan has never heard of is drift, not a bonus.

Rows that are not authored yet are skipped. The plan is a queue, not a
promise that everything in it exists — so authoring a system is what brings it
under the gate, with no status column to keep in sync by hand.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLAN = ROOT / "docs" / "SYSTEMS-PLAN.md"
SYSTEMS = ROOT / "mods" / "reachlock" / "systems"

# Authored systems the plan deliberately does not cover, each with the reason.
#
# Same convention as the narrow `#[allow(dead_code)]` this repo already
# blesses: an exception that names what it is waiting on and stays greppable,
# rather than a check quietly weakened for everything.
EXCEPTIONS = {
    # Authored by the editor's assistant during a live session; off-convention
    # seed, filename that does not match its id, and in no band. Open decision
    # recorded in SYSTEMS-PLAN.md section 1.4 — keep, reseat, or drop.
    "zola_swamp_system",
}


def parse_plan():
    """Rows of the section 4 table, as dicts, plus any row that failed to parse.

    Returning the failures matters as much as returning the rows. A row that
    stops matching — someone drops the backticks around an id, or reorders a
    column — silently leaves coverage, and every remaining row still passes.
    That is the vacuously-green check this project has been bitten by before,
    so a line that *looks* like a data row and does not parse is an error, not
    a skip.
    """
    text = PLAN.read_text(encoding="utf-8")
    rows = {}
    unparsed = []
    for line in text.splitlines():
        looks_like_a_row = re.match(r"\|\s*\d+\s*\|", line)
        # `| 12 | `rho_seven` | Rho-7 | compact | core | -90, -150, 120 | ...`
        m = re.match(
            r"\|\s*(\d+)\s*\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|"
            r"\s*([^|]+?)\s*\|\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\|\s*(\d+)\s*\|",
            line,
        )
        if not m:
            if looks_like_a_row:
                unparsed.append(line.strip())
            continue
        n, sid, display, faction, biome, x, y, z, seed = m.groups()
        rows[sid] = {
            "n": int(n),
            "id": sid,
            "display_name": display,
            "faction": faction,
            "biome": biome,
            "position": (int(x), int(y), int(z)),
            "seed": int(seed),
        }
    return rows, unparsed


def parse_systems():
    """Authored `ChartedSystem` files, keyed by id."""
    out = {}
    if not SYSTEMS.is_dir():
        return out
    for path in sorted(SYSTEMS.glob("*.ron")):
        text = path.read_text(encoding="utf-8")
        sid = re.search(r'id:\s*"([^"]+)"', text)
        if not sid:
            continue
        display = re.search(r'display_name:\s*"([^"]*)"', text)
        seed = re.search(r"seed:\s*(\d+)", text)
        biome = re.search(r"biome:\s*(\w+)", text)
        pos = re.search(
            r"position:\s*\(\s*x:\s*(-?\d+)\s*,\s*y:\s*(-?\d+)\s*,\s*z:\s*(-?\d+)\s*\)",
            text,
        )
        out[sid.group(1)] = {
            "path": path.relative_to(ROOT),
            "display_name": display.group(1) if display else None,
            "seed": int(seed.group(1)) if seed else None,
            "biome": biome.group(1) if biome else None,
            "position": tuple(int(g) for g in pos.groups()) if pos else None,
        }
    return out


def check_plan_itself(plan, problems):
    """The plan has to be coherent before it can be a spec for anything."""
    by_seed = {}
    for row in plan.values():
        by_seed.setdefault(row["seed"], []).append(row["id"])
    for seed, ids in sorted(by_seed.items()):
        if len(ids) > 1:
            problems.append(
                f"PLAN: seed {seed} is claimed by {', '.join(sorted(ids))}. "
                f"A seed drives all of a system's procedural generation; two "
                f"systems sharing one are the same system twice."
            )
    for row in plan.values():
        # Existing content uses 0x0n0n0n0n by row index. A seed off that
        # sequence is not wrong in itself, but it is almost always a typo,
        # and a wrong seed silently regenerates the system.
        expected = int(f"{row['n']:02x}" * 4, 16)
        if row["seed"] != expected:
            problems.append(
                f"PLAN: row {row['n']} (`{row['id']}`) has seed {row['seed']}, "
                f"but the 0x0n0n0n0n convention gives {expected}."
            )


def main() -> int:
    if not PLAN.is_file():
        print(f"MISSING: {PLAN.relative_to(ROOT)}")
        return 1

    problems_early = []
    plan, unparsed = parse_plan()
    for line in unparsed:
        problems_early.append(
            f"UNPARSED ROW: {line[:100]}\n"
            f"      This looks like a table row but does not match the expected "
            f"format, so it is not being checked at all."
        )
    if not plan:
        # A parse that finds nothing would pass every other check vacuously.
        print("PLAN: no rows parsed from the section 4 table — the format changed.")
        return 1

    authored = parse_systems()
    problems = problems_early
    check_plan_itself(plan, problems)

    for sid, have in sorted(authored.items()):
        if sid in EXCEPTIONS:
            continue
        want = plan.get(sid)
        if want is None:
            problems.append(
                f"UNPLANNED: `{sid}` ({have['path']}) is authored but is in no "
                f"plan row. Add it to SYSTEMS-PLAN.md section 4, or to the "
                f"exceptions in this script with a reason."
            )
            continue
        for field in ("display_name", "seed", "biome", "position"):
            if have[field] is None:
                problems.append(
                    f"UNREADABLE: `{sid}` ({have['path']}) has no parseable {field}."
                )
            elif have[field] != want[field]:
                problems.append(
                    f"DRIFT: `{sid}` ({have['path']}) has {field} "
                    f"{have[field]!r}, plan row {want['n']} says {want[field]!r}."
                )

    if problems:
        print("SYSTEM PLAN VIOLATION\n")
        for p in problems:
            print(f"  {p}")
        print(
            f"\nThe plan is the spec: docs/SYSTEMS-PLAN.md. Either the content "
            f"is wrong or the plan is out of date — but they cannot both be "
            f"right, and finding out by reading in three weeks is the failure "
            f"this check exists to prevent."
        )
        return 1

    planned = len(plan)
    done = sum(1 for sid in plan if sid in authored)
    skipped = len(EXCEPTIONS & set(authored))
    note = f", {skipped} exception(s)" if skipped else ""
    print(f"plan OK ({done}/{planned} systems authored and matching{note})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
