# port_reach perf re-attack (2026-07-12)

## Target
`port_reach` in `.dl/flow-panel.dl` is the slowest rule on a clean isolated
sprefa build: **~1654ms cold** (member_edge is next at ~1449ms). Bring it under
the 1000ms per-statement budget WITHOUT changing its result rows.

`.dl/flow-panel.dl` IS the source of truth (git-tracked; the "served copy" in
CLAUDE.md is the daemon's in-memory image, not a separate file).

## The rule (re-find by name; line numbers drift)
```
rel port_reach(port: text, node: text).
port_reach(p, n) <- port_of(a, p), flow_edge(a, n).
port_reach(p, n) <- port_out(p, a), flow_edge(a, n).
port_reach(p, n) <- port_reach(p, m), !port_of(m, _), flow_edge(m, n).   # recursive hop
```
A graph-contraction walk: from each port's node, walk `flow_edge` forward,
halting at any node that is itself a `port_of` pin. `port_of` is SMALL;
`flow_edge` is LARGE (~166k rows on sprefa alone, indexed on the `from` column).

## Known-bad approach (do NOT repeat)
Pulling the anti-join out as `flow_open(m,n) <- flow_edge(m,n), !port_of(m,_)`
read by the recursive hop REGRESSED to >10s: it computes the anti-join over ALL
flow_edge rows up front, discarding the demand restriction the port tag gives.
Do not pre-cross the anti side with flow_edge.

## Approaches to try (measure each; keep only a proven win)
1. Narrow the ANTI side to a small membership set only: `has_port(node) <-
   port_of(node, _).` then the recursive hop reads `!has_port(m)` as an indexed
   NOT-IN against a small 1-col set, NOT a materialized open-edge product. The
   difference from the known-bad attempt: never materialize `flow_edge` filtered
   rows; only make the membership test cheap.
2. Column/key order: check whether the recursive hop joins `flow_edge` on an
   indexed column and whether reordering body atoms or the rel key changes the
   plan. Grep the engine's index creation to see what indexes exist on
   flow_edge / port_of.
3. If neither helps, a proven "can't, because <atom> dominates" is an acceptable
   outcome — say which atom and why it can't be narrowed.

## Measurement (the honest number; NOT the daemon, which serves 5 roots)
```
cd <worktree> && rm -f /tmp/pr-iso.db && \
  SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 dl --db /tmp/pr-iso.db 2>&1 \
  | grep -oE 'rebuilding `[a-z_]+` took [0-9]+ms|total derived-rebuild time this tick is [0-9]+ms' | sort -u
```
Fresh `/tmp/pr-iso.db` each run (cold, comparable). ALWAYS pipe through that grep
— a bare `dl` prints ~100 lines of file-size lint.

## Result-unchanged proof (required)
Capture `port_reach` row count + a sample before and after; assert identical.
Read `rel_port_reach` from the scratch `--db` SQLite, or `? port_reach(port, node).`

## Laws
- Style: this file uses single-letter dl vars already (p/n/a/m) — match the
  file's existing style (colocated consistency), do not rename them.
- One rel = one rule kind (never head a rel with both a source and derived rule).
- No banned identifiers (provenance/substrate/load-bearing/regime).
- Hermetic runs only (SPREFA_CONFIG=/nonexistent + scratch --db). Never touch
  ~/.local/state or a running daemon.
- Commit protocol: one change, `git commit -n`, do NOT push.
- If a step can't be done within the laws, STOP that step and note why.

## Final summary shape
The rewrite (rels added + rule diff), before/after ms on the measurement command,
result-unchanged evidence (row counts + sample), new total tick time, and any
skipped approach with the reason.
