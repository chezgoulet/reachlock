#!/usr/bin/env python3
"""Ship-Share conformance: every golden fixture under
godot/framework/protocol/share/fixtures/ must validate against the payload
schema (godot/framework/protocol/SHIP-SHARE.md).

Ship-Share payloads ride Godot's high-level ENet RPC as Dictionaries, so
there is no NDJSON wire to test — the schema pins the payload SHAPES so
host and client builds can only drift loudly (a `share_version` bump), and
the engine's seat/intent handlers are driven with these exact payloads in
godot/tests/test_ship_share.gd, no network hardware required.

Run via `make share` or `python3 scripts/check_ship_share.py`.
"""
from __future__ import annotations

import glob
import json
import os
import sys

from jsonschema_lite import check_schema

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROTOCOL_DIR = os.path.join(REPO_ROOT, "godot", "framework", "protocol")
SCHEMA_PATH = os.path.join(PROTOCOL_DIR, "schemas", "share_message.schema.json")
FIXTURES_DIR = os.path.join(PROTOCOL_DIR, "share", "fixtures")

DIRECTIONS = {"client_to_host", "host_to_client"}

# Intents may only flow client -> host, state host -> client. A fixture on
# the wrong side of the table is a contract bug.
INTENT_KINDS = {"hello", "claim_seat", "release_seat", "move", "station",
                "control", "choose", "say"}
STATE_KINDS = {"welcome", "refuse", "seat_denied", "roster", "seats", "pawn",
               "station_state", "dialogue_line", "dialogue_choices",
               "dialogue_ended", "world"}


def main() -> int:
    errors: list[str] = []
    try:
        with open(SCHEMA_PATH, encoding="utf-8") as fh:
            schema = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"cannot load payload schema: {exc}", file=sys.stderr)
        return 1

    fixtures = sorted(glob.glob(os.path.join(FIXTURES_DIR, "*.json")))
    if not fixtures:
        print(f"no fixtures found under {FIXTURES_DIR}", file=sys.stderr)
        return 1

    seen_kinds: set[str] = set()
    for path in fixtures:
        rel = os.path.relpath(path, REPO_ROOT)
        try:
            with open(path, encoding="utf-8") as fh:
                fixture = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{rel}: failed to parse: {exc}")
            continue
        direction = fixture.get("direction")
        if direction not in DIRECTIONS:
            errors.append(f"{rel}: direction must be one of {sorted(DIRECTIONS)}")
        message = fixture.get("message")
        if not isinstance(message, dict):
            errors.append(f"{rel}: fixture has no `message` object")
            continue
        check_schema(message, schema, rel, errors)
        kind = str(message.get("kind"))
        if direction == "client_to_host" and kind in STATE_KINDS:
            errors.append(f"{rel}: '{kind}' is state; it flows host -> client only")
        if direction == "host_to_client" and kind in INTENT_KINDS:
            errors.append(f"{rel}: '{kind}' is an intent; it flows client -> host only")
        try:
            reparsed = json.loads(json.dumps(message, separators=(",", ":"), allow_nan=False))
            if reparsed != message:
                errors.append(f"{rel}: message does not survive a JSON round-trip")
        except ValueError as exc:
            errors.append(f"{rel}: message is not JSON-safe: {exc}")
        seen_kinds.add(kind)
        print(f"ok   {rel}")

    schema_kinds = {
        branch["properties"]["kind"]["const"] for branch in schema.get("oneOf", [])
    }
    for missing in sorted(schema_kinds - seen_kinds):
        errors.append(f"schema defines payload kind '{missing}' but no fixture covers it")
    if schema_kinds != INTENT_KINDS | STATE_KINDS:
        errors.append("checker's intent/state tables disagree with the schema — update both together")

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"\nship-share conformance OK — {len(fixtures)} fixture(s), all {len(schema_kinds)} payload kinds covered")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
