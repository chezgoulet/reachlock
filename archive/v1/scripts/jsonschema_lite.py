"""A deliberately small, dependency-free subset of JSON Schema.

Shared by validate_mod_data.py (entity contracts) and check_soul_protocol.py
(wire contract fixtures) so CI needs nothing but python3.

Supported keywords: type, required, properties, additionalProperties, items,
enum, const, pattern, minimum, maximum, minItems, oneOf, $ref (local
"#/$defs/..." within the root schema only), $defs. Keep every schema in the
repo within this subset — the point is that the subset is small enough to be
obviously correct and portable to GDScript/Rust/Go checkers.
"""
from __future__ import annotations

import re

_TYPE_CHECKS = {
    "object": lambda v: isinstance(v, dict),
    "array": lambda v: isinstance(v, list),
    "string": lambda v: isinstance(v, str),
    "boolean": lambda v: isinstance(v, bool),
    "integer": lambda v: isinstance(v, int) and not isinstance(v, bool),
    "number": lambda v: isinstance(v, (int, float)) and not isinstance(v, bool),
    "null": lambda v: v is None,
}


def _resolve_ref(ref: str, root: dict) -> dict:
    if not ref.startswith("#/"):
        raise ValueError(f"only local $ref supported, got {ref!r}")
    node: object = root
    for part in ref[2:].split("/"):
        if not isinstance(node, dict) or part not in node:
            raise ValueError(f"$ref {ref!r} does not resolve")
        node = node[part]
    if not isinstance(node, dict):
        raise ValueError(f"$ref {ref!r} is not a schema object")
    return node


def check_schema(
    value: object, schema: dict, path: str, errors: list[str], root: dict | None = None
) -> None:
    """Append human-readable problems to `errors`. `root` is the document the
    schema came from, used to resolve local $refs; defaults to `schema`."""
    if root is None:
        root = schema

    if "$ref" in schema:
        check_schema(value, _resolve_ref(schema["$ref"], root), path, errors, root)
        return

    if "oneOf" in schema:
        branch_errors: list[list[str]] = []
        for branch in schema["oneOf"]:
            errs: list[str] = []
            check_schema(value, branch, path, errs, root)
            if not errs:
                break
            branch_errors.append(errs)
        else:
            shortest = min(branch_errors, key=len) if branch_errors else []
            errors.append(f"{path}: matched no oneOf branch (closest: {'; '.join(shortest[:2])})")
        return

    stype = schema.get("type")
    if stype is not None:
        types = stype if isinstance(stype, list) else [stype]
        if not any(_TYPE_CHECKS[t](value) for t in types):
            errors.append(f"{path}: expected {'/'.join(types)}, got {type(value).__name__}")
            return

    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: expected const {schema['const']!r}, got {value!r}")

    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: {value!r} is not one of {schema['enum']}")

    if isinstance(value, str) and "pattern" in schema:
        if not re.search(schema["pattern"], value):
            errors.append(f"{path}: {value!r} does not match pattern {schema['pattern']!r}")

    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            errors.append(f"{path}: {value} is below minimum {schema['minimum']}")
        if "maximum" in schema and value > schema["maximum"]:
            errors.append(f"{path}: {value} is above maximum {schema['maximum']}")

    if isinstance(value, dict):
        for key in schema.get("required", []):
            if key not in value:
                errors.append(f"{path}: missing required key {key!r}")
        props = schema.get("properties", {})
        additional = schema.get("additionalProperties", True)
        for key, item in value.items():
            child = f"{path}.{key}"
            if key in props:
                check_schema(item, props[key], child, errors, root)
            elif isinstance(additional, dict):
                check_schema(item, additional, child, errors, root)
            elif additional is False:
                errors.append(f"{path}: unknown key {key!r} (typo? custom data belongs under `extra`)")

    if isinstance(value, list):
        if "minItems" in schema and len(value) < schema["minItems"]:
            errors.append(f"{path}: fewer than {schema['minItems']} item(s)")
        items = schema.get("items")
        if isinstance(items, dict):
            for i, item in enumerate(value):
                check_schema(item, items, f"{path}[{i}]", errors, root)
