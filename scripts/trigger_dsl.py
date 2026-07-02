#!/usr/bin/env python3
"""The REACHLOCK trigger-condition DSL — reference implementation.

This is the condition language of storyline cards and dialogue branches
(GAME-DESIGN.md §8):

    faction.compact.trust < -50 and player.location == "verne"
    not (soul.tib.trust >= 40) or "duskway_veteran" in player.flags

THIS FILE DEFINES THE SEMANTICS. The in-engine GDScript evaluator and any
server-side evaluator must agree with it; the self-test battery below is the
contract. Run `python3 scripts/trigger_dsl.py --self-test` (wired into
`make check`).

Grammar (v0 — frozen; extensions are additive and bump the version):

    expr       := or_expr
    or_expr    := and_expr ( "or" and_expr )*
    and_expr   := unary ( "and" unary )*
    unary      := "not" unary | comparison
    comparison := operand ( ("=="|"!="|"<"|"<="|">"|">=") operand
                          | "in" operand )?
    operand    := number | string | "true" | "false" | path | "(" expr ")"
    path       := ident ( "." ident )*     # e.g. faction.compact.trust
    ident      := [a-z_][a-z0-9_]*
    string     := '"' [^"]* '"'
    number     := -?digits(.digits)?

Semantics:
- Paths resolve against a nested context (dict of dicts/values). Namespaces
  in the REACHLOCK profile: `faction.*`, `player.*`, `soul.*`, `universe.*`.
- A path that does not resolve is an AUTHORING ERROR: this reference
  evaluator raises EvalError. Engines evaluating at runtime treat it as
  `false` and log a warning (strict in CI, lenient at runtime) — but CI
  validates every authored condition by parsing it, so typos die in CI.
- `<' `<=` `>` `>=` compare numbers only; comparing non-numbers is an error.
- `==`/`!=` compare equal types only (number~number, string~string,
  bool~bool); cross-type comparison is an error, not `false`.
- `in` is list membership: `x in path` where the path resolves to a list.
- Bare non-boolean operands are errors — `soul.tib.trust` alone is not a
  condition; write `soul.tib.trust > 0`. No truthiness. No side effects.
- Conditions are total: they evaluate to exactly `true` or `false` or raise.
"""
from __future__ import annotations

import re
import sys

VERSION = 0

_TOKEN = re.compile(
    r"\s*(?:(?P<num>-?\d+(?:\.\d+)?)"
    r"|(?P<str>\"[^\"\n]*\")"
    r"|(?P<ident>[a-z_][a-z0-9_]*(?:\.[a-z_][a-z0-9_]*)*)"
    r"|(?P<op>==|!=|<=|>=|<|>|\(|\)))"
)

KEYWORDS = {"and", "or", "not", "in", "true", "false"}


class ParseError(ValueError):
    pass


class EvalError(ValueError):
    pass


def tokenize(text: str) -> list[tuple[str, object]]:
    tokens: list[tuple[str, object]] = []
    pos = 0
    while pos < len(text):
        m = _TOKEN.match(text, pos)
        if not m:
            if text[pos:].strip() == "":
                break
            raise ParseError(f"unexpected character {text[pos:].lstrip()[0]!r} at offset {pos}")
        pos = m.end()
        if m.group("num") is not None:
            tokens.append(("num", float(m.group("num"))))
        elif m.group("str") is not None:
            tokens.append(("str", m.group("str")[1:-1]))
        elif m.group("ident") is not None:
            word = m.group("ident")
            if word in ("and", "or", "not", "in"):
                tokens.append(("kw", word))
            elif word == "true":
                tokens.append(("bool", True))
            elif word == "false":
                tokens.append(("bool", False))
            else:
                tokens.append(("path", word))
        else:
            tokens.append(("op", m.group("op")))
    return tokens


class Parser:
    def __init__(self, tokens: list[tuple[str, object]]):
        self.tokens = tokens
        self.i = 0

    def peek(self) -> tuple[str, object] | None:
        return self.tokens[self.i] if self.i < len(self.tokens) else None

    def take(self) -> tuple[str, object]:
        tok = self.peek()
        if tok is None:
            raise ParseError("unexpected end of condition")
        self.i += 1
        return tok

    def parse(self) -> tuple:
        node = self.or_expr()
        if self.peek() is not None:
            raise ParseError(f"trailing input from token {self.i}: {self.peek()!r}")
        return node

    def or_expr(self) -> tuple:
        node = self.and_expr()
        while self.peek() == ("kw", "or"):
            self.take()
            node = ("or", node, self.and_expr())
        return node

    def and_expr(self) -> tuple:
        node = self.unary()
        while self.peek() == ("kw", "and"):
            self.take()
            node = ("and", node, self.unary())
        return node

    def unary(self) -> tuple:
        if self.peek() == ("kw", "not"):
            self.take()
            return ("not", self.unary())
        return self.comparison()

    def comparison(self) -> tuple:
        left = self.operand()
        tok = self.peek()
        if tok is not None and tok[0] == "op" and tok[1] in ("==", "!=", "<", "<=", ">", ">="):
            op = str(self.take()[1])
            return ("cmp", op, left, self.operand())
        if tok == ("kw", "in"):
            self.take()
            return ("in", left, self.operand())
        return left

    def operand(self) -> tuple:
        kind, value = self.take()
        if kind in ("num", "str", "bool"):
            return ("lit", value)
        if kind == "path":
            return ("get", value)
        if (kind, value) == ("op", "("):
            node = self.or_expr()
            if self.take() != ("op", ")"):
                raise ParseError("expected ')'")
            return node
        raise ParseError(f"unexpected token {value!r}")


