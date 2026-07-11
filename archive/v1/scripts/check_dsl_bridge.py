#!/usr/bin/env python3
"""Trigger-DSL conformance bridge — proves the GDScript evaluator matches the
Python reference battery (M5, Sprint 02).

scripts/trigger_dsl.py DEFINES the semantics; godot/scripts/framework/
trigger_dsl.gd re-implements them in-engine. Until this bridge existed the
two had never been run against each other. It:

  1. emits the reference battery (conditions + context + expected outcomes),
  2. runs it through the in-engine evaluator via a headless Godot
     (godot/tools/dsl_bridge.gd),
  3. diffs the outcomes. Booleans must match exactly; cases the reference
     rejects must error in the GDScript evaluator's strict mode.

Godot binary resolution: $GODOT_BIN, then `godot` on PATH, then the Flatpak
(`flatpak run org.godotengine.Godot`). Temp files live inside the repo (NOT
/tmp) so the Flatpak sandbox can read them.

Exit 0 on full agreement, 1 on any divergence, 2 on setup problems.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import trigger_dsl

REPO_ROOT = Path(__file__).resolve().parent.parent
GODOT_PROJECT = REPO_ROOT / "godot"
BRIDGE_SCRIPT = "res://tools/dsl_bridge.gd"


def godot_command() -> list[str]:
    env_bin = os.environ.get("GODOT_BIN")
    if env_bin:
        return [env_bin]
    if shutil.which("godot"):
        return ["godot"]
    if shutil.which("flatpak"):
        return ["flatpak", "run", "org.godotengine.Godot"]
    raise SystemExit("no Godot found: set GODOT_BIN, or install godot / the Flatpak")


def main() -> int:
    battery = trigger_dsl.battery()

    # Inside the repo so a sandboxed (Flatpak) Godot can see the files.
    with tempfile.TemporaryDirectory(dir=REPO_ROOT, prefix=".dsl-bridge-") as tmp:
        battery_path = Path(tmp) / "battery.json"
        out_path = Path(tmp) / "results.json"
        battery_path.write_text(json.dumps(battery), encoding="utf-8")

        cmd = godot_command() + [
            "--headless", "--path", str(GODOT_PROJECT), "--script", BRIDGE_SCRIPT,
            "--", f"--battery={battery_path}", f"--out={out_path}",
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        if not out_path.is_file():
            print("dsl-bridge: the Godot run produced no results file", file=sys.stderr)
            print(proc.stdout[-2000:], file=sys.stderr)
            print(proc.stderr[-2000:], file=sys.stderr)
            return 2
        results = json.loads(out_path.read_text(encoding="utf-8"))

    if results.get("semantics_version") != battery["semantics_version"]:
        print(f"dsl-bridge: semantics version mismatch: reference v{battery['semantics_version']}, "
              f"engine ran v{results.get('semantics_version')}", file=sys.stderr)
        return 1

    by_condition = {r["condition"]: r for r in results.get("results", [])}
    failures = []
    for case in battery["cases"]:
        condition, expected = case["condition"], case["expected"]
        got = by_condition.get(condition)
        if got is None:
            failures.append(f"  {condition!r}: engine produced no outcome")
            continue
        outcome = got["outcome"]
        if expected == "error":
            if outcome != "error":
                failures.append(f"  {condition!r}: reference errors, engine returned {outcome!r}")
        elif outcome != expected:
            detail = f" ({got.get('detail')})" if outcome == "error" else ""
            failures.append(f"  {condition!r}: expected {expected!r}, engine returned {outcome!r}{detail}")

    if failures:
        print(f"DSL conformance bridge FAILED ({len(failures)}/{len(battery['cases'])} cases diverge):",
              file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"DSL conformance bridge OK — {len(battery['cases'])} case(s), GDScript evaluator "
          f"matches the Python reference (semantics v{battery['semantics_version']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
