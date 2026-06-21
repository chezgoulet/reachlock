#!/usr/bin/env python3
"""Validate REACHLOCK mod data: every JSON file under godot/mods parses, and
each mod manifest declares the fields the loader will require.

Run locally with `make validate` or `python3 scripts/validate_mod_data.py`.
Exits non-zero on the first batch of problems so CI fails loudly.
"""
from __future__ import annotations

import glob
import json
import os
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.join(REPO_ROOT, "godot", "mods")

# Keys the mod loader (#8) will depend on. Keep in sync with the loader.
REQUIRED_MANIFEST_KEYS = {"id", "name", "version", "provides"}


def main() -> int:
    json_files = sorted(glob.glob(os.path.join(MODS_DIR, "**", "*.json"), recursive=True))
    if not json_files:
        print(f"ERROR: no mod JSON files found under {MODS_DIR}", file=sys.stderr)
        return 1

    errors: list[str] = []
    manifest_count = 0

    for path in json_files:
        rel = os.path.relpath(path, REPO_ROOT)
        try:
            with open(path, encoding="utf-8") as fh:
                data = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{rel}: failed to parse: {exc}")
            continue

        if os.path.basename(path) == "manifest.json":
            manifest_count += 1
            if not isinstance(data, dict):
                errors.append(f"{rel}: manifest must be a JSON object")
                continue
            missing = REQUIRED_MANIFEST_KEYS - data.keys()
            if missing:
                errors.append(f"{rel}: manifest missing required keys: {sorted(missing)}")

        print(f"ok   {rel}")

    if manifest_count == 0:
        errors.append("no manifest.json found under any mod directory")

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"\nvalidated {len(json_files)} file(s), {manifest_count} manifest(s) — all good")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
