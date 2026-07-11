# M5 — "Trust the foundation" ✅ (2026-07-03)

Validation debt paid. No new gameplay — this milestone is the license to
build M6+ on the Sprint 01 internals. Everything below was run headless on
the dev machine; each step names the command and what it must print.

## 1. The DSL conformance bridge runs (and already earned its keep)

What a tester does:

```sh
make dsl-bridge
```

What they observe:

```
DSL conformance bridge OK — 27 case(s), GDScript evaluator matches the
Python reference (semantics v0)
```

The first-ever run of this bridge failed **16 of 27** cases: the in-engine
tokenizer rejected every condition containing a space, so every authored
storyline condition had been silently evaluating `false` at runtime
(lenient mode). One-line guard fix in `trigger_dsl.gd`; the bridge now runs
in CI's godot job on every push. Success looks like: change the GDScript
evaluator's semantics in any way and CI goes red.

## 2. Deep review filed, correctness issues fixed

Findings and dispositions: [docs/reviews/2026-07-03-sprint02-deep-review.md](../reviews/2026-07-03-sprint02-deep-review.md).
Five correctness fixes landed (3× pan wire contract, 1× GDScript DSL
tokenizer, 1× ragamuffin vault isolation), each with a regression test.
Verify with the standing gates:

```sh
make check                                   # reachlock
(cd ../pan && cargo test)                    # pan — 84 tests
(cd ../ragamuffin-raga && go test ./internal/... -short)
python3 tests/soul-protocol-harness/run_harness.py   # 21/21 steps
```

## 3. Vault hygiene — test contamination is no longer possible

What a tester does:

```sh
./scripts/dev_stack.sh test
```

What they observe: a second Ragamuffin on `127.0.0.1:8001` with its own
database under `~/.local/share/reachlock-test/`, and three exports printed —
`REACHLOCK_MEMORY_URL` (port 8001), `REACHLOCK_MEMORY_KEY`, and
`REACHLOCK_VAULT_PREFIX=test-`. With those exported, every vault the game
touches is named `test-soul-*` on a database that has no play data. The
play stack's vaults cannot be reached by name, port, or file path.

Backstop below the convention: the embedded store's table naming is now
injective (it previously merged `soul-x`/`soul_x`/`soul:x` into one table),
with a migration that preserves existing play vaults. Verified:

```sh
(cd ../ragamuffin-raga && go test ./internal/embeddedstore/ -run 'Injective|Isolated|Migrate' -v)
```

## 4. Dev stack binds loopback with auth on by default

What a tester does: `./scripts/dev_stack.sh` (or `test`), then:

```sh
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8001/v1/briefing   # 401
curl -s -o /dev/null -w "%{http_code}\n" -H "Authorization: Bearer $(cat ~/.local/share/reachlock/ragamuffin.key)" \
  http://127.0.0.1:8001/v1/briefing                                          # 200
ss -tlnp | grep 8001                                                         # 127.0.0.1:8001, not 0.0.0.0
```

Observed in this run: `401` / `401` (wrong key) / `200`, bind `127.0.0.1:8001`.
The key is generated once into `~/.local/share/reachlock/ragamuffin.key`
(0600); MemoryStore reads it automatically (env `REACHLOCK_MEMORY_KEY`
overrides), so `make godot` still needs zero setup. Pan already binds
loopback by construction (`serve_loopback` hardcodes 127.0.0.1).

## Known gaps (tracked, non-blocking)

- The play stack was not restarted during this run (nothing was running);
  the vault-isolation migration applies on its next start. First start
  after this change will log nothing visible — the rename is silent.
- The MMO HTTP driver's shared-state race (review finding #9) is filed for
  the MMO sprint, not fixed here — no player-facing path uses it.
