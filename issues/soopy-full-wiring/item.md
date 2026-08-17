---
created: 2026-08-16
updated: 2026-08-16
type: epic
owner: chris
status: open
priority: high
---

# Soopy fully wired as the one source-identity layer

## Description

Every byte read and every revision named in sprefa-extract, sprefa-engine-rs, and the dl6 hosts goes through soopy types and reads. Measured base: plans/2026-08-16-soopy-extract-entanglement.md (13 collapse candidates). Extract core needs no changes: dispatch(path, content, mask) already accepts caller bytes; all defects are call sites choosing raw disk.

## Goal

One source-identity layer. Every byte and every revision in extract, engine,
and the dl6 hosts flows through soopy; no raw `std::fs::read` where a rev pin
exists; digests comparable end to end. Measured base:
`plans/2026-08-16-soopy-extract-entanglement.md`.

## Issues

- [ ] @soopy-extract-host-reads (high) — kills the rev-pin identity defect
- [ ] @soopy-blobsource-revive (high) — rev-correct reader into production
- [ ] @soopy-contentid-adoption — one digest type end to end
- [ ] @soopy-change-facts-work — the `--changed` question in v6
- [ ] @soopy-catfile-gitbatch
- [ ] @soopy-typed-seams (cross-repo: soopy serde derive)
- [ ] @soopy-depcrawl-manifest
- [ ] @soopy-refmemo-freshness
- [ ] @soopy-lockfile-unify

## Phases

1. Reads: extract-host-reads, blobsource-revive (disjoint files, parallel)
2. Identity: contentid-adoption (wide, lands after phase 1 to avoid churn)
3. Revisions: change-facts-work, depcrawl-manifest, catfile-gitbatch
4. Hygiene: typed-seams, refmemo-freshness, lockfile-unify

## Comments

Extract core is untouched throughout: `dispatch(path, content, mask)` already
takes caller bytes. All work is call sites and host executors.

### 2026-08-17T03:54:37Z · @soopy-driver

CI FINDING, pre-existing, epic-relevant. The cargo-dist 'plan' job fails on EVERY sprefa PR and has since the soopy path dependency landed: 'failed to read /home/runner/work/sprefa/hafley-rs/crates/soopy/Cargo.toml: No such file or directory'. Receipts: run 31992387756 (my PR #335), and the same failure on already-MERGED PRs #328 and #329. v6/sprefa-extract/Cargo.toml:95 and v6/sprefa-engine-rs/Cargo.toml:22 both point at ../../../hafley-rs/crates/soopy, which exists only on this machine, so neither crate is buildable in CI and cargo metadata cannot even read the workspace. Consequence for @soopy-lockfile-unify (PR #332): both sprefa lockfiles' soopy closures are resolved against a LOCAL hafley-rs working tree, so their contents depend on unpushed state in another repo. The lockstep rail holds the two in agreement with each other, which is the card's scope; it cannot make them reproducible off this machine. Fixing that means a git or registry dependency on soopy instead of a path, or vendoring, and that is a call for Chris.

### 2026-08-17T03:55:19Z · @soopy-driver

EPIC ROLL-UP from soopy-driver. LANDED BEFORE I STARTED, verified against origin/main and closed: soopy-blobsource-revive (PR #309), soopy-extract-host-reads (PR #310), soopy-catfile-gitbatch (a16a16a83), soopy-depcrawl-manifest (PR #308). MERGED THIS SESSION: hafley-rs PR #9 (soopy gains pub ContentId::blake3, ReadRequest serde derives, pub git_dirs) and sprefa PR #330 (extract-flow-cli-dispatch). OPEN, all graded green by me, all MERGEABLE: #331 soopy-change-facts-work (WORK revisions, 90/0 twice), #332 soopy-lockfile-unify (one 127-crate soopy closure plus the lockstep rail, extract 131/0 and engine 87/0 twice), #333 the blake3 dep drop (142/0 twice), #335 soopy-refmemo-freshness (89/0 twice, and it FLIPS a committed assertion: see the PR). NEEDS CHRIS: soopy-typed-seams, all four candidates measured wrong, blocked, or out-of-repo, see that card's two notes. NOT STARTED: extract-blob-cache-parallel, which by repo law needs a written candidate-by-candidate library analysis (moka / quick_cache / lru / plain HashMap) before any bespoke cache code, and measurement on the scale corpus; it is not mechanical-lane material.


