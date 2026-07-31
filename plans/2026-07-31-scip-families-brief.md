# scip + diet_scip extractor families — brief (opus worktree)

User directive 2026-07-31: "sprefa-extract should have way to call out for
scip indexing, see dl5's scip_want, we may as well just make 2 families,
diet_scip and just scip, anything 'diet' means parse technique and
heuristics, not actual scip data." This EXPLICITLY supersedes the
extractor-is-fixed directive for this lane.

## Prior art to REUSE, not rewrite (v5, repo root src/)

- `src/scip_setup.rs`: the INDEXERS table (lang, marker files, bin,
  install hint, argv with {out}), `ensure_index` (existing index wins;
  detected+installed indexer runs once to `.dl/index.scip`; no
  toolchain = LOUD named skip), merge_files for multi-root.
- `src/rels/scip.rs`: the scip protobuf ingest and the 10-rel
  projection (scip_def/name/ref/edge/fn_edge/callee_type/local/impl/
  occurrence/binding) with the moniker-self-disambiguation notes.
Port or share what fits sprefa-extract's crate shape; state what you
took and what you changed. Both `rust-analyzer` and `scip-typescript`
are INSTALLED on this machine — receipts run real indexers.

## The two families

- **`scip`** (new): real SCIP index data. `extract --family scip
  <root>`: ensure-index (v5 contract: cache at a stable path, reuse,
  else run the detected indexer UNDER A BUDGET per the timeout-gun law
  — process-group kill + named timeout skip), then project the index
  to JSONL records mirroring the v5 rel shapes (def/name/ref/edge/
  fn_edge/callee_type/local/impl minimum; occurrence/binding optional,
  say why if skipped).
- **`diet_scip`** (rename-in-place): the EXISTING tree-sitter +
  heuristic resolve outputs (the terra --resolve pass) under the
  honest label. "diet" = parse technique + heuristics, never actual
  scip data — put that sentence in the --help text and family.rs.
  EXISTING family names (cst,type,call,df and current callers:
  v6/dl/src/4_ingest.ts, the extraction host templates,
  flagship/atlas scripts) MUST keep working — diet_scip is additive
  naming; whether old spellings deprecate is a user call, note it.

## Receipts

- CLI golden tests pinning both families' JSONL contracts (the
  bin-vs-lib parity precedent from the --resolve lane).
- `scip` family run REAL on: (a) the pinned 13-file rust flagship
  corpus via rust-analyzer, (b) one small TS dir via scip-typescript.
  Row counts stated; a def known to resolve cross-file in real SCIP
  but NOT in diet_scip is the discriminating receipt (find one, pin
  it — that difference is the whole point of the two names).
- no-toolchain path: named skip line, exit contract stated.
- indexer-under-budget: planted-slow (or absent-binary) receipt.
- v6 ingestion smoke: one extraction-host demand row per family lands
  EDB arrivals through the served engine (the extraction-live
  pattern); do NOT rewire any existing host.
- Rust test posture per the standing directive: byte/golden grading,
  no parallel test pyramid.

## Fences

- Worktree law: first action `git merge --ff-only edfe1743`; failure =
  STOP AND REPORT.
- Yours: v6/sprefa-extract/**, its golden tests, ONE new v6 fixture +
  receipt script leg for the ingestion smoke.
- NOT yours: 3_clock_check.pl (fix lane), all existing receipt scripts
  (timeout lane owns every script — put your smoke in a NEW script),
  v5 src/** (READ scip_setup.rs/rels/scip.rs freely, copy code out,
  never edit), bench-cli/**.
- pnpm install per package; never symlink outer node_modules.
- Style laws per CLAUDE.md. Commit per step `git commit -n`; no push.

Report: base verification, what was ported vs written, both families'
record shapes, the discriminating cross-file receipt, counts, budgets.
