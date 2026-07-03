#!/usr/bin/env python3
"""Soul Protocol integration test harness — the executable system spec.

Boots a real `pan serve` subprocess, connects over TCP loopback, and walks
the full Soul Protocol lifecycle using the 15 shared fixtures
(godot/framework/protocol/fixtures/) as the expected wire traffic — then
exercises every error path in the protocol's closed code set. Exits 0 on
full pass, 1 on any failure. No dependencies beyond the Python stdlib.

Companion narrative: docs/SYSTEM-INTEGRATION.md. Wire contract:
godot/framework/protocol/SOUL-PROTOCOL.md.

## How "matches the fixture" is defined

The fixtures are canonical *shapes*, not session transcripts, so comparison
is exact everywhere determinism allows and explicitly documented where the
session must diverge:

- Host->daemon messages are sent BYTE-FOR-BYTE as the fixture's `message`
  object (compact NDJSON re-serialization of the same JSON value).
- Daemon envelopes: `v` must be 0; `seq` must be strictly increasing across
  every line the daemon writes on one connection; `re` must equal the `seq`
  of the message being answered (absent on parse-reject replies, which
  answer lines that may not even have a seq).
- `welcome` (02) and `ack` (13) bodies: exact match.
- `decision` responding to 07_perceive_event: exact full-body match against
  08_decision_invoke_move (the rules mind is deterministic).
- `decision` responding to 05_perceive_utterance: matched against
  06_decision_express with the fixture's `invoke` intents removed. The
  harness runs a deterministic stub LLM that always answers with fixture
  06's exact Express line, so the Express text and Conclude outcome are
  exact; but Pan's v0 LLM provider does not emit tool-invokes (fixture 06
  documents the full wire shape including `npc.remember`, which is the
  M7 conversation-memory work). When Pan starts emitting invokes, this
  comparison fails loudly and must be updated deliberately.
- `error` bodies: `code` is contract and matched exactly; `message` is
  informational and only required to name the offending item (capability
  id, soul id, unknown type).
"""

import argparse
import json
import re
import shutil
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HARNESS_DIR = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_DIR.parent.parent
FIXTURES_DIR = REPO_ROOT / "godot" / "framework" / "protocol" / "fixtures"
FIXTURE_NAMES = [f"{i:02d}" for i in range(1, 16)]
SOCKET_TIMEOUT = 30.0  # generous: first LLM decision on a cold CI box

GREEN, RED, YELLOW, DIM, RESET = "\033[32m", "\033[31m", "\033[33m", "\033[2m", "\033[0m"


def plain():
    """Disable colors when stdout is not a tty (CI logs)."""
    global GREEN, RED, YELLOW, DIM, RESET
    GREEN = RED = YELLOW = DIM = RESET = ""


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

def load_fixtures(fixtures_dir: Path) -> dict:
    """Load the 15 fixtures keyed by their two-digit prefix."""
    out = {}
    for path in sorted(fixtures_dir.glob("*.json")):
        with open(path, encoding="utf-8") as f:
            out[path.name[:2]] = json.load(f)
    missing = [n for n in FIXTURE_NAMES if n not in out]
    if missing:
        raise SystemExit(f"fixtures missing from {fixtures_dir}: {missing}")
    return out


def express_line(fixture_06: dict) -> str:
    """The canonical Express line the stub LLM must reproduce."""
    for intent in fixture_06["message"]["body"]["decision"]["intents"]:
        if intent.get("intent") == "express":
            return intent["body"]
    raise SystemExit("fixture 06 has no express intent")


def strip_invokes(decision_body: dict) -> dict:
    """Fixture 06's expectation minus `invoke` intents (see module docstring)."""
    body = json.loads(json.dumps(decision_body))
    body["decision"]["intents"] = [
        i for i in body["decision"]["intents"] if i.get("intent") != "invoke"
    ]
    return body


# ---------------------------------------------------------------------------
# The stub LLM (deterministic OpenAI-compatible endpoint)
# ---------------------------------------------------------------------------

class StubLlm(BaseHTTPRequestHandler):
    """Always answers a chat completion with fixture 06's exact Express line.

    Pan probes GET /api/version to detect Ollama's native API; we 404 it so
    Pan speaks OpenAI-compatible /v1/chat/completions. HTTP/1.0 (the default)
    closes after each response, which is exactly what Pan's tiny client reads.
    """

    line = "REPLACED AT STARTUP"

    def do_GET(self):
        if self.path == "/v1/models":
            self._json({"data": [{"id": "fixture-echo"}]})
        else:
            self.send_error(404)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        self.rfile.read(length)  # drain; the answer never depends on the prompt
        self._json({"choices": [{"message": {"role": "assistant", "content": self.line}}]})

    def _json(self, obj):
        payload = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):
        pass  # keep harness output to the per-step lines


