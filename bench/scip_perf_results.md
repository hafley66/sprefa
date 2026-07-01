# SCIP perf — measured on v5, 2026-06-30

Reproduce: `bench/scip_perf.sh rust .` (from the repo root) plus the in-place cold/warm
split below (run `rust-analyzer scip` twice in the same dir).

## rust-analyzer scip on v5 (real numbers)

| experiment | result |
|---|---|
| cold `rust-analyzer scip .` | 11.37s real / 15.63s user, 9.58 MB index |
| "warm" 2nd CLI pass (caches primed) | 10.59s — same as cold |
| same dir, same state, two passes | byte-identical |
| two separate checkouts, same source | differ at char 140 — exactly 1 string each (the embedded absolute root) |
| string diffs after root-strip | 0 → OID-shareable modulo path |
| dl ingest of the 9.58 MB index | 0.64s (5.8% of the 11s gen) |

## The warm/cold finding (the correction)

There are two rust-analyzers, and only one is warm:

- **RA-the-server** (editor / daemon): salsa lives in memory, go-to-def / refs
  answer in ms. Genuinely incremental per keystroke. This is the warm one.
- **`rust-analyzer scip`** (the batch CLI): forks a fresh process and rebuilds
  salsa from zero every run. Cold 11.37s, "warm" 2nd run 10.59s — no reuse.
  There is no warm path for the CLI.

Corrected claim: **SCIP-via-CLI is always ~11s; the warm oracle is the server,
and the server does not emit SCIP.**

## Consequences for the plan

- **Option 2 (LSP-client oracle)** is the only true per-edit incremental route,
  because the batch CLI cannot be warmed.
- **Option 1 / 4 (cache + share the 11s artifact across worktrees)** is the right
  move for the batch path: same-state indexes are identical modulo one path
  string, and dl already keys files by relative path, so OID-sharing +
  path-rebase is safe and cheap (ingest 0.64s ≪ gen 11s).

## TS / TypeScript 7 (tsgo) note

tsgo (`@typescript/native-preview`) is the Go port, ~10x faster typechecks, but
has no SCIP emitter; `scip-typescript` still rides the old TS6 JS compiler, so
the Go speedup does not reach SCIP indexing yet. Go itself has `scip-go`.
