# M7 — "It feels like talking to someone" ✅ (2026-07-03)

All three clauses verified against the LIVE stack — real pan, real Ollama
(`gemma4:e4b` on CPU), real Ragamuffin (embedded store, auth on) — with a
scripted host walking the exact message flow the game uses. Transcripts
below are verbatim from the run. The *feel* still deserves a human
playtest; everything mechanical behind the feel is proven.

## 1. No dead-air pauses

Pan's decide used to run inline in the connection read loop — one thinking
soul stalled every other soul (the known M2.1 debt). Perceives now fan out
to worker threads: begin under the session lock (cheap), **decide outside
every lock**, finish back under the lock at the enact boundary.

Live measurement (one LLM soul mid-completion, one rules soul poked):

```
rules decision arrived at 0.04s while the LLM was still thinking
llm decision arrived at 4.4s: "Worst? Honey, 'worst' implies a scale. …"
```

Supersession moved with it: a newer goal revision discards in-flight work
at the enact boundary with `error: superseded` — the walk-away case never
produces an orphaned line. The integration harness pins both behaviors
(steps "async perceive: a rules decision overtakes a slow llm decision"
and "supersession: a newer revision discards in-flight work" — 23/23),
and the daemon's wire `seq` stays monotonic because seq allocation and the
write happen under one lock scope (the harness caught the ordering race
during development).

Host side, unchanged but load-bearing: DialogueRunner falls back the
moment a mind concludes `abandoned` (fail-fast, no timeout wait), with the
15 s ceiling only guarding a hung daemon.

## 2. Multi-turn conversations cohere

The protocol always had a `history` context channel; the host never sent
it — every generated line saw only the current utterance. SoulInstance now
carries the running transcript (last 8 turns, via DialogueRunner) on the
history channel, ordered persona → memory → history → world per the
profile.

Live, first run:

```
[ 5.2s] player: Rough landing back there. You holding up?
        vex:    Just rattled. The hull can take a few more hits, though. You alright?
[ 4.7s] player: Hang on - what did I just ask you about?
        vex:    You asked if I was alright. Now, are you done distracting me?
```

Honest note: a second run answered the same probe wrong ("You didn't ask
anything") — the channel reliably delivers the transcript; a 4B model
sometimes uses it and sometimes riffs. Model quality is content/config
tuning, not contract work.

## 3. A memory formed from unscripted conversation is recalled next session

This was M3's open gap, and it turned out to be TWO live bugs, both fixed
with regression tests:

- **Ragamuffin's fact extraction spoke the OpenAI-compatible endpoint**,
  where Ollama ignores `think: false` — reasoning models burned the whole
  completion budget "thinking" and returned empty content (measured live:
  519 chars of reasoning, 0 content). `RAGAMUFFIN_LLM_PROVIDER=ollama` now
  selects Ollama's NATIVE `/api/chat` with think off (answer in 9 s). The
  dev stack sets it by default.
- **Body-vault ingest wrote facts to the wrong store.** The documented
  REACHLOCK path (`POST /v1/ingest/conversation` with `vault` in the body)
  never put the vault into the request context, so per-vault resolution
  fell through to the server-wide facts store — while the vault-scoped
  hybrid read the per-vault one. The soul could never see its own
  conversation memories. Fixed by injecting the body vault into the
  context (plus nil-guards on the per-vault resolvers).

Live, end to end — unscripted two-turn conversation → ingest → distilled
facts → release + re-instantiate the soul ("next session") → hybrid recall
rides the memory channel:

```
ingest: status=ok fact_count=1
recall fragments: "The landing was rough", "The assistant was bruised",
                  "The salvage crate took more punishment …", …
[13.9s] player: Been a few days. You remember what we talked about after that landing?
        vex:    You wanna know what we talked about? You got a memory like a busted comm unit, Captain.
```

The facts of the exchange, not the transcript, are what persists — exactly
the memory-interface contract.

## How a human replays this

```sh
./scripts/dev_stack.sh test           # isolated ragamuffin, LLM extraction on
python3 <scratch>/m7_live.py          # or: play; talk to Tib twice; quit; relaunch
```

In-game: the same paths run under `make godot` with the full dev stack —
talk to Tib through a generated dialogue node, end the conversation
(ingest fires), relaunch, and ask him about it.

## Known gaps (tracked, non-blocking)

- Small-model coherence varies run to run (see §2). Latency ~4–7 s per
  line on CPU is the dead-air *between* lines; the async work removes the
  cross-soul stall, but per-line latency is model/hardware.
- Extraction sometimes returns granular facts (fact_count=6), sometimes a
  single conversation-summary fact — both recall fine; shape tuning is a
  Ragamuffin roadmap item.
- The unused `_tick_of_prices`-style UI polish (typing indicator while a
  soul thinks) wants a human eye; not attempted headless.
