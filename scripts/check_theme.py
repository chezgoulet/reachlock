#!/usr/bin/env python3
"""UI theming guard.

Every screen in the client is styled from `assets/ui/*.ron`. UI code names a
style class and the theme resolver writes the concrete colors:

    commands.spawn(theme::text("row.value", name));

A widget that constructs its own `TextColor` / `BackgroundColor` /
`BorderColor` from a literal is invisible to the stylesheet: editing the theme
will not restyle it, and it will not follow a re-skin. The client had 67 such
literals across ~60 files, no two of them quite the same shade of pale blue.
This guard stops that set from growing back.

What this checks
----------------
Only the three components the theme resolver writes, constructed from a
literal `Color`. That is deliberately narrower than "no Color:: in the
client": sprite tints, gizmo strokes, 3D materials and the camera clear color
are render-layer concerns that the class system does not cover, and a guard
that flagged them would be claiming to enforce a rule nobody follows.

The allowlist
-------------
`scripts/theme_allowlist.txt` holds the files not yet migrated, with the count
of remaining literals in each. Counts may only shrink: fixing some but not all
of a file still passes, adding one to an allowlisted file does not. A file that
reaches zero must be removed from the list, so the allowlist cannot quietly
become permanent.

Run `--update` after a migration to rewrite the counts, and `--self-test` to
confirm the guard still catches a violation.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLIENT = ROOT / "reachlock-client/src"
ALLOWLIST = ROOT / "scripts/theme_allowlist.txt"

# The theme module owns these constructions; that is its whole job.
EXEMPT_DIRS = {"theme"}

# `TextColor(Color::…)`, `BackgroundColor(Color::…)`, `BorderColor::all(Color::…)`
# and struct-literal `BorderColor { top: Color::… }`.
PATTERNS = [
    re.compile(r"\bTextColor\s*\(\s*Color::"),
    re.compile(r"\bBackgroundColor\s*\(\s*Color::"),
    re.compile(r"\bBorderColor::all\s*\(\s*Color::"),
    re.compile(r"\bBorderColor\s*\{[^}]*Color::"),
]

# `Color::NONE` is the absence of a color rather than a choice of one, so it
# has nothing to theme. The theme's own helpers use it as the neutral the
# resolver paints over.
EXEMPT_VALUES = re.compile(r"Color::NONE\b")


def is_exempt(path: Path) -> bool:
    return any(part in EXEMPT_DIRS for part in path.relative_to(CLIENT).parts)


def scan_text(text: str):
    """Yield (line_no, line) for each unthemed UI color construction.

    Skips comments and `#[cfg(test)]` modules: test fixtures may build any
    color they like, and the comments here explain this very history.
    """
    hits = []
    in_test_mod = False
    test_depth = 0
    depth = 0
    for n, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()

        if not in_test_mod and stripped.startswith("#[cfg(test)]"):
            in_test_mod = True
            test_depth = depth
            continue
        if in_test_mod:
            depth += line.count("{") - line.count("}")
            if depth <= test_depth and "}" in line:
                in_test_mod = False
            continue
        depth += line.count("{") - line.count("}")

        if stripped.startswith("//"):
            continue
        code = EXEMPT_VALUES.sub("", line.split("//")[0])
        if any(p.search(code) for p in PATTERNS):
            hits.append((n, stripped))
    return hits


def scan_tree():
    """{relative path: [(line, text), …]} for every offending client file."""
    found = {}
    for path in sorted(CLIENT.rglob("*.rs")):
        if is_exempt(path):
            continue
        hits = scan_text(path.read_text(encoding="utf-8"))
        if hits:
            found[str(path.relative_to(ROOT))] = hits
    return found


def read_allowlist():
    if not ALLOWLIST.exists():
        return {}
    allowed = {}
    for line in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        path, _, count = line.rpartition(" ")
        allowed[path.strip()] = int(count)
    return allowed


def write_allowlist(found):
    lines = [
        "# Files not yet migrated to the theme, with their remaining count of",
        "# unthemed TextColor/BackgroundColor/BorderColor literals.",
        "#",
        "# Counts may only shrink. Regenerate with:",
        "#     python3 scripts/check_theme.py --update",
        "# A file that reaches zero must be deleted from this list.",
        "",
    ]
    for path, hits in sorted(found.items()):
        lines.append(f"{path} {len(hits)}")
    ALLOWLIST.write_text("\n".join(lines) + "\n", encoding="utf-8")


def self_test() -> int:
    """The guard must catch a violation, and must not cry wolf."""
    must_flag = [
        'TextColor(Color::srgb(0.8, 0.9, 1.0)),',
        "BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),",
        "BorderColor::all(Color::WHITE),",
        "        TextColor( Color::BLACK ),",
    ]
    must_pass = [
        'commands.spawn(theme::text("row.value", name));',
        "TextColor::default(),",
        "BackgroundColor(theme_color),",
        "// TextColor(Color::srgb(1.0, 0.0, 0.0)) — the old way",
        "let tint = Color::srgb(1.0, 0.0, 0.0); // sprite, not UI",
    ]
    failures = []
    for line in must_flag:
        if not scan_text(line):
            failures.append(f"MISSED a real violation: {line!r}")
    for line in must_pass:
        if scan_text(line):
            failures.append(f"FALSE POSITIVE on: {line!r}")

    fixture = "#[cfg(test)]\nmod tests {\n    TextColor(Color::WHITE),\n}\n"
    if scan_text(fixture):
        failures.append("FALSE POSITIVE inside a #[cfg(test)] module")

    if failures:
        print("check-theme self-test FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"check-theme self-test OK ({len(must_flag)} caught, {len(must_pass)} ignored)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="rewrite the allowlist")
    parser.add_argument("--self-test", action="store_true", help="check the guard itself")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    found = scan_tree()
    if args.update:
        write_allowlist(found)
        total = sum(len(h) for h in found.values())
        print(f"allowlist updated: {len(found)} file(s), {total} literal(s) remaining")
        return 0

    allowed = read_allowlist()
    problems = []

    for path, hits in sorted(found.items()):
        budget = allowed.get(path)
        if budget is None:
            problems.append(
                f"{path}: {len(hits)} unthemed UI color(s) in a file that is not "
                f"on the allowlist"
            )
            for n, text in hits[:5]:
                problems.append(f"    {path}:{n}: {text}")
        elif len(hits) > budget:
            problems.append(
                f"{path}: {len(hits)} unthemed UI color(s), allowlist permits "
                f"{budget} — this file got worse"
            )

    # A file that has been fully migrated must leave the list, or the allowlist
    # slowly stops describing anything.
    for path, budget in sorted(allowed.items()):
        actual = len(found.get(path, []))
        if actual == 0:
            problems.append(
                f"{path}: fully themed ({budget} expected, 0 found) — "
                f"remove it from scripts/theme_allowlist.txt"
            )
        elif actual < budget:
            problems.append(
                f"{path}: down to {actual} from {budget} — run "
                f"`python3 scripts/check_theme.py --update` to lock the gain in"
            )

    if problems:
        print("UNTHEMED UI COLORS", file=sys.stderr)
        print("", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("", file=sys.stderr)
        print(
            "  Use a style class instead:  theme::text(\"row.value\", …)\n"
            "  Classes live in assets/ui/phosphor.ron.",
            file=sys.stderr,
        )
        return 1

    remaining = sum(len(h) for h in found.values())
    if remaining:
        print(f"theme OK ({remaining} known literal(s) left in {len(found)} file(s))")
    else:
        print("theme OK (every client UI color comes from the stylesheet)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
