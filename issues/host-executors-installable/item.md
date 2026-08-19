---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: M
blocked_by: ['@dl6-build-single-binary']
---

# sprefa_extract and soopy installable or bundled for programs outside the repo

## Description

## Description
`sprefa_extract` and `soopy` are in-repo crates reached through `DL_EXTRACT_BIN` / `SOOPY_BIN`. A program that uses the `sprefa_extract` or `soopy` adapters cannot run where the repo is absent.
## Acceptance Criteria
- [ ] `just install-extract`, `just install-soopy` (version-stamped like install-boop), or one `dl6 build --bundle` that links them in-process on the Rust door (the Rust door already calls them in-process through the sidecar rows, `hosts.rs`); pick by measuring binary size and build time, write the two numbers here.
- [ ] `fresh-machine.md` gains the one extra command if installs are separate.
