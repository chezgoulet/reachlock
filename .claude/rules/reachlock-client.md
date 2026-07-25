---
paths:
  - "reachlock-client/**"
---

# reachlock-client rules

- Floats are fine here (bridge/render layer) but must never feed back into
  contract-visible gameplay state — that stays fixed-point in core.
- bevy 0.18 gotchas: mesh types import from `bevy::mesh::`; `Timer::finished`
  is now `is_finished`; `RapierPhysicsPlugin::<()>` (unit generic).
- Bevy query filters trip clippy `type_complexity`; `#[allow]` on the system
  fn is the accepted pattern.
- Every LLM call surfaces a visible deliberation state — no silent inference.
- Offline is first-class: every feature must work with no server.
- New systems register in `main.rs` with an explicit run condition
  (`in_spaceflight` / `in_any_interior` / `space_live` / `in_state`).
