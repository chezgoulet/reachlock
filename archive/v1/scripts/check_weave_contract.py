#!/usr/bin/env python3
"""Weave contract conformance: the reference resolver over golden and
adversarial fixtures.

This file IS the reference implementation of weave resolution
(godot/framework/WEAVE-CONTRACT.md): given a `woven` node's `may` allowlist
and a mind's proposal, produce the clamped, deterministic resolution the
save persists. Every fixture under godot/framework/weave/fixtures/ carries
the expected resolution; the engine implementation
(godot/scripts/framework/weave_loom.gd) must produce an identical
resolution for every fixture (godot/tests/test_weave_loom.gd) — the
trigger-DSL bridge pattern.

The adversarial fixtures are the point: a proposal that exceeds its
allowlist must be provably neutered here, in CI, forever.

Run via `make weave` or `python3 scripts/check_weave_contract.py`.
"""
from __future__ import annotations

import glob
import json
import os
import sys

from jsonschema_lite import check_schema

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FRAMEWORK_DIR = os.path.join(REPO_ROOT, "godot", "framework")
PROPOSAL_SCHEMA_PATH = os.path.join(FRAMEWORK_DIR, "schemas", "weave_proposal.schema.json")
FIXTURES_DIR = os.path.join(FRAMEWORK_DIR, "weave", "fixtures")

DEFAULT_MAX_CHOICES = 3
DEFAULT_MAX_MUTATIONS = 4


# --- the reference resolver (normative; weave_loom.gd must mirror) -----------


def _grant_permits(grant: dict, mutation: dict) -> tuple[bool, dict | None]:
    """Does this grant permit this mutation? Returns (permitted, clamped)."""
    op = mutation.get("op")
    if op == "adjust_relationship" and grant.get("op") == op:
        if mutation.get("target") not in grant.get("targets", []):
            return False, None
        if mutation.get("axis") not in grant.get("axes", []):
            return False, None
        cap = int(grant.get("max_amount", 0))
        clamped = dict(mutation)
        clamped["amount"] = max(-cap, min(cap, int(mutation.get("amount", 0))))
        return True, clamped
    if op == "adjust_faction" and grant.get("op") == op:
        if mutation.get("faction") not in grant.get("factions", []):
            return False, None
        if mutation.get("axis") not in grant.get("axes", []):
            return False, None
        cap = int(grant.get("max_amount", 0))
        clamped = dict(mutation)
        clamped["amount"] = max(-cap, min(cap, int(mutation.get("amount", 0))))
        return True, clamped
    if op in ("set_flag", "clear_flag", "set_player_flag", "clear_player_flag") \
            and grant.get("op") == op:
        if mutation.get("flag") not in grant.get("flags", []):
            return False, None
        return True, dict(mutation)
    if op == "add_memory" and grant.get("op") == op:
        cap = float(grant.get("max_importance", 0.0))
        clamped = dict(mutation)
        clamped["importance"] = min(cap, float(mutation.get("importance", 0.5)))
        return True, clamped
    return False, None


def _filter_mutations(mutations: list, grants: list, log: list[str]) -> list:
    kept = []
    for mutation in mutations:
        for grant in grants:
            permitted, clamped = _grant_permits(grant, mutation)
            if permitted:
                if clamped != mutation:
                    log.append(f"clamped {mutation.get('op')}: {mutation} -> {clamped}")
                kept.append(clamped)
                break
        else:
            log.append(f"dropped {mutation.get('op')}: no grant permits {mutation}")
    return kept


def resolve(node: dict, proposal, log: list[str]) -> dict | None:
    """WEAVE-CONTRACT.md resolution. Returns the resolution, or None for
    discard-whole (engine plays the authored fallback)."""
    schema_errors: list[str] = []
    with open(PROPOSAL_SCHEMA_PATH, encoding="utf-8") as fh:
        check_schema(proposal, json.load(fh), "proposal", schema_errors)
    if schema_errors or not str(proposal.get("line", "")).strip():
        log.append("discarded: proposal fails the proposal schema")
        return None

    may = node.get("may", {})
    grants = may.get("grants", [])
    max_choices = int(may.get("max_choices", DEFAULT_MAX_CHOICES))
    max_mutations = int(may.get("max_mutations", DEFAULT_MAX_MUTATIONS))
    return_to = node.get("return_to", "end")

    choices = list(proposal.get("choices", []))
    if len(choices) > max_choices:
        log.append(f"dropped {len(choices) - max_choices} choice(s) past max_choices")
        choices = choices[:max_choices]

    budget = max_mutations

    def take(mutations: list) -> list:
        nonlocal budget
        kept = _filter_mutations(mutations, grants, log)
        if len(kept) > budget:
            log.append(f"dropped {len(kept) - budget} mutation(s) past max_mutations")
            kept = kept[:budget]
        budget -= len(kept)
        return kept

    resolved = {
        "line": proposal["line"],
        "mutations": take(proposal.get("mutations", [])),
        "choices": [
            {
                "text": choice["text"],
                "mutations": take(choice.get("mutations", [])),
                "goto": return_to,
            }
            for choice in choices
        ],
    }
    return resolved


# --- conformance runner -------------------------------------------------------


def main() -> int:
    errors: list[str] = []
    fixtures = sorted(glob.glob(os.path.join(FIXTURES_DIR, "*.json")))
    if not fixtures:
        print(f"no fixtures found under {FIXTURES_DIR}", file=sys.stderr)
        return 1

    for path in fixtures:
        rel = os.path.relpath(path, REPO_ROOT)
        try:
            with open(path, encoding="utf-8") as fh:
                fixture = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{rel}: failed to parse: {exc}")
            continue
        node = fixture.get("node")
        proposal = fixture.get("proposal")
        if not isinstance(node, dict) or "proposal" not in fixture:
            errors.append(f"{rel}: fixture needs `node` and `proposal`")
            continue
        log: list[str] = []
        got = resolve(node, proposal, log)
        expected = fixture.get("resolved")
        if got != expected:
            errors.append(
                f"{rel}: resolution mismatch\n    expected: {json.dumps(expected)}\n"
                f"    got:      {json.dumps(got)}\n    log: {log}")
            continue
        # A resolution must itself survive the allowlist: re-resolving the
        # already-clamped output must be a fixed point (nothing to clamp).
        if got is not None:
            relog: list[str] = []
            again = resolve(node, {
                "line": got["line"],
                "mutations": got["mutations"],
                "choices": [
                    {"text": c["text"], "mutations": c["mutations"]} for c in got["choices"]
                ],
            }, relog)
            if again != got:
                errors.append(f"{rel}: resolution is not a fixed point: {relog}")
                continue
        print(f"ok   {rel}" + (f"  ({len(log)} clamp/drop)" if log else ""))

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"\nweave conformance OK — {len(fixtures)} fixture(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
