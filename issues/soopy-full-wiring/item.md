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