def start_stub_llm(canonical_line: str):
    StubLlm.line = canonical_line
    server = ThreadingHTTPServer(("127.0.0.1", 0), StubLlm)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


# ---------------------------------------------------------------------------
# The pan daemon subprocess
# ---------------------------------------------------------------------------

def find_pan_bin(cli_arg: str | None) -> Path:
    """Locate (or build) the pan binary: --pan-bin, $PAN_BIN, sibling checkout."""
    import os
    candidates = []
    if cli_arg:
        candidates.append(Path(cli_arg))
    if os.environ.get("PAN_BIN"):
        candidates.append(Path(os.environ["PAN_BIN"]))
    pan_src = REPO_ROOT.parent / "pan"
    candidates += [
        pan_src / "target" / "release" / "pan",
        pan_src / "target" / "debug" / "pan",
    ]
    for c in candidates:
        if c.is_file():
            return c
    if (pan_src / "Cargo.toml").is_file() and shutil.which("cargo"):
        print(f"{DIM}pan binary not found — building from {pan_src}…{RESET}")
        subprocess.run(["cargo", "build", "--bin", "pan"], cwd=pan_src, check=True)
        return pan_src / "target" / "debug" / "pan"
    raise SystemExit(
        "pan binary not found. Set PAN_BIN or --pan-bin, or check out\n"
        f"github.com/chezgoulet/pan at {pan_src} and `cargo build --bin pan`."
    )


class PanDaemon:
    """Owns the `pan serve` subprocess: spawn on an OS-assigned port, parse
    the bound port from stderr, keep draining stderr for post-mortems, kill
    on exit."""

    def __init__(self, pan_bin: Path, llm_port: int):
        import os
        env = dict(os.environ)
        env["PAN_LLM_BASE"] = f"http://127.0.0.1:{llm_port}"
        env["PAN_LLM_MODEL"] = "fixture-echo"
        env.pop("REACHLOCK_PAN_PORT", None)  # --port 0 must win
        self.proc = subprocess.Popen(
            [str(pan_bin), "serve", "--port", "0"],
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
            env=env, text=True,
        )
        self.stderr_lines: list[str] = []
        self.port = self._await_bound_port()
        threading.Thread(target=self._drain, daemon=True).start()

    def _await_bound_port(self) -> int:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            line = self.proc.stderr.readline()
            if not line:
                break
            self.stderr_lines.append(line.rstrip())
            m = re.search(r"pan serve: bound 127\.0\.0\.1:(\d+)", line)
            if m:
                return int(m.group(1))
        self.kill()
        raise SystemExit(
            "pan serve never reported a bound port; stderr so far:\n  "
            + "\n  ".join(self.stderr_lines)
        )

    def _drain(self):
        for line in self.proc.stderr:
            self.stderr_lines.append(line.rstrip())

    def kill(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()

    def alive(self) -> bool:
        return self.proc.poll() is None


# ---------------------------------------------------------------------------
# One NDJSON connection
# ---------------------------------------------------------------------------

class Connection:
    """One host connection. Tracks the daemon's seq to assert monotonicity."""

    def __init__(self, port: int):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=SOCKET_TIMEOUT)
        self.reader = self.sock.makefile("r", encoding="utf-8", newline="\n")
        self.last_daemon_seq = -1

    def send_raw(self, line: str):
        self.sock.sendall(line.encode("utf-8") + b"\n")

    def send(self, message: dict):
        self.send_raw(json.dumps(message, ensure_ascii=False, separators=(",", ":")))

    def recv(self) -> dict | None:
        """One daemon line, parsed; None on clean EOF."""
        line = self.reader.readline()
        if not line:
            return None
        msg = json.loads(line)
        if msg["seq"] <= self.last_daemon_seq:
            raise CheckFailed(
                f"daemon seq not monotonic: {msg['seq']} after {self.last_daemon_seq}")
        self.last_daemon_seq = msg["seq"]
        return msg

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


class CheckFailed(Exception):
    pass


# ---------------------------------------------------------------------------
# Assertions
# ---------------------------------------------------------------------------

def check(cond: bool, what: str):
    if not cond:
        raise CheckFailed(what)


