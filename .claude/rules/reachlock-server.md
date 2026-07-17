---
paths:
  - "reachlock-server/**"
---

# reachlock-server rules

- Server is a ledger, not a simulator: it records seeds, claims, and signed
  evaluations; clients run the simulation.
- Race conditions are schema problems — Postgres UNIQUE constraints are the
  atomic arbiter, never application-level locks.
- The universe tick must not block message routing (`tokio::sync::mpsc`);
  a long tick skips the next one instead of queueing.
- Tick logic shared with offline clients lives in `reachlock-core::sim` —
  parity ("same seed + same event log = same universe") is test-enforced.
