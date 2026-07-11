# M1 — "A rules NPC decides" ✅ (2026-07-02)

The first Sprint 01 integration milestone, verified end-to-end on a live run.

## What ran

```
pan serve --port 40707                       # pan-daemon (Rust), rules mind
REACHLOCK_FORCE_MODE=landed godot --headless # the game, no rendering
```

Output:

```
mods: loaded reachlock (31 entities)
souls: connected to pan-serve/0.1.0 (protocol 0)
souls: decision for tib (tib_g1): 1 intent(s)
```

## What that one log line proves

1. **Mod loader** → 31 entities of authored content through the framework path.
2. **Soul Protocol v0** → hello/welcome handshake, capability registration,
   `instantiate_soul` with Tib's birth-state (soul file v1), all over NDJSON/TCP
   exactly as the frozen contract and its 15 golden fixtures specify.
3. **NpcSpawner** → Tib spawned because Sorrow Station's content says
   `npcs_present: ["tib"]` — the engine named no one.
4. **Perceive → decide → wire back** → the landed scene broadcast
   `location.player_arrived`; Pan's rules provider decided through the
   validate/govern pipeline; the decision returned and dispatched to Tib's
   SoulInstance signals.
5. **Resilience** → an earlier run exposed (and we fixed) a daemon bug: a host
   crash killed the accept loop. The mind daemon now outlives its hosts.

## Not yet in this demo

- The mind is the deterministic rules provider — M2 ("Tib speaks") swaps in
  the LLM provider behind the same contract.
- Memory is authored seeds only — M3 ("Tib remembers") connects Ragamuffin.