def expect_response(conn: Connection, sent: dict, ty: str, body: dict | None = None,
                    expect_re: bool = True) -> dict:
    """Read one daemon line; assert envelope discipline and (optionally) an
    exact body. Returns the parsed message for further checks."""
    msg = conn.recv()
    check(msg is not None, "connection closed; expected a response")
    check(msg.get("v") == 0, f"envelope v: expected 0, got {msg.get('v')}")
    check(msg.get("type") == ty, f"type: expected {ty!r}, got {msg.get('type')!r}"
                                 f" (body: {json.dumps(msg.get('body'))[:200]})")
    if expect_re:
        check(msg.get("re") == sent["seq"],
              f"re: expected {sent['seq']} (the request seq), got {msg.get('re')}")
    else:
        check("re" not in msg, f"re: expected absent on unsolicited reply, got {msg.get('re')}")
    if body is not None:
        check(msg["body"] == body,
              "body mismatch:\n"
              f"    expected: {json.dumps(body, ensure_ascii=False)}\n"
              f"    actual:   {json.dumps(msg['body'], ensure_ascii=False)}")
    return msg


def expect_error(conn: Connection, sent: dict | None, code: str, names: str = "",
                 expect_re: bool = True) -> dict:
    """Expect an `error` reply: code exact, message must name the offender."""
    msg = conn.recv()
    check(msg is not None, "connection closed; expected an error reply")
    check(msg.get("type") == "error",
          f"type: expected 'error', got {msg.get('type')!r}"
          f" (body: {json.dumps(msg.get('body'))[:200]})")
    if sent is not None and expect_re:
        check(msg.get("re") == sent["seq"],
              f"re: expected {sent['seq']}, got {msg.get('re')}")
    got_code = msg["body"].get("code")
    check(got_code == code, f"error code: expected {code!r}, got {got_code!r}"
                            f" (message: {msg['body'].get('message')!r})")
    if names:
        check(names in msg["body"].get("message", ""),
              f"error message should name {names!r}: {msg['body'].get('message')!r}")
    return msg


def expect_closed(conn: Connection, what: str):
    """The daemon must close the connection: the next read returns EOF."""
    line = conn.reader.readline()
    check(line == "", f"{what}: expected clean close, got line: {line[:200]!r}")


# ---------------------------------------------------------------------------
# The walk
# ---------------------------------------------------------------------------

class Runner:
    def __init__(self):
        self.n = 0
        self.failures = 0

    def step(self, name: str, fn):
        self.n += 1
        try:
            note = fn()
        except CheckFailed as e:
            self.failures += 1
            print(f"  {RED}FAIL{RESET} [{self.n:02d}] {name}\n       {e}")
            raise
        except (OSError, json.JSONDecodeError) as e:
            self.failures += 1
            print(f"  {RED}FAIL{RESET} [{self.n:02d}] {name}\n       {type(e).__name__}: {e}")
            raise CheckFailed(str(e)) from e
        suffix = f"  {DIM}{note}{RESET}" if isinstance(note, str) else ""
        print(f"  {GREEN}ok{RESET}   [{self.n:02d}] {name}{suffix}")


# The rules soul the harness instantiates to make decisions deterministic for
# the event / signal / unknown-capability steps. Same soul_id as fixture 04:
# re-instantiation replaces the mind, which is protocol-legal (the daemon
# owns runtime mind-state; the host owns the birth-state it sends).
RULES_SOUL = {
    "soul_id": "example_pilot",
    "mind": "rules",
    "soul": {
        "id": "example_pilot",
        "name": "Example Pilot",
        "rules": [
            {"when_event_topic": "combat.crew_saved",
             "then_invoke": {"capability": "npc.move_to", "args": {"room": "cockpit"}}},
            {"when_signal_over": {"name": "hull_integrity", "threshold": 0.3},
             "then_invoke": {"capability": "npc.move_to", "args": {"room": "engineering"}}},
            {"when_event_topic": "test.unregistered_capability",
             "then_invoke": {"capability": "npc.fly_ship", "args": {}}},
        ],
    },
}

ACK = {}  # fixture 13's body — the empty object


