---
name: project_dl_productionization
description: dl productionization arc — root workspace, macOS CI, cargo-dist prebuilt releases, setup skill-dest fix, and the .cargo/config nightly gotcha
metadata:
  node_type: memory
  type: project
  originSessionId: bd430550-432d-4c86-aa40-8828c5757705
---

Productionization arc on main (2026-06-30), after the ghcacher effect-logging
work [[project_sh_effect_runtime]] / [[project_dl_self_validation_docs]]. Three
tracks the user ordered: resp-upsert → CI → dist.

**resp latest-wins (1c9798a):** `resp` accumulates one row per response; a
CHANGING resource lands multiple 200s with different etags, and the naive carry
`etag_next <- resp(200, tag, _)` then derives MANY etags → poll fans out (a
latent bug). Fix is pure-dl: thread the `clock` bucket into `resp`,
`resp_latest(ep, max(b))` picks the newest 200, `resp_current` is that one body,
etag carry + entities read `resp_current` (single-valued; a 304 holds the last
good). `change_log` still keeps full history. Applied to gh-cache.dl /
-config.dl / -full.dl; e2e test resp_current_is_the_latest_wins_view... .

**THE .cargo/config gotcha (9e5b3ff):** root `.cargo/config.toml` was TRACKED
with `rustflags = ["-Z","threads=8","-C","link-arg=-fuse-ld=lld"]` — nightly +
lld, local-dev-only. It broke EVERY stable consumer: CI, the release runners,
and a stable `cargo install --git`. Crate has NO nightly LANG features (no
`#![feature]`, builds clean on stable). Fix: `git rm --cached` + gitignore it
(working copy stays). If a future stable build mysteriously fails on `-Z`, check
this file is still untracked.

**Root workspace (9e5b3ff):** added `/Cargo.toml` (members = v5 +
v5/tree-sitter-dl; exclude v3, v4, v5/src/v5cozokuzu; `[profile.dev/test]`
hoisted from v5, since profiles in a non-root member are IGNORED). Required so
`cargo install --git <url> sprefa-v5 --bin dl` resolves the subdir crate. Root
Cargo.lock now drives; v5/Cargo.lock removed. Build target moved to /target
(was v5/target). v5/Cargo.toml gained package metadata
(description/license/repository/homepage/readme/keywords/categories) for
install + cargo-dist installer URLs.

**CI (9e5b3ff, .github/workflows/ci.yml):** runs on **macos-latest** (NOT
ubuntu) — `dl` deps `tray-icon`/`tao` are UNCONDITIONAL (no cfg gate), need GTK
on Linux, tray is macOS-v1. Steps: `cargo test -p sprefa-v5 --lib` + `--test it`
(it carries the LSP smoke tests/it/lsp_protocol.rs + dl_diag), build dl, dogfood
`./target/release/dl --check --no-daemon --root .` (clean tree exits 0), assert
.githooks/pre-commit still greps `dl --check`. First push (pre-workspace) FAILED
in 22s on the `-Z threads` config; fixed by the untrack + workspace.

**cargo-dist (e8c2f66):** installed via `cargo install cargo-dist --locked`
(curl|sh installer is BLOCKED by the auto-mode policy — needs a Bash settings
rule, a chat "you're allowed" doesn't override it). `dist init` →
dist-workspace.toml + `[profile.dist]` (release+thin-lto) + release.yml. Tag `v*`
→ build + tarball + sha256 + GitHub Release + shell installer. **macOS targets
only** (aarch64+x86_64-apple-darwin); Linux/Windows DEFERRED until tray-icon/tao
are cfg-gated to macOS. Each artifact ships dl + scc_reach. `dist generate`
after editing dist-workspace.toml; `dist plan` validates; `dist build
--artifacts=local` verified locally (6.3M tarball).

**setup skill-dest fix (cf778c1):** `dl setup` on a fresh machine where
~/.claude exists but ~/.claude/skills doesn't fell through to
~/.config/sprefa/skills (Claude Code never reads it) and skipped the CC copy.
Now detects Claude Code by ~/.claude (config dir) + creates skills/; warns when
no agent found.

**PACKAGE RENAMED sprefa-v5 → sprefa-dl (ec86f59):** install/publish/dist-artifact
name is now `sprefa-dl` (`cargo install --git URL sprefa-dl --bin dl`; artifacts
sprefa-dl-*.tar.xz). The internal LIBRARY crate keeps `[lib] name = "sprefa_v5"`
so the ~20 `use sprefa_v5::…` files don't churn — package name != lib name. CI
uses `-p sprefa-dl`. (Internal lib name + SCIP moniker still say v5; full rename
deferred, cosmetic.)

**STILL OPEN:** cfg-gate tray-icon/tao to macOS so Linux/Windows CI + dist
targets can be added (the real cross-platform unblock). The first macOS CI green
+ first `v*` tag release are the remaining confirmations.
