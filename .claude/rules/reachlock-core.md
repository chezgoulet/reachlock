---
paths:
  - "reachlock-core/**"
---

# reachlock-core rules

- Zero rendering/IO deps. Pure functions only; any new dependency must be
  justified in the PR. `make check-purity` fails if core's dependency tree
  pulls in bevy/wgpu/winit/tokio/reqwest/hyper/axum/sqlx/redis/rfd/eframe/egui.
- No floats in gameplay values — `util::rng::Fixed` (1/1024) or plain integers.
- New/changed generator ⇒ extend `src/determinism.rs` and recapture goldens
  deliberately; call out manifest changes in the commit message.
- Wire shapes (`network/messages.rs`, contract JSON, content schemas) are
  pinned by serialization tests. Changing one is a protocol revision: update
  the test AND note it.
- Seeds are ≤ 2^53 (JSON float survival); `Seed::new` masks — keep it that way.