def lifecycle_connection(r: Runner, port: int, fx: dict, line: str):
    """Connection 1: the full happy-path lifecycle, fixtures 01→14."""
    conn = Connection(port)
    try:
        def send_fixture(num: str) -> dict:
            msg = fx[num]["message"]
            conn.send(msg)
            return msg

        r.step("01_hello → welcome matches 02_welcome (exact body)", lambda: (
            expect_response(conn, send_fixture("01"), "welcome",
                            fx["02"]["message"]["body"]) and None))

        r.step("03_register_capabilities → 13_ack", lambda: (
            expect_response(conn, send_fixture("03"), "ack", ACK) and None))

        r.step("04_instantiate_soul (mind: llm) → 13_ack", lambda: (
            expect_response(conn, send_fixture("04"), "ack", ACK) and None))

        def perceive_utterance():
            expected = strip_invokes(fx["06"]["message"]["body"])
            expect_response(conn, send_fixture("05"), "decision", expected)
            return ("Express line is fixture-exact via the stub LLM; fixture 06's "
                    "npc.remember invoke is not emitted by Pan v0 (see README)")
        r.step("05_perceive_utterance → decision matches 06_decision_express", perceive_utterance)

        def perceive_supersession():
            expected = {
                "soul_id": "example_pilot", "goal_id": "conv_00042", "goal_revision": 2,
                "decision": {"intents": [
                    {"intent": "express", "body": line},
                    {"intent": "conclude", "outcome": "achieved"},
                ]},
            }
            expect_response(conn, send_fixture("10"), "decision", expected)
            return "decision echoes goal_revision 2 — the host can drop stale revision 1"
        r.step("10_perceive_superseding_revision → decision carries revision 2",
               perceive_supersession)

        def reinstantiate():
            msg = {"v": 0, "seq": 100, "type": "instantiate_soul", "body": RULES_SOUL}
            conn.send(msg)
            expect_response(conn, msg, "ack", ACK)
        r.step("re-instantiate example_pilot as a rules mind → 13_ack", reinstantiate)

        r.step("07_perceive_event → decision matches 08_decision_invoke_move (exact body)",
               lambda: expect_response(conn, send_fixture("07"), "decision",
                                       fx["08"]["message"]["body"]) and None)

        def perceive_tick():
            expected = {
                "soul_id": "example_pilot", "goal_id": "idle_example_pilot",
                "goal_revision": 9,
                "decision": {"intents": [{"intent": "conclude", "outcome": "continue"}]},
            }
            expect_response(conn, send_fixture("11"), "decision", expected)
            return "no rule fires on an idle tick → conclude: continue"
        r.step("11_perceive_tick → decision concludes continue", perceive_tick)

        def perceive_signal():
            expected = {
                "soul_id": "example_pilot", "goal_id": "sig_hull_low", "goal_revision": 1,
                "decision": {"intents": [
                    {"intent": "invoke", "capability": "npc.move_to",
                     "args": {"room": "engineering"}},
                    {"intent": "conclude", "outcome": "achieved"},
                ]},
            }
            expect_response(conn, send_fixture("12"), "decision", expected)
            return "hull_integrity 0.31 crossed the 0.3 rule threshold"
        r.step("12_perceive_signal → decision invokes npc.move_to engineering", perceive_signal)

        def unknown_capability():
            msg = {"v": 0, "seq": 101, "type": "perceive", "body": {
                "soul_id": "example_pilot",
                "goal": {"id": "g_unknown_cap", "revision": 1,
                         "objective": "Fire a rule invoking an unregistered capability.",
                         "trigger": {"kind": "event", "topic": "test.unregistered_capability",
                                     "payload": {}}},
                "context": {"fragments": []},
            }}
            conn.send(msg)
            expect_error(conn, msg, fx["09"]["message"]["body"]["code"], names="npc.fly_ship")
            return "conformance case 09: validate stage rejects the invoke"
        r.step("perceive firing an unregistered capability → error: unknown_capability",
               unknown_capability)

        r.step("15_release_soul → 13_ack", lambda: (
            expect_response(conn, send_fixture("15"), "ack", ACK) and None))

        r.step("perceive after release → error: unknown_soul", lambda: (
            expect_error(conn, send_fixture("05"), "unknown_soul",
                         names="example_pilot") and None))

        def shutdown():
            expect_response(conn, send_fixture("14"), "ack", ACK)
            expect_closed(conn, "post-shutdown")
        r.step("14_shutdown → 13_ack, then the connection closes cleanly", shutdown)
    finally:
        conn.close()


