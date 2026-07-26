#!/usr/bin/env python3
"""Unregistered-resource guard.

A Bevy system taking `Res<T>` or `ResMut<T>` fails *at runtime* if nothing ever
inserted `T`, and Bevy's default error handler turns that into a panic. The
compiler cannot catch it: declaring the resource, writing the system, and
registering the system all typecheck perfectly. Only registering the resource
is missing, and nothing checks that.

`CulturePanelVisible` shipped that way — declared, read by two systems, wired
to a keybind, and registered nowhere. Entering the game panicked with
`Parameter ResMut<CulturePanelVisible> failed validation: Resource does not
exist`, and with the `debug-names` feature off (the default) the message could
not even name the system.

What counts as registered
-------------------------
`init_resource::<T>()`, `init_non_send_resource::<T>()`, `insert_resource(T…)`
or `insert_non_send_resource(T…)` anywhere in the crate — including inside a
plugin's `build`, and including `commands.insert_resource(...)` from a system,
which is how `ContentIndex` legitimately arrives at `Startup`.

What is deliberately not flagged
--------------------------------
`Option<Res<T>>` and `If<Res<T>>`: those are the documented ways to say "this
resource may not exist yet", and Bevy skips or nulls them instead of panicking.
Sub-state resources (`State<T>`/`NextState<T>`) are managed by Bevy itself.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ["reachlock-client/src"]

# `#[derive(..., Resource, ...)]` … `pub struct Foo` / `pub enum Foo`
DERIVE_RE = re.compile(
    r"#\[derive\([^)]*\bResource\b[^)]*\)\]"      # the derive
    r"(?:\s*#\[[^\]]*\])*"                        # any attributes between
    r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)",
    re.S,
)

REGISTER_RE = re.compile(
    r"(?:init_resource|init_non_send_resource)::<\s*([\w:]+)\s*>|"
    r"(?:insert_resource|insert_non_send_resource)\s*\(\s*([\w:]+)"
)

# `Res<Foo>` / `ResMut<'_, Foo>` in a system signature, but NOT when wrapped in
# Option<…> or If<…>.
USE_RE = re.compile(r"(?<!Option<)(?<!If<)\bRes(?:Mut)?\s*<\s*(?:'[\w_]+\s*,\s*)?([\w:]+)\s*>")

# Bevy owns these.
BUILTIN = {"State", "NextState", "Time", "AssetServer", "Assets", "Commands"}

# Resources Bevy *used to* provide. These still typecheck — the types exist,
# they are just no longer inserted as resources — so nothing but a runtime
# panic reveals them. `Res<ButtonInput<GamepadButton>>` outlived Bevy 0.15,
# where gamepads became entities and their state moved onto the `Gamepad`
# component; the system compiled for three releases and panicked the first
# time it ran.
#
# The generic argument is what matters here, so these are matched against the
# full `Res<...>` text rather than a bare type name.
REMOVED_BY_BEVY = {
    "ButtonInput<GamepadButton>": "gamepads are entities since Bevy 0.15 — "
    "query `&Gamepad` and call `gamepad.just_pressed(..)`",
    "Axis<GamepadAxis>": "gamepads are entities since Bevy 0.15 — "
    "query `&Gamepad` and call `gamepad.get(..)`",
    "Axis<GamepadButton>": "gamepads are entities since Bevy 0.15 — "
    "query `&Gamepad`",
}

# Matches the whole generic payload, e.g. `ButtonInput<GamepadButton>`.
USE_GENERIC_RE = re.compile(
    r"(?<!Option<)(?<!If<)\bRes(?:Mut)?\s*<\s*(?:'[\w_]+\s*,\s*)?([\w:]+\s*<[^>]*>)\s*>"
)


# `fn load_settings() -> Settings` — used to resolve a registration written as
# `insert_resource(settings::load_settings())`, where the expression names no
# type at all.
FN_RETURN_RE = re.compile(r"\bfn\s+(\w+)\s*\([^)]*\)\s*->\s*([\w:]+)")


def type_of(path: str, fn_returns: dict[str, str] | None = None) -> str | None:
    """The type name in a registration expression.

    `init_resource::<crate::a::Foo>` and `insert_resource(Foo::from_env()`
    both arrive as a `::`-joined path, but the last segment means different
    things: the type in the first case, a constructor in the second. The type
    is the last segment that looks like one — `Foo` in `Foo::from_env`, and
    still `Foo` in `crate::a::Foo`.

    When nothing in the path is a type, the expression is a plain function
    call (`settings::load_settings()`), so fall back to that function's
    declared return type. Guessing instead would report `Settings` — a
    resource 44 files depend on — as missing, and a guard that cries wolf
    gets switched off.
    """
    for segment in reversed(path.split("::")):
        if segment[:1].isupper():
            return segment
    if fn_returns:
        return fn_returns.get(path.rsplit("::", 1)[-1])
    return None


def rust_files():
    for crate in CRATES:
        yield from sorted((ROOT / crate).rglob("*.rs"))


def strip_comments(text: str) -> str:
    """Drop line comments.

    Without this the guard flags the doc comment that *explains* a removed
    resource — the comment on `detect_gamepad` naming
    `Res<ButtonInput<GamepadButton>>` as the thing it stopped using was
    reported as a live use of it.
    """
    return "\n".join(line.split("//")[0] for line in text.splitlines())


def strip_tests(text: str) -> str:
    """Drop `#[cfg(test)]` modules — fixtures build their own worlds."""
    out, depth, skipping, skip_at = [], 0, False, 0
    for line in text.splitlines(keepends=True):
        if not skipping and line.strip().startswith("#[cfg(test)]"):
            skipping, skip_at = True, depth
            continue
        if skipping:
            depth += line.count("{") - line.count("}")
            if depth <= skip_at and "}" in line:
                skipping = False
            continue
        depth += line.count("{") - line.count("}")
        out.append(line)
    return "".join(out)


def main() -> int:
    declared, registered = {}, set()
    used: dict[str, list[str]] = {}
    removed: dict[str, list[str]] = {}
    fn_returns: dict[str, str] = {}

    # Pass 1: function return types, so a registration written as a plain
    # function call can still be resolved to the resource it produces.
    for path in rust_files():
        for name, ret in FN_RETURN_RE.findall(strip_comments(path.read_text(encoding="utf-8"))):
            fn_returns[name] = ret.rsplit("::", 1)[-1]

    for path in rust_files():
        raw = path.read_text(encoding="utf-8")
        text = strip_tests(strip_comments(raw))
        rel = str(path.relative_to(ROOT))

        for name in DERIVE_RE.findall(text):
            declared[name] = rel
        for a, b in REGISTER_RE.findall(text):
            name = type_of(a or b, fn_returns)
            if name:
                registered.add(name)
        for name in USE_RE.findall(text):
            short = name.rsplit("::", 1)[-1]
            if short in BUILTIN:
                continue
            used.setdefault(short, []).append(rel)
        for generic in USE_GENERIC_RE.findall(text):
            key = re.sub(r"\s+", "", generic).rsplit("::", 1)[-1]
            if key in REMOVED_BY_BEVY:
                removed.setdefault(key, []).append(rel)

    problems = []
    for name, sites in sorted(removed.items()):
        where = sorted(set(sites))
        problems.append(f"{name} is no longer a Bevy resource — {REMOVED_BY_BEVY[name]}:")
        problems.extend(f"    {w}" for w in where[:4])
    for name, sites in sorted(used.items()):
        if name not in declared:
            continue  # declared in another crate; not ours to register
        if name in registered:
            continue
        where = sorted(set(sites))
        problems.append(
            f"{name} (declared in {declared[name]}) is read by "
            f"{len(where)} file(s) but never registered:"
        )
        problems.extend(f"    {w}" for w in where[:4])

    if problems:
        print("UNREGISTERED RESOURCES", file=sys.stderr)
        print("", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("", file=sys.stderr)
        print(
            "  Each of these panics the moment its system first runs.\n"
            "  Add `.init_resource::<T>()` in main.rs, or take `Option<Res<T>>`\n"
            "  if absence is a real state the system should handle.",
            file=sys.stderr,
        )
        return 1

    print(f"resources OK ({len(declared)} declared, {len(used)} read, all registered)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
