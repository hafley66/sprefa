# Rust hygiene SQLite pin

## Context

Five manifests use `rusqlite` 0.32 with `bundled`, which resolves `libsqlite3-sys` 0.30.1. SQLite's WAL documentation, section 11, lists the WAL-reset bug in SQLite 3.7.0 through 3.51.2 and its fix in 3.51.3. The root manifest also depends on `apalis-sqlite` 1.0.0-rc.8, which requires SQLx 0.8.6 and `libsqlite3-sys` 0.30.1.

`v6/sprefa-engine-rs/grade.sh` checks the Rust corpus but had no `just` recipe. `emit_rust_harness` prints its command-line usage to stderr.

## Decisions

The four independent V6 manifests require `rusqlite` 0.40, matching `v6/boop/Cargo.toml`. The root pin requires an `apalis-sqlite` release compatible with a single `libsqlite3-sys` 0.38.x package.

`rust-grade` runs `v6/sprefa-engine-rs/grade.sh` and is included in `green`; the script rejects a byte-clean result below 230.

The usage line remains stderr CLI output with an `@eprintln-ok` marker.

## Verification

Run each affected crate's SQLite version query, then run `cargo build --workspace`, `cargo test --no-fail-fast`, `bash v6/sprefa-engine-rs/grade.sh`, and `just conformance` three times each. Record cold and warm `rust-grade` measurements.

## Staffing

Implementation: Codex agent. Worktree: yes. Base SHA: `0b672fc11ef2d73478a72849c62d921074f460b4`. Suite budget: individual gates only.