def error_path_connection(r: Runner, port: int, fx: dict):
    """Connection 2: rejected lines must not kill the session."""
    conn = Connection(port)
    try:
        def handshake():
            msg = fx["01"]["message"]
            conn.send(msg)
            expect_response(conn, msg, "welcome", fx["02"]["message"]["body"])
        r.step("fresh connection: 01_hello → 02_welcome", handshake)

        def bad_frame():
            conn.send_raw('{"v":0,"seq":1,')  # truncated JSON
            expect_error(conn, None, "bad_frame", expect_re=False)
        r.step("malformed JSON → error: bad_frame", bad_frame)

        def unknown_type():
            conn.send({"v": 0, "seq": 2, "type": "frobnicate", "body": {}})
            expect_error(conn, None, "unknown_type", names="frobnicate", expect_re=False)
        r.step("unknown message type → error: unknown_type (connection stayed open)",
               unknown_type)

        r.step("perceive for a never-instantiated soul → error: unknown_soul", lambda: (
            conn.send(fx["05"]["message"]),
            expect_error(conn, fx["05"]["message"], "unknown_soul", names="example_pilot"),
        ) and None)

        def shutdown():
            conn.send(fx["14"]["message"])
            expect_response(conn, fx["14"]["message"], "ack", ACK)
            expect_closed(conn, "post-shutdown")
        r.step("connection survived every rejected line; 14_shutdown closes it", shutdown)
    finally:
        conn.close()


def version_mismatch_connection(r: Runner, port: int, fx: dict):
    """Connection 3: a wrong protocol_version at hello ends the connection."""
    conn = Connection(port)
    try:
        def wrong_version():
            msg = json.loads(json.dumps(fx["01"]["message"]))
            msg["body"]["protocol_version"] = 99
            conn.send(msg)
            expect_error(conn, msg, "version_unsupported", expect_re=False)
            expect_closed(conn, "post-version-mismatch")
        r.step("hello with protocol_version 99 → error: version_unsupported, then close",
               wrong_version)
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def cross_check_pan_fixtures(r: Runner, fixtures_dir: Path):
    """When a pan checkout is present, its fixture copies must be
    byte-identical to ours — that is the cross-repo contract."""
    pan_fixtures = REPO_ROOT.parent / "pan" / "pan-daemon" / "tests" / "fixtures"
    if not pan_fixtures.is_dir():
        print(f"  {YELLOW}skip{RESET} fixture byte-identity (no pan checkout at ../pan)")
        return

    def compare():
        drift = []
        for name in sorted(p.name for p in fixtures_dir.glob("*.json")):
            ours, theirs = fixtures_dir / name, pan_fixtures / name
            if not theirs.is_file():
                drift.append(f"{name}: missing on the pan side")
            elif ours.read_bytes() != theirs.read_bytes():
                drift.append(f"{name}: bytes differ")
        check(not drift, "fixture drift between repos:\n       " + "\n       ".join(drift))
        return f"{len(list(fixtures_dir.glob('*.json')))} fixtures byte-identical with ../pan"
    r.step("shared fixtures are byte-identical across repos", compare)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pan-bin", help="path to the pan binary (default: $PAN_BIN, "
                                          "then ../pan/target/{release,debug}/pan)")
    parser.add_argument("--fixtures", type=Path, default=FIXTURES_DIR,
                        help="fixtures directory (default: the framework copy)")
    args = parser.parse_args()
    if not sys.stdout.isatty():
        plain()

    fx = load_fixtures(args.fixtures)
    line = express_line(fx["06"])
    pan_bin = find_pan_bin(args.pan_bin)

    print(f"soul-protocol-harness: pan={pan_bin}")
    print(f"                       fixtures={args.fixtures}")

    stub, llm_port = start_stub_llm(line)
    daemon = PanDaemon(pan_bin, llm_port)
    print(f"                       pan serve pid={daemon.proc.pid} "
          f"port={daemon.port} (stub llm on 127.0.0.1:{llm_port})\n")

    r = Runner()
    try:
        cross_check_pan_fixtures(r, args.fixtures)
        print("— connection 1: full lifecycle —")
        lifecycle_connection(r, daemon.port, fx, line)
        print("— connection 2: error paths, connection survives —")
        error_path_connection(r, daemon.port, fx)
        print("— connection 3: version mismatch closes —")
        version_mismatch_connection(r, daemon.port, fx)

        r.step("pan daemon survived all three connections",
               lambda: check(daemon.alive(), "pan serve exited early") or
               f"pid {daemon.proc.pid} still serving")
    except CheckFailed:
        pass  # already reported by the step runner
    finally:
        daemon.kill()
        stub.shutdown()

    print()
    if r.failures:
        print(f"{RED}FAILED{RESET}: {r.failures} of {r.n} steps "
              f"(pan stderr tail below)")
        for stderr_line in daemon.stderr_lines[-15:]:
            print(f"  {DIM}pan: {stderr_line}{RESET}")
        return 1
    print(f"{GREEN}PASS{RESET}: all {r.n} steps — full Soul Protocol lifecycle "
          f"+ every error path, against a real pan serve subprocess")
    return 0


if __name__ == "__main__":
    sys.exit(main())
