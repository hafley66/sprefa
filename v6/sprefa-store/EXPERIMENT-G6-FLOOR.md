# G6 — drive retract_scc BELOW DRed (the perf floor)

You own `v6/sprefa-store/` ONLY. A parallel job owns `v6/labkit/`; do not touch
it. Do NOT change the public signatures of `retract`, `retract_dred`,
`retract_dred_cte`, `retract_scc`, `alive_keys`, `add_rows`, `add_deps`,
`assert`, `conn`, `attach` on RelStore — optimize the INTERNALS only.

## The result to beat (measured, in PERF-REPORT.md)
`cascade::retract_scc` is CORRECT on cycles but too slow: CYC 960k = 2890 ms,
which is ~7.1x plain counting (408 ms) and ~1.22x DRed-loop (2361 ms). It does
NOT yet beat DRed. The pinned design says SCC nested fixpoint should be CHEAPER
than DRed's two full passes. Deliver that. Target: retract_scc < DRed-loop on
CYC 960k, and ~= plain counting on pure-DAG cuts.

## Why it is slow (read cascade.rs retract_scc, ~line 266)
It makes ~3 passes over the cut's forward cone:
  1. `retract()` runs the full counting cascade — walks the cone once.
  2. `cx_scc_scope` is then built by a RECURSIVE CTE that re-walks the SAME cone.
  3. the nested fixpoint walks the cone a THIRD time.
DRed is 2 passes. Three passes cannot beat two. Collapse the redundancy.

## Levers (measure each hermetically; keep only wins, revert losers)
1. **Only counted-positive members need correction.** After step 1, any cone row
   counting left at weight=0 is already correctly dead (DAG-correct). The SCC
   phase only needs the rows counting left weight>0. Restrict scope + fixpoint to
   those. On a mostly-acyclic cut this shrinks the SCC work to near zero.
2. **Skip the SCC phase entirely** when no counted-positive member remains in the
   cone after step 1 (a pure-DAG cut): then retract_scc == retract, ONE pass. Add
   the cheap probe and early-return.
3. **Fuse cone capture into counting** so the scope is a byproduct of pass 1, not
   a separate recursive re-walk (eliminate pass 2). If retract() cannot expose
   the cone without changing its signature, capture it inside retract_scc's own
   first walk instead of calling retract() then re-walking.
4. Keep the PK-frontier / INSERT-OR-IGNORE / ping-pong already in place; look for
   further per-round statement fusion (74->56 already done; go lower if it wins).

## MEASUREMENT DISCIPLINE (non-negotiable — the user is emphatic)
- Interpret your own numbers firsthand in the result doc; say what each means.
- DISTRUST your first output. Re-run every headline number at least twice and
  confirm the correctness HASH is identical across runs before believing a time.
- Identical-across-scale numbers are a RED FLAG to disprove, not a flex. A flat
  column must be EXPLAINED (e.g. Rust stays ~0.1MB because rows live in SQLite —
  prove it by showing sqlite_hw and db MB DO scale) or it is a bug.
- Correctness is blake3/hash vs `benchgraph::oracle_survivors`, per-process.
- Every optimization re-verified against the oracle; a faster-but-wrong variant
  is a failure, not a win.

## Validate
- `cargo test --test agreement` stays GREEN (retract_scc still byte-identical to
  oracle on all 15 cyclic + 3 DAG shapes). Do not weaken the test.
- `cargo run --example perf_report` regenerates PERF-REPORT.md; the new
  retract_scc row must show a LOWER CYC 960k ms than the DRed-loop row in the
  SAME report, still `yes`.
- `rg -n 'eprintln' src/` — no NEW eprintln (tracing only).

## Commit
Leave changes STAGED (`git add -A v6/sprefa-store`). Do NOT commit (pre-commit
hook execs `dl`, not on PATH; do not bypass). Coordinator commits.

## Result file
`v6/sprefa-store/EXPERIMENT-G6-RESULT.md`: each lever tried, its measured
before/after WITH your firsthand read, what you re-ran to falsify yourself, the
final retract_scc-vs-DRed comparison, and any lever you reverted and why. Terse.
