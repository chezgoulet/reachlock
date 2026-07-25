# Adding a New Generator

1. Create `core/src/generator/my_gen.rs` — pure fn, no IO, no floats in gameplay values
2. Add `pub mod my_gen;` to `core/src/generator/mod.rs`
3. Add golden entry to `core/src/determinism.rs`, bump manifest version
4. Commit message MUST mention "manifest vN→vM"
