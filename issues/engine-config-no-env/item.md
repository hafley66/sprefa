---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: S
blocked_by: ['@dl6-build-single-binary']
---

# Engine config without repo-relative env defaults

## Description

## Description
Runtime behavior depends on env vars with repo-relative defaults: `DL_EXTRACT_BIN`, `DL_ADAPTERS_DIR` (`types.rs:631` defaults to `CARGO_MANIFEST_DIR/../dl/fixtures`), `SOOPY_BIN`, `$DL_*` inside fixture `sh` templates. An installed binary outside the repo gets wrong defaults silently.
## Acceptance Criteria
- [ ] One config source for the Rust door: CLI flags first, then `<prog>.toml` beside the binary, then env; defaults that work from an installed binary (no `CARGO_MANIFEST_DIR`).
- [ ] Every env var the engine reads is listed by `<prog> config` and in `docs/config.md`; a missing required executor path is a named error at boot, never a spawn failure mid-tick.
- [ ] Fixtures keep `$DL_EXTRACT_BIN` (the shell adapter fills it from config).
