#!/usr/bin/env python3
"""Soul Protocol conformance: every golden fixture under
godot/framework/protocol/fixtures/ must validate against the wire schema.

This is the shared, language-neutral half of the conformance suite. Pan runs
the same fixtures through its serde types in its own CI; the Godot bridge is
built against them. If a fixture and the schema disagree, the contract is
broken — fix the contract deliberately (version bump + migration note), never
by quietly editing a fixture.

Run via `make protocol` or `python3 scripts/check_soul_protocol.py`.
"""
from __future__ import annotations

import glob
import json
import os
import sys

from jsonschema_lite import check_schema

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROTOCOL_DIR = os.path.join(REPO_ROOT, "godot", "framework", "protocol")
SCHEMA_PATH = os.path.join(PROTOCOL_DIR, "schemas", "soul_message.schema.json")
FIXTURES_DIR = os.path.join(PROTOCOL_DIR, "fixtures")

DIRECTIONS = {"host_to_daemon", "daemon_to_host"}


def main() -> int:
    errors: list[str] = []
    try:
        with open(SCHEMA_PATH, encoding="utf-8") as fh:
            schema = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"cannot load wire schema: {exc}", file=sys.stderr)
        return 1

    fixtures = sorted(glob.glob(os.path.join(FIXTURES_DIR, "*.json")))
    if not fixtures:
        print(f"no fixtures found under {FIXTURES_DIR}", file=sys.stderr)
        return 1

    seen_types: set[str] = set()
    for path in fixtures:
        rel = os.path.relpath(path, REPO_ROOT)
        try:
            with open(path, encoding="utf-8") as fh:
                fixture = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{rel}: failed to parse: {exc}")
            continue
        if fixture.get("direction") not in DIRECTIONS:
            errors.append(f"{rel}: direction must be one of {sorted(DIRECTIONS)}")
        message = fixture.get("message")
        if not isinstance(message, dict):
            errors.append(f"{rel}: fixture has no `message` object")
            continue
        check_schema(message, schema, rel, errors)
        # The wire is NDJSON: every fixture message must survive compact
        # round-tripping (no NaN/Infinity, no non-string keys).
        try:
            reparsed = json.loads(json.dumps(message, separators=(",", ":"), allow_nan=False))
            if reparsed != message:
                errors.append(f"{rel}: message does not survive a JSON round-trip")
        except ValueError as exc:
            errors.append(f"{rel}: message is not NDJSON-safe: {exc}")
        seen_types.add(str(message.get("type")))
        print(f"ok   {rel}")

    # Coverage: every message type the schema defines must have >= 1 fixture.
    schema_types = {
        branch["properties"]["type"]["const"] for branch in schema.get("oneOf", [])
    }
    for missing in sorted(schema_types - seen_types):
        errors.append(f"schema defines message type '{missing}' but no fixture covers it")

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"\nprotocol conformance OK — {len(fixtures)} fixture(s), all {len(schema_types)} message types covered")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
