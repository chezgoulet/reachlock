# S114 — CI Integrity: Toolchain Pin & --all-targets (H14, H15)

**Wave: Hotfix · Depends on:** None (CI config only)

## Outcome

The `ci.yml` determinism workflow uses `@stable` rust toolchain (which ignores `rust-toolchain.toml`) and is missing `--all-targets` on clippy. Both are aligned with `check.yml` which does these correctly. The three jobs in `ci.yml` (x86_64 test, aarch64 determinism, i686 determinism) are pinned to `1.96.0` like the check workflow.

## Context

The repository has two CI workflows:

| File | Purpose | Toolchain | clippy targets |
|------|---------|-----------|----------------|
| `check.yml` | PR/push to testing | ✅ `@master` + `toolchain: "1.96.0"` | ✅ `--workspace --all-targets` |
| `ci.yml` | Push to main/testing | ❌ `@stable` (drifts!) | ❌ `--workspace` only |

`@stable` uses whatever Rust version is current at runtime — this changes over time. The `rust-toolchain.toml` file pins the project to `1.96.0` but `@stable` ignores this. Different determinism jobs could end up running different compiler versions, potentially producing different binary hashes.

### Key files

| File | Role |
|------|------|
| `.github/workflows/ci.yml` | Fix lines 20, 30, 46, 60 |
| `.github/workflows/check.yml` | Reference — correct pattern (do not modify) |
| `rust-toolchain.toml` | Pins to `1.96.0` — no change needed |

## Fix 1 — H14: Pin toolchain in all ci.yml jobs

Three jobs in `ci.yml` use `dtolnay/rust-toolchain@stable`:

- Line 20: `test` job (x86_64 build/lint/test)
- Line 46: `determinism-arm` job (aarch64)
- Line 60: `determinism-i686` job (i686)

**For each of these three `uses:` lines, change:**
```yaml
      - uses: dtolnay/rust-toolchain@stable
```
**to:**
```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.96.0"
```

The `test` job also needs `components: rustfmt, clippy` — keep this:
```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.96.0"
          components: rustfmt, clippy
```

The determinism jobs don't need clippy/fmt:
```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.96.0"
```

## Fix 2 — H15: Add --all-targets to ci.yml clippy

**File:** `.github/workflows/ci.yml` line 30

**Before:**
```yaml
      - run: cargo clippy --workspace -- -D warnings
```

**After:**
```yaml
      - run: cargo clippy --workspace --all-targets -- -D warnings
```

The `test` job (x86_64) is the only job that runs clippy in `ci.yml`. The determinism jobs don't run clippy (they only emit manifests).

## Acceptance gates

```bash
# Verify toolchain pin matches
grep -n "rust-toolchain" .github/workflows/ci.yml
# All matches should show toolchain: "1.96.0"

grep -n "all-targets" .github/workflows/ci.yml
# Line 30 should include --all-targets

# Verify check.yml still has the correct pattern (no regression)
grep -n "toolchain" .github/workflows/check.yml

# CI passes on next push
git push
```

## Non-goals

- Changing `check.yml` (it's already correct)
- Upgrading the pinned toolchain version
- Adding new CI jobs
- Changing format/lint commands

## Gotchas

- **`@master` is used by `dtolnay/rust-toolchain` for non-stable channels.** When using `@master`, you MUST specify `toolchain:` explicitly. Available channels: `stable`, `beta`, `nightly`, or a specific version like `"1.96.0"`.
- **The determinism jobs on ARM build a Rust binary.** They need the `toolchain` field even without `components` — otherwise they fall back to the default toolchain.
- **i686 cross-compilation.** The i686 job also specifies `targets: i686-unknown-linux-gnu`. Keep this field alongside the new `toolchain:` field.
- **CI may fail after this change if `1.96.0` exposes new clippy warnings that `@stable` didn't.** This is intentional — `@stable` was hiding issues. Fix any new warnings in the same PR.
