#!/usr/bin/env python3
"""Fail on a crate-level `#![allow(dead_code)]`.

A crate root that allows dead code switches off the compiler's own
unreachability detection for everything below it. That is how the client
carried ~75 findings — a whole unused widget kit, a duplicate breaking-point
model, ships and careers that reached no player — while `make check` stayed
green, and it is why `check_resources.py` had to be written by hand to
re-detect one special case of what the compiler already knew.

Targeted `#[allow(dead_code)]` on an item, or `#![allow(dead_code)]` on a
module that documents why, is fine: each one names what is waiting and stays
greppable. Blanket suppression at a crate root is not.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE_ROOTS = ["main.rs", "lib.rs"]
PATTERN = re.compile(r"^\s*#!\[allow\(dead_code\)\]", re.M)

def main() -> int:
    offenders = []
    for crate in sorted(ROOT.glob("reachlock-*/src")):
        for name in CRATE_ROOTS:
            path = crate / name
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8")
            for m in PATTERN.finditer(text):
                line = text[: m.start()].count("\n") + 1
                offenders.append(f"{path.relative_to(ROOT)}:{line}")

    if offenders:
        print("DEAD-CODE VIOLATION: crate-level #![allow(dead_code)]\n")
        for o in offenders:
            print(f"  {o}")
        print(
            "\nThis disables dead-code detection for the whole crate. Wire the\n"
            "code, delete it, or put a narrow #[allow(dead_code)] on the specific\n"
            "item with a comment saying what it is waiting for."
        )
        return 1

    print("dead-code OK (no crate-level allow)")
    return 0

if __name__ == "__main__":
    sys.exit(main())
