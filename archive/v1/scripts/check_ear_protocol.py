#!/usr/bin/env python3
"""Ear Protocol conformance: wire fixtures + the choice-matcher reference.

Two halves, both normative (godot/framework/protocol/EAR-PROTOCOL.md):

1. Every golden fixture under godot/framework/protocol/ear/fixtures/ must
   validate against the wire schema — the same shared conformance idea as
   the Soul and Sim Protocols. reachlock-eard round-trips the same fixtures
   through its Go wire types in `go test ./internal/eard/`.

2. This file IS the reference implementation of the deterministic choice
   matcher. Every case in godot/framework/protocol/ear/match_cases.json
   must produce its expected verdict here, and the GDScript matcher
   (godot/scripts/framework/ear_match.gd) must agree on every case
   (godot/tests/test_ear_match.gd) — the trigger-DSL bridge pattern.
   Tuning the constants or the stopword list is a contract change: fix the
   cases deliberately, never by quietly editing an expectation.

Run via `make ear` or `python3 scripts/check_ear_protocol.py`.
"""
from __future__ import annotations

import glob
import json
import os
import sys

from jsonschema_lite import check_schema

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROTOCOL_DIR = os.path.join(REPO_ROOT, "godot", "framework", "protocol")
SCHEMA_PATH = os.path.join(PROTOCOL_DIR, "schemas", "ear_message.schema.json")
FIXTURES_DIR = os.path.join(PROTOCOL_DIR, "ear", "fixtures")
MATCH_CASES_PATH = os.path.join(PROTOCOL_DIR, "ear", "match_cases.json")

DIRECTIONS = {"host_to_daemon", "daemon_to_host"}

# --- the choice matcher (normative constants; ear_match.gd must mirror) ------

MATCH_THRESHOLD = 0.5
MATCH_MARGIN = 0.1
STOPWORD_WEIGHT = 0.25

# Closed list. Function words that carry no intent on their own; everything
# else weighs 1.0. Growing this list is a contract change.
STOPWORDS = frozenset("""
a an the and or but so if then than that this these those it its im id ill
ive is are was were be been being do does did doing dont didnt not no yes
i you he she we they me him her us them my your his hers our their to of
in on at for with from by as about into over under out up down off again
just very really too also there here what when where who whom why how all
any both each few more most other some such only own same can cant will
wont would could should might must let lets got get have has had having
going gonna
""".split())


def normalize(text: str) -> list[str]:
    """Lowercase, strip apostrophes, everything else non-alnum -> space."""
    lowered = text.lower().replace("'", "").replace("’", "")
    cleaned = "".join(c if c.isalnum() else " " for c in lowered)
    return cleaned.split()


def weight(tokens: set[str]) -> float:
    return sum(STOPWORD_WEIGHT if t in STOPWORDS else 1.0 for t in tokens)


def score(transcript_tokens: set[str], choice_tokens: set[str]) -> float:
    """Weighted Dice overlap over token sets."""
    denominator = weight(transcript_tokens) + weight(choice_tokens)
    if denominator == 0:
        return 0.0
    return 2.0 * weight(transcript_tokens & choice_tokens) / denominator


def match(transcript: str, choices: list[str]) -> int:
    """The normative verdict: index of the matched choice, or -1."""
    transcript_tokens = set(normalize(transcript))
    if not transcript_tokens or all(t in STOPWORDS for t in transcript_tokens):
        return -1
    scores = [score(transcript_tokens, set(normalize(c))) for c in choices]
    if not scores:
        return -1
    best = max(range(len(scores)), key=lambda i: (scores[i], -i))
    runner_up = max((scores[i] for i in range(len(scores)) if i != best), default=0.0)
    if scores[best] >= MATCH_THRESHOLD and scores[best] - runner_up >= MATCH_MARGIN:
        return best
    return -1


# --- conformance runner -------------------------------------------------------


def check_fixtures(errors: list[str]) -> int:
    try:
        with open(SCHEMA_PATH, encoding="utf-8") as fh:
            schema = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        errors.append(f"cannot load wire schema: {exc}")
        return 0

    fixtures = sorted(glob.glob(os.path.join(FIXTURES_DIR, "*.json")))
    if not fixtures:
        errors.append(f"no fixtures found under {FIXTURES_DIR}")
        return 0

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
        try:
            reparsed = json.loads(json.dumps(message, separators=(",", ":"), allow_nan=False))
            if reparsed != message:
                errors.append(f"{rel}: message does not survive a JSON round-trip")
        except ValueError as exc:
            errors.append(f"{rel}: message is not NDJSON-safe: {exc}")
        seen_types.add(str(message.get("type")))
        print(f"ok   {rel}")

    schema_types = {
        branch["properties"]["type"]["const"] for branch in schema.get("oneOf", [])
    }
    for missing in sorted(schema_types - seen_types):
        errors.append(f"schema defines message type '{missing}' but no fixture covers it")
    return len(fixtures)


def check_match_cases(errors: list[str]) -> int:
    rel = os.path.relpath(MATCH_CASES_PATH, REPO_ROOT)
    try:
        with open(MATCH_CASES_PATH, encoding="utf-8") as fh:
            cases = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        errors.append(f"{rel}: failed to parse: {exc}")
        return 0
    if not isinstance(cases, list) or not cases:
        errors.append(f"{rel}: expected a non-empty array of cases")
        return 0

    for i, case in enumerate(cases):
        name = case.get("name", f"case {i}")
        transcript = case.get("transcript")
        choices = case.get("choices")
        expect = case.get("expect")
        if not isinstance(transcript, str) or not isinstance(choices, list) \
                or not isinstance(expect, int):
            errors.append(f"{rel}: {name}: needs transcript(str), choices(list), expect(int)")
            continue
        got = match(transcript, choices)
        if got != expect:
            errors.append(
                f"{rel}: {name}: expected {expect}, matcher says {got} "
                f"(transcript {transcript!r})")
        else:
            print(f"ok   match: {name}")
    return len(cases)


def main() -> int:
    errors: list[str] = []
    fixture_count = check_fixtures(errors)
    case_count = check_match_cases(errors)

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"\near conformance OK — {fixture_count} fixture(s), {case_count} match case(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
