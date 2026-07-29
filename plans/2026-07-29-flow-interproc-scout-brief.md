# flow-interproc scout brief (codex luna, READ-ONLY analysis lane)

Objective: produce the gap analysis that lets the flow-interproc port arc be
dispatched with zero unpriced surprises. Deliverable = ONE new file,
`plans/2026-07-29-flow-interproc-scout.md`, committed on your branch (or left
uncommitted if git metadata writes fail — then just leave it in the tree and
say so). You change NOTHING else.

Context: v6 alpha's flagship is dataflow analysis. Step 1 (callgraph rail
graded v5-vs-v6, `v6/tsv2/scripts/flagship-callgraph.sh`) landed. Step 2 is
porting `examples/flow-interproc.dl` the same way. It was blocked on resolved
call edges; that block just fell: `extract --resolve PATH...` now emits
`resolved_edge` JSONL (v6/sprefa-extract/src/bin/extract.rs, `resolve_project`;
record shape in src/types.rs `FlatFact::ResolvedEdge`).

## Questions to answer, each with file:line receipts

1. **Consumed-rel inventory.** Read `examples/flow-interproc.dl` AND
   `std/flow.dl` (it `use`s it). List every body relation
   (df_edge, df_node, df_param, df_arg, call_edge, call_edge_bare, type_sig,
   scan, closure, anything else std/flow.dl pulls in) and, for each, WHERE the
   v5 engine produces it (grep src/ for the producing code; the df value lift
   lives somewhere in the syn/oxc/tree-sitter lift path; call_edge resolution
   incl the SCIP-preferred path; type_sig extraction).
2. **v6 coverage map.** For each consumed rel: does v6 have the facts today?
   The v6 extractor's full contract is `extract --schema`
   (v6/sprefa-extract/src/bin/extract.rs SCHEMA) — families cst/type/call,
   records def/edge/site/sig/const/specifier + the new resolved_edge. Say
   per-rel: COVERED (by which record), APPROXIMABLE (how, at what precision
   loss), or ABSENT (df value-lift plane expected here — say exactly what an
   extractor df family would need to emit).
3. **closure() spelling.** What `closure(flow_edge)` actually is in v5 (grep
   for it in src/; builtin? special eval?). v6 has recursive strata compiled
   incrementally (P2/P3, support-count retraction with cycle-reachable reseed
   — see plans/2026-07-29-emitter-p2-p3-header.md and the P3 rows in
   CLAUDE.md). Lay out the v6 spelling options: (a) direct two-rule recursion,
   (b) a closure sugar expanding to (a), with the vocabulary law in mind
   (rxjs/prolog/SQL words only). Recommend smallest correct.
4. **Grading rig delta.** Read `v6/tsv2/scripts/flagship-callgraph.sh` and
   `flagship-classify.py`. What changes to grade flow-interproc the same way
   (v5 leg hermetic + DL_STATE_DIR-isolated, per-bucket classification, rule
   fidelity legs)? Note: flow_reach is cyclic/transitive — the callgraph arc
   explicitly did NOT grade reaches/closure; say what grading transitive
   closure output needs (row-count explosion risk on the pinned corpus?).
5. **Smallest correct port scope.** Of the four queries (flow_edge,
   flow_reach, flow_param_type, flow_node_type): which are gradeable with
   TODAY's v6 facts, which need the df lift, and the ranked gap list with a
   priced next step per gap.

## Laws
- READ-ONLY lane: the one plans doc is your only write. No source edits, no
  fixture edits, no regens.
- Any v5 `dl` run you need: hermetic ONLY —
  `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 dl <file> --db <scratch>`
  with a scratch db path under /tmp. NEVER touch ~/.local/state/sprefa or the
  running daemon.
- Line numbers in this brief may be stale; re-find by symbol name.
- If a question cannot be answered within these laws, write the named blocker
  in the doc instead of improvising.

## Required doc shape
Sections 1-5 above, each ending in a table or ranked list; a final
"DISPATCH-READY?" verdict line saying what the port arc can start on
immediately vs what waits on extraction work.