def parse(text: str) -> tuple:
    """Parse a condition to an AST. Raises ParseError. Use for CI validation."""
    if not text.strip():
        raise ParseError("empty condition")
    return Parser(tokenize(text)).parse()


def _resolve(path: str, context: dict) -> object:
    node: object = context
    for part in path.split("."):
        if not isinstance(node, dict) or part not in node:
            raise EvalError(f"path '{path}' does not resolve (stopped at '{part}')")
        node = node[part]
    return node


def _value(node: tuple, context: dict) -> object:
    if node[0] == "lit":
        return node[1]
    if node[0] == "get":
        return _resolve(str(node[1]), context)
    # A parenthesized boolean sub-expression used as an operand.
    return _eval(node, context)


def _numeric(v: object, op: str) -> float:
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise EvalError(f"operator '{op}' needs numbers, got {type(v).__name__}")
    return float(v)


def _eval(node: tuple, context: dict) -> bool:
    head = node[0]
    if head == "or":
        return _eval(node[1], context) or _eval(node[2], context)
    if head == "and":
        return _eval(node[1], context) and _eval(node[2], context)
    if head == "not":
        return not _eval(node[1], context)
    if head == "cmp":
        op, left, right = str(node[1]), _value(node[2], context), _value(node[3], context)
        if op in ("<", "<=", ">", ">="):
            a, b = _numeric(left, op), _numeric(right, op)
            return {"<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
        # == / != : equal-type comparison only (numbers compare across int/float)
        both_num = all(
            isinstance(v, (int, float)) and not isinstance(v, bool) for v in (left, right)
        )
        if not both_num and type(left) is not type(right):
            raise EvalError(f"'{op}' compares {type(left).__name__} with {type(right).__name__}")
        return (left == right) if op == "==" else (left != right)
    if head == "in":
        needle, haystack = _value(node[1], context), _value(node[2], context)
        if not isinstance(haystack, list):
            raise EvalError("'in' needs a list on the right")
        return needle in haystack
    if head in ("lit", "get"):
        v = _value(node, context)
        if isinstance(v, bool):
            return v
        raise EvalError("bare non-boolean operand is not a condition (no truthiness)")
    raise EvalError(f"unknown node {head!r}")


def evaluate(text: str, context: dict) -> bool:
    """Parse and evaluate. Raises ParseError / EvalError. This function IS the
    semantics other implementations must match."""
    return _eval(parse(text), context)


# --- the contract battery ----------------------------------------------------

_CTX = {
    "faction": {"compact": {"trust": -60, "notoriety": 20}},
    "player": {"location": "verne", "credits": 1250, "flags": ["duskway_veteran"], "docked": True},
    "soul": {"tib": {"trust": 45, "mood": "wary"}},
    "universe": {"tick": 10450},
}

# (condition, expected) — expected is True/False, or the exception type name.
_CASES = [
    ("faction.compact.trust < -50", True),
    ("faction.compact.trust < -50 and player.location == \"verne\"", True),
    ("faction.compact.trust < -50 and player.location == \"aethon\"", False),
    ("soul.tib.trust >= 45", True),
    ("not soul.tib.trust >= 45", False),
    ("soul.tib.mood == \"wary\"", True),
    ("\"duskway_veteran\" in player.flags", True),
    ("\"compact_medal\" in player.flags", False),
    ("player.docked", True),
    ("player.docked == true", True),
    ("universe.tick > 10000 or soul.tib.trust > 90", True),
    ("universe.tick > 99999 or soul.tib.trust > 90", False),
    ("(universe.tick > 99999 or soul.tib.trust > 40) and player.docked", True),
    ("not (player.location == \"verne\")", False),
    ("player.credits >= 1250 and player.credits <= 1250", True),
    # precedence: and binds tighter than or
    ("false and false or true", True),
    ("true or false and false", True),
    # errors — these MUST raise, not silently coerce
    ("soul.tib.trust", "EvalError"),                     # bare number
    ("player.location < 5", "EvalError"),                # string vs number
    ("player.location == 5", "EvalError"),               # cross-type ==
    ("soul.nobody.trust > 0", "EvalError"),              # unresolvable path
    ("\"x\" in player.location", "EvalError"),           # `in` on non-list
    ("faction.compact.trust <", "ParseError"),
    ("== 5", "ParseError"),
    ("player.location == 'verne'", "ParseError"),        # single quotes invalid
    ("", "ParseError"),
    ("player.flags contains \"x\"", "ParseError"),
]


def self_test() -> int:
    failures: list[str] = []
    for condition, expected in _CASES:
        try:
            got: object = evaluate(condition, _CTX)
        except (ParseError, EvalError) as exc:
            got = type(exc).__name__
        if got != expected:
            failures.append(f"  {condition!r}: expected {expected!r}, got {got!r}")
    if failures:
        print(f"trigger DSL self-test FAILED ({len(failures)}/{len(_CASES)}):", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"trigger DSL self-test OK — {len(_CASES)} case(s), semantics v{VERSION}")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv[1:]:
        raise SystemExit(self_test())
    if len(sys.argv) > 1:
        try:
            print(evaluate(sys.argv[1], _CTX))
        except (ParseError, EvalError) as exc:
            print(f"{type(exc).__name__}: {exc}", file=sys.stderr)
            raise SystemExit(1)
    else:
        print(__doc__)
