#!/usr/bin/env python3
"""Validate REACHLOCK mod data against the framework contracts.

Every JSON file under godot/mods must parse, every manifest and entity file
must conform to its schema in godot/framework/schemas/, and each mod's data
files must agree with what its manifest `provides`.

The schema checker is a deliberately small, dependency-free subset of JSON
Schema (type, required, properties, additionalProperties, items, enum,
pattern, minimum, maximum, minItems) so CI needs nothing but python3. Keep
the schemas within that subset.

Run locally with `make validate` or `python3 scripts/validate_mod_data.py`.
Errors fail the build; warnings (declared-but-not-yet-authored ids) do not.
"""
from __future__ import annotations

import glob
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODS_DIR = os.path.join(REPO_ROOT, "godot", "mods")
SCHEMAS_DIR = os.path.join(REPO_ROOT, "godot", "framework", "schemas")

# provides-kind -> schema basename. Kinds without a schema (yet) only need to
# parse; add the schema here when the contract lands.
KIND_SCHEMAS = {
    "factions": "faction",
    "ships": "ship",
    "npcs": "npc",
    "locations": "location",
    "dialogues": "dialogue",
    "goods": "good",
    "faction_actions": "faction_action",
    "missions": "mission",
}


from jsonschema_lite import check_schema


# --- loading ----------------------------------------------------------------


def load_json(path: str, errors: list[str]) -> object | None:
    rel = os.path.relpath(path, REPO_ROOT)
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        errors.append(f"{rel}: failed to parse: {exc}")
        return None


def load_schemas(errors: list[str]) -> dict[str, dict]:
    schemas: dict[str, dict] = {}
    for path in sorted(glob.glob(os.path.join(SCHEMAS_DIR, "*.schema.json"))):
        name = os.path.basename(path).removesuffix(".schema.json")
        data = load_json(path, errors)
        if isinstance(data, dict):
            schemas[name] = data
    if "manifest" not in schemas:
        errors.append(f"framework schema missing: {os.path.join(SCHEMAS_DIR, 'manifest.schema.json')}")
    return schemas


def check_dialogue_graph(data: dict, rel: str, errors: list[str]) -> None:
    """Beyond the schema: the graph must be sound and every condition must be
    a valid trigger-DSL expression. A typo'd condition dies here, in CI."""
    import trigger_dsl

    nodes = data.get("nodes")
    if not isinstance(nodes, dict):
        return
    targets = set(nodes) | {"end"}
    if data.get("entry") not in nodes:
        errors.append(f"{rel}: entry node {data.get('entry')!r} does not exist")

    def check_condition(cond: object, where: str) -> None:
        if isinstance(cond, str):
            try:
                trigger_dsl.parse(cond)
            except trigger_dsl.ParseError as exc:
                errors.append(f"{rel}: {where}: bad condition {cond!r}: {exc}")

    check_condition(data.get("condition"), "dialogue guard")
    for node_id, node in nodes.items():
        if not isinstance(node, dict):
            continue
        kind = node.get("kind")
        if kind == "authored" and "text" not in node:
            errors.append(f"{rel}: node {node_id!r} is authored but has no text")
        if kind == "generated" and "prompt_hint" not in node:
            errors.append(f"{rel}: node {node_id!r} is generated but has no prompt_hint")
        choices = node.get("choices")
        if not choices and node.get("goto") not in targets:
            errors.append(f"{rel}: node {node_id!r} has no choices and no valid goto (dead end)")
        for i, choice in enumerate(choices or []):
            if isinstance(choice, dict):
                if choice.get("goto") not in targets:
                    errors.append(f"{rel}: node {node_id!r} choice {i}: goto {choice.get('goto')!r} does not exist")
                check_condition(choice.get("condition"), f"node {node_id!r} choice {i}")


def validate_mod(mod_dir: str, schemas: dict[str, dict], errors: list[str], warnings: list[str]) -> int:
    """Validate one mod directory. Returns the number of files checked."""
    checked = 0
    manifest_path = os.path.join(mod_dir, "manifest.json")
    rel_manifest = os.path.relpath(manifest_path, REPO_ROOT)
    manifest = load_json(manifest_path, errors)
    checked += 1
    if not isinstance(manifest, dict):
        if manifest is not None:
            errors.append(f"{rel_manifest}: manifest must be a JSON object")
        return checked
    if "manifest" in schemas:
        check_schema(manifest, schemas["manifest"], rel_manifest, errors)

    provides = manifest.get("provides", {})
    if not isinstance(provides, dict):
        return checked

    seen_dirs = {
        d for d in os.listdir(mod_dir) if os.path.isdir(os.path.join(mod_dir, d))
    }
    for extra_dir in sorted(seen_dirs - set(provides)):
        warnings.append(f"{rel_manifest}: directory {extra_dir}/ has no matching `provides` kind — its files are never loaded")

    for kind, declared in sorted(provides.items()):
        if not isinstance(declared, list):
            continue
        declared_set = set(declared)
        found: set[str] = set()
        for path in sorted(glob.glob(os.path.join(mod_dir, kind, "*.json"))):
            rel = os.path.relpath(path, REPO_ROOT)
            data = load_json(path, errors)
            checked += 1
            if not isinstance(data, dict):
                continue
            schema_name = KIND_SCHEMAS.get(kind)
            if schema_name and schema_name in schemas:
                check_schema(data, schemas[schema_name], rel, errors)
            if kind == "dialogues":
                check_dialogue_graph(data, rel, errors)
            entity_id = data.get("id")
            if not isinstance(entity_id, str):
                errors.append(f"{rel}: entity file has no string `id`")
                continue
            basename = os.path.basename(path).removesuffix(".json")
            if basename != entity_id:
                errors.append(f"{rel}: filename {basename!r} != id {entity_id!r} — rename one")
            if entity_id in found:
                errors.append(f"{rel}: duplicate id {entity_id!r} within {kind}/")
            found.add(entity_id)
            if entity_id not in declared_set:
                errors.append(f"{rel}: id {entity_id!r} is not declared in manifest provides.{kind}")
        for missing in sorted(declared_set - found):
            warnings.append(f"{rel_manifest}: provides.{kind} declares {missing!r} but no data file exists yet")

    return checked


def main() -> int:
    errors: list[str] = []
    warnings: list[str] = []

    schemas = load_schemas(errors)

    mod_dirs = sorted(
        os.path.dirname(p)
        for p in glob.glob(os.path.join(MODS_DIR, "*", "manifest.json"))
    )
    if not mod_dirs:
        errors.append(f"no mod with a manifest.json found under {MODS_DIR}")

    checked = 0
    for mod_dir in mod_dirs:
        print(f"mod  {os.path.relpath(mod_dir, REPO_ROOT)}")
        checked += validate_mod(mod_dir, schemas, errors, warnings)

    for w in warnings:
        print(f"warn {w}")

    if errors:
        print(f"\n{len(errors)} problem(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(
        f"\nvalidated {checked} file(s) across {len(mod_dirs)} mod(s) "
        f"against {len(schemas)} schema(s) — all good ({len(warnings)} warning(s))"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
