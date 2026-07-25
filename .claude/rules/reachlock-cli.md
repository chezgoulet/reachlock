---
paths:
  - "reachlock-cli/**"
---

# reachlock-cli rules

- Subcommands: `gen`, `determinism`, `content`. New tooling extends these
  rather than adding parallel binaries.
- The determinism runner is a CI gate — output must stay machine-comparable
  (stable ordering, no timestamps, no platform-dependent formatting).
- Content validation errors must name the file, the schema, and the failing
  field — modders see these messages too (spec §23: our content is a mod).
