#!/usr/bin/env python3
"""Three-ring architecture guard (#7).

Enforces the rule from docs/ARCHITECTURE.md: the engine layer (Ring 0) must
have ZERO content. Engine code under godot/scripts/ may not:

  1. hardcode a content id that any mod `provides` (faction/ship/npc/location/
     storyline ids — derived dynamically from every godot/mods/**/manifest.json
     plus the `id` of every content data file), nor
  2. reference a `res://mods/...` content path directly.

The guard inspects STRING LITERALS only. Engine code that meaningfully names a
content entity always does so as a quoted string (`faction_id == "compact"`),
so this catches the real smell while ignoring English words like "reach" that
appear in comments or prose.

Escape hatch: append `arch-allow` in a comment on a line to exempt it.

Run via `make architecture` or `python3 scripts/check_architecture.py`.
"""
from __future__ import annotations

import glob
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.join(REPO_ROOT, "godot", "mods")
ENGINE_DIR = os.path.join(REPO_ROOT, "godot", "scripts")
ENGINE_GLOBS = ("**/*.gd", "**/*.cs")

# Matches double- and single-quoted string literals (no escaped-quote handling
# needed for the content ids and resource paths we care about).
STRING_LITERAL = re.compile(r'"([^"\n]*)"' + r"|'([^'\n]*)'")
ALLOW_MARKER = "arch-allow"


def _collect_strings(node: object, out: list[str]) -> None:
    """Recursively gather every string value in a parsed JSON structure."""
    if isinstance(node, str):
        out.append(node)
    elif isinstance(node, list):
        for item in node:
            _collect_strings(item, out)
    elif isinstance(node, dict):
        for value in node.values():
            _collect_strings(value, out)


def content_ids(mods_dir: str = MODS_DIR) -> set[str]:
    """The denylist: every id a mod provides, plus every data file's `id`."""
    ids: set[str] = set()
    for path in glob.glob(os.path.join(mods_dir, "**", "*.json"), recursive=True):
        try:
            with open(path, encoding="utf-8") as fh:
                data = json.load(fh)
        except (OSError, json.JSONDecodeError):
            # validate_mod_data.py owns reporting malformed JSON; skip here.
            continue
        if not isinstance(data, dict):
            continue
        if os.path.basename(path) == "manifest.json":
            provided: list[str] = []
            _collect_strings(data.get("provides", {}), provided)
            ids.update(provided)
        elif isinstance(data.get("id"), str):
            ids.add(data["id"])
    return {i for i in ids if i}


def engine_files(engine_dir: str = ENGINE_DIR) -> list[str]:
    files: list[str] = []
    for pattern in ENGINE_GLOBS:
        files.extend(glob.glob(os.path.join(engine_dir, pattern), recursive=True))
    return sorted(files)


def scan(engine_dir: str = ENGINE_DIR, mods_dir: str = MODS_DIR, root: str = REPO_ROOT) -> list[str]:
    ids = content_ids(mods_dir)
    violations: list[str] = []

    for path in engine_files(engine_dir):
        rel = os.path.relpath(path, root)
        with open(path, encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, start=1):
                if ALLOW_MARKER in line:
                    continue
                for m in STRING_LITERAL.finditer(line):
                    literal = m.group(1) if m.group(1) is not None else m.group(2)
                    if "res://mods/" in literal or literal.startswith("mods/"):
                        violations.append(
                            f"{rel}:{lineno}: engine code references content path "
                            f'"{literal}" — reach content through the loader, not by path'
                        )
                    elif literal in ids:
                        violations.append(
                            f"{rel}:{lineno}: engine code hardcodes content id "
                            f'"{literal}" — this belongs in godot/mods/, not the engine'
                        )
    return violations


def self_test() -> int:
    """Build a synthetic engine+content tree and assert the guard bites."""
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        mods = os.path.join(tmp, "godot", "mods", "demo")
        engine = os.path.join(tmp, "godot", "scripts")
        os.makedirs(mods)
        os.makedirs(engine)
        with open(os.path.join(mods, "manifest.json"), "w", encoding="utf-8") as fh:
            json.dump({"id": "demo", "provides": {"factions": ["acme_syndicate"]}}, fh)

        # A clean engine file must pass: the word appears only as prose/identifier.
        clean = os.path.join(engine, "clean.gd")
        with open(clean, "w", encoding="utf-8") as fh:
            fh.write('# the player can reach the acme_syndicate territory\n')
            fh.write('func go() -> void:\n\tprint("hello")\n')
        if scan(engine, mods, tmp):
            failures.append("clean engine file was flagged (false positive)")

        # Hardcoded content id as a string literal must be caught.
        with open(os.path.join(engine, "bad_id.gd"), "w", encoding="utf-8") as fh:
            fh.write('func f() -> bool:\n\treturn faction_id == "acme_syndicate"\n')
        if not any("acme_syndicate" in v for v in scan(engine, mods, tmp)):
            failures.append("hardcoded content id was NOT caught")

        # The arch-allow escape hatch must suppress a violation.
        os.remove(os.path.join(engine, "bad_id.gd"))
        with open(os.path.join(engine, "allowed.gd"), "w", encoding="utf-8") as fh:
            fh.write('var x = "acme_syndicate"  # arch-allow: bootstrap default\n')
        if scan(engine, mods, tmp):
            failures.append("arch-allow marker did not suppress violation")

        # A res://mods/ content path must be caught.
        with open(os.path.join(engine, "bad_path.gd"), "w", encoding="utf-8") as fh:
            fh.write('var s = load("res://mods/demo/ships/x.json")\n')
        if not any("res://mods/" in v for v in scan(engine, mods, tmp)):
            failures.append("res://mods/ content path was NOT caught")

    if failures:
        print("self-test FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("self-test OK — guard catches hardcoded ids and content paths, "
          "ignores prose, honors arch-allow")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()

    ids = content_ids()
    if not ids:
        print("WARNING: no content ids discovered under godot/mods/ — guard is a no-op")

    violations = scan()
    if violations:
        print(f"Architecture guard FAILED — {len(violations)} violation(s):\n", file=sys.stderr)
        for v in violations:
            print(f"  - {v}", file=sys.stderr)
        print(
            "\nSee docs/ARCHITECTURE.md. The engine must contain zero content; "
            "move this into godot/mods/ or push it down into a framework contract.",
            file=sys.stderr,
        )
        return 1

    print(
        f"Architecture guard OK — {len(engine_files())} engine file(s) clean "
        f"against {len(ids)} content id(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
