#!/usr/bin/env python3
"""Three-ring architecture guard (#7).

Enforces the rule from docs/ARCHITECTURE.md: the engine layer (Ring 0) must
have ZERO content. Engine code under godot/scripts/ and server/ may not:

  1. hardcode a content id that any mod `provides` (faction/ship/npc/location/
     storyline ids — derived dynamically from every godot/mods/**/manifest.json
     plus the `id` of every content data file), nor
  2. reference a `res://mods/...` content path directly. (Go engine code
     cannot reach Godot paths at runtime; the rule is enforced for godot
     source only. Go engine code that *names* a `res://` string in a test
     or a constant would be flagged the same way.)

The guard inspects STRING LITERALS only. Engine code that meaningfully names a
content entity always does so as a quoted string (`faction_id == "compact"`),
so this catches the real smell while ignoring English words like "reach" that
appear in comments or prose. Go source is checked for "..." and '...'
literals; Godot source additionally for backtick `` `...` `` raw strings.

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

# Engine directories scanned by the guard. Each entry is (absolute path,
# glob pattern) and the engine check runs against every file those globs
# expand to. The two engines share a denylist (built once from the mods
# tree) so a content id hardcoded in either one is caught the same way.
ENGINE_DIRS: list[tuple[str, str]] = [
    (os.path.join(REPO_ROOT, "godot", "scripts"), "**/*.gd"),
    (os.path.join(REPO_ROOT, "server"), "**/*.go"),
]

# Backtick raw strings (Go) only need to be handled when scanning Go
# sources — Godot GDScript doesn't use backticks. The regex stays
# cheap: every backtick-delimited run on a line is one match.
RAW_STRING = re.compile(r"`([^`\n]*)`")
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


def engine_files(dirs: list[tuple[str, str]] | None = None) -> list[str]:
    """Every file the guard scans, in stable order."""
    if dirs is None:
        dirs = ENGINE_DIRS
    files: list[str] = []
    for d, pattern in dirs:
        files.extend(glob.glob(os.path.join(d, pattern), recursive=True))
    return sorted(files)


def _scan_one(path: str, rel: str, ids: set[str], violations: list[str]) -> None:
    """Run the literal scan on a single file. Populates violations."""
    is_go = path.endswith(".go")
    with open(path, encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, start=1):
            if ALLOW_MARKER in line:
                continue
            for m in STRING_LITERAL.finditer(line):
                literal = m.group(1) if m.group(1) is not None else m.group(2)
                _id = find_content_id(literal, ids)
                if "res://mods/" in literal or literal.startswith("mods/"):
                    violations.append(
                        f"{rel}:{lineno}: engine code references content path "
                        f'"{literal}" — reach content through the loader, not by path'
                    )
                elif _id is not None:
                    violations.append(
                        f"{rel}:{lineno}: engine code hardcodes content id "
                        f'"{_id}" — this belongs in godot/mods/, not the engine'
                    )
            # Go raw strings (backticks). Godot doesn't use them, so only
            # scan when the file is a .go source.
            if is_go:
                for m in RAW_STRING.finditer(line):
                    literal = m.group(1)
                    _id = find_content_id(literal, ids)
                    if "res://mods/" in literal or literal.startswith("mods/"):
                        violations.append(
                            f"{rel}:{lineno}: engine code references content path "
                            f'"{literal}" — reach content through the loader, not by path'
                        )
                    elif _id is not None:
                        violations.append(
                            f"{rel}:{lineno}: engine code hardcodes content id "
                            f'"{_id}" — this belongs in godot/mods/, not the engine'
                        )



def find_content_id(literal, ids):
    """Return the first content id embedded in `literal`, or None.

    A literal "hardcodes" a content id if the id appears as a whole
    token in it — either as the literal's whole value, or as a
    sub-token separated by non-id characters. The token-boundary
    check keeps the guard from flagging substrings that share a
    prefix or suffix with an id (e.g. "compact_storage" is fine;
    id = "compact" is a whole token in `id = "compact"`).
    """
    if not literal:
        return None
    for cid in ids:
        if cid == literal:
            return cid
        idx = 0
        while True:
            pos = literal.find(cid, idx)
            if pos < 0:
                break
            before = literal[pos - 1] if pos > 0 else ""
            after = literal[pos + len(cid)] if pos + len(cid) < len(literal) else ""
            if not (before.isalnum() or before == "_") and not (after.isalnum() or after == "_"):
                return cid
            idx = pos + 1
    return None


def scan(
    engine_dirs: list[tuple[str, str]] | None = None,
    mods_dir: str = MODS_DIR,
    root: str = REPO_ROOT,
) -> list[str]:
    """The full check. Returns a list of violation strings (empty = clean)."""
    if engine_dirs is None:
        engine_dirs = ENGINE_DIRS
    ids = content_ids(mods_dir)
    violations: list[str] = []
    for path in engine_files(engine_dirs):
        rel = os.path.relpath(path, root)
        _scan_one(path, rel, ids, violations)
    return violations


def self_test() -> int:
    """Build a synthetic engine+content tree and assert the guard bites.

    The self-test exercises BOTH engine surfaces (Godot and Go) because
    the scope expansion is the whole point of the new contract — a guard
    that only tested the old Godot surface would still pass with the
    Go side broken. We write a Go file with a hardcoded content id and
    a Go raw string containing a content id, and assert both bite.
    """
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        mods = os.path.join(tmp, "godot", "mods", "demo")
        engine_gd = os.path.join(tmp, "godot", "scripts")
        engine_go = os.path.join(tmp, "server")
        for d in (mods, engine_gd, engine_go):
            os.makedirs(d)
        with open(os.path.join(mods, "manifest.json"), "w", encoding="utf-8") as fh:
            json.dump({"id": "demo", "provides": {"factions": ["acme_syndicate"]}}, fh)

        # A clean engine file must pass: the word appears only as prose/identifier.
        clean = os.path.join(engine_gd, "clean.gd")
        with open(clean, "w", encoding="utf-8") as fh:
            fh.write('# the player can reach the acme_syndicate territory\n')
            fh.write('func go() -> void:\n\tprint("hello")\n')
        if scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp):
            failures.append("clean engine file was flagged (false positive)")

        # Hardcoded content id as a string literal must be caught (Godot).
        with open(os.path.join(engine_gd, "bad_id.gd"), "w", encoding="utf-8") as fh:
            fh.write('func f() -> bool:\n\treturn faction_id == "acme_syndicate"\n')
        if not any("acme_syndicate" in v for v in scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)):
            failures.append("hardcoded content id in Godot was NOT caught")

        # Hardcoded content id in a Go regular string must be caught.
        with open(os.path.join(engine_go, "bad_id.go"), "w", encoding="utf-8") as fh:
            fh.write('package main\n')
            fh.write('var FactionID = "acme_syndicate"\n')
        if not any('bad_id.go' in v and "acme_syndicate" in v
                   for v in scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)):
            failures.append("hardcoded content id in Go was NOT caught (scope expansion broken?)")

        # Hardcoded content id in a Go raw (backtick) string must be caught.
        with open(os.path.join(engine_go, "bad_raw.go"), "w", encoding="utf-8") as fh:
            fh.write('package main\n')
            fh.write('var msg = `faction is acme_syndicate`\n')
        if not any('bad_raw.go' in v and "acme_syndicate" in v
                   for v in scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)):
            failures.append("hardcoded content id in Go raw string was NOT caught")

        # The arch-allow escape hatch must suppress a violation in either engine.
        # Remove all the previously-created bad files so the scan can be
        # checked for the specific allowed files in isolation.
        for stale in ("bad_id.gd", "bad_id.go", "bad_raw.go"):
            try:
                os.remove(os.path.join(engine_gd if stale.endswith(".gd") else engine_go, stale))
            except FileNotFoundError:
                pass
        with open(os.path.join(engine_gd, "allowed.gd"), "w", encoding="utf-8") as fh:
            fh.write('var x = "acme_syndicate"  # arch-allow: bootstrap default\n')
        v_gd = scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)
        if any("allowed.gd" in v for v in v_gd):
            failures.append("arch-allow marker did not suppress Godot violation")
        with open(os.path.join(engine_go, "allowed.go"), "w", encoding="utf-8") as fh:
            fh.write('package main\n')
            fh.write('var msg = "acme_syndicate" // arch-allow: bootstrap default\n')
        v_go = scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)
        if any("allowed.go" in v for v in v_go):
            failures.append("arch-allow marker did not suppress Go violation")

        # A res://mods/ content path must be caught (Godot only — Go can't
        # meaningfully use res:// paths, but the literal would still be
        # flagged if it appeared in a Go string).
        with open(os.path.join(engine_gd, "bad_path.gd"), "w", encoding="utf-8") as fh:
            fh.write('var s = load("res://mods/demo/ships/x.json")\n')
        if not any("res://mods/" in v for v in scan([(engine_gd, "**/*.gd"), (engine_go, "**/*.go")], mods, tmp)):
            failures.append("res://mods/ content path was NOT caught")

    if failures:
        print("self-test FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("self-test OK — guard catches hardcoded ids and content paths in "
          "both Godot and Go engines, ignores prose, honors arch-allow")
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
    godot_count = sum(
        1 for p in engine_files() if p.endswith(".gd")
    )
    go_count = sum(
        1 for p in engine_files() if p.endswith(".go")
    )
    print(
        f"Architecture guard OK — {godot_count} Godot engine file(s), "
        f"{go_count} Go engine file(s), all clean against {len(ids)} content id(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
