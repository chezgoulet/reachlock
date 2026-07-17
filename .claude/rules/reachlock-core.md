---
paths:
  - "reachlock-core/**"
---

# reachlock-core rules

- Zero rendering/IO deps. Pure functions only; any new dependency must compile
  to wasm32 and be justified in the PR.
- No floats in gameplay values — `util::rng::Fixed` (1/1024) or plain integers.
- New/changed generator ⇒ extend `src/determinism.rs` and recapture goldens
  deliberately; call out manifest changes in the commit message.
- Wire shapes (`network/messages.rs`, contract JSON, content schemas) are
  pinned by serialization tests. Changing one is a protocol revision: update
  the test AND note it.
- Seeds are ≤ 2^53 (JSON float survival); `Seed::new` masks — keep it that way.
