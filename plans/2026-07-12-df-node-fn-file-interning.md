# Intern df_node.fn and df_node.file to sym (i64)

## Why (dbstat evidence)
`df_node` is the slowest cold write (~720ms/154k rows). The bytes, from the
`dbstat` vtab (now surfaced by `Db::rel_stats` in perf.jsonl):

| KB | object |
|---|---|
| 19,628 | `sqlite_autoindex_rel_df_node_rev_1` (full-row PK) |
| 18,696 | `sqlite_autoindex_rel_df_node_1` (full-row PK) |
| 17,428 | `rel_df_node_rev` table |
| 16,640 | `rel_df_node` table |
| 11,816 | `idx_df_node_rev_fn` |
| 9,652  | `idx_df_node_fn` |

`df_node.fn` averages 54 text bytes, `file` 21, stored raw on every row and
duplicated into every index over them (and into the full-row PK). `id` is
ALREADY interned to `sym` (an i64 StringId). Interning `fn` and `file` the same
way collapses ~13MB of repeated text in the table plus ~21MB of fn-index text.

## The hard part: cross-family joins (DO NOT SKIP)
`df_node.fn` and `.file` are JOIN KEYS across ~17 `.dl` files. Interning the
column without interning what it joins against SILENTLY BREAKS the join (an i64
never equals a text). Blast radius to audit and keep sound (grep first, re-find
by name — line numbers drift):
- `df_node.fn` (the BARE enclosing-fn sym `file::kind::name`) joins against:
  `std/flow.dl` (df_node.fn vs call_site.callee / member sym), `std/entry.dl`
  (entry_seed/op_endpoint bare-fn bridge, `entry_reach_node ... df_node(node,_,_,fnb,_,_)`),
  `.dl/flow-panel.dl` (many: fill_fn/read_fn/bare_fn/param/ret), `.dl/rails.dl`,
  `.dl/mark-lens.dl`, `.dl/graph-diff.dl` (df_node_rev.fn).
- `df_node.file` joins against `call_site.file`, `loop_over.file`,
  `comment_node`/`node` file columns, and `mark(...).path` (mark-lens.dl:38
  `df_node(id,_,marked,_,path,line)`).

Two sound strategies (PICK PER COLUMN, justify in the summary):
  (A) **Intern the whole join cluster.** Change df_node.fn AND its join partners
      (call_site.callee, entry_seed.fn, op_endpoint.fnb, ...) to sym together, so
      i64==i64 joins hold. Widest change, but keeps joins native-int (fastest).
  (B) **Intern df_node's column only, decode at the join in .dl** via `sym(col)`
      where a consumer compares it to a text column. Smaller code change, but
      every consumer join needs an explicit `sym(...)`/text bridge and it's easy
      to miss one.
Strategy (A) is preferred for `fn` if the partner set is bounded; (B) is the
fallback. `file` is a path shared with many rels — (A) likely too wide, start
with (B) or leave `file` for a follow-up and do `fn` first (fn is the bigger
win: 21MB of index vs 4.5MB).

## Parity gate (non-negotiable)
This changes join semantics. BEFORE and AFTER, the `?`-query row sets of every
reach/flow rel MUST be byte-identical:
`port_reach`, `entry_reach_node`, `op_reach_node`, `entry_reach`, `op_reach_fn`,
`flow_edge`, `member_edge`, `panel_node`, `panel_edge`, `mark_df`.
Method: run `.dl/flow-panel.dl` + `std/entry.dl` on a fixed scratch corpus
before the change (capture row sets), and after — assert identical. Any drift =
a broken join = STOP and report which rel and which join.

## Order of work
1. Land the introspection FIRST (done: `Db::rel_stats`, perf.jsonl detail,
   `DL_PROFILE_EXTRACT`). Use it to measure before/after df_node write ms AND
   the dbstat byte drop.
2. `fn` interning (strategy A if bounded, else B) + parity gate + measure.
3. `file` interning as a SEPARATE commit only if `fn` lands clean.
4. Update the `df_node.fn`/`.file` column types in `src/engine/decls.rs`
   (Type::Text -> Type::Sym), the extractor emit in
   `refresh_dataflow_rels` (`src/engine/extract.rs`: intern fn/file through the
   existing `SymSink`, same as `id`; flush_syms already batches), and every
   `.dl` consumer join. The `_rev` twins (df_node_rev) carry the same columns —
   do them in lockstep or the twin write stays fat.
5. Column typecheck: a `sym` column compared to a `text` column must be a typed
   error or an explicit bridge, never a silent no-match — check `src/typecheck.rs`
   flags it (the sym/text split from the intern-key arc should already).

## Laws
- No `provenance`/`substrate`/`load-bearing`/`regime` identifiers.
- Descriptive dl var names, never single-letter (rename opportunistically in any
  `.dl` rule you touch: `fnb`/`callee_bare` are fine; `f`/`l` are not).
- N+1: emit stays collect-then-flush (the SymSink batch); no per-row intern SQL.
- One-rel-one-rule-kind still holds; df_node stays a single extract rel.
- Hermetic runs: `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, scratch --db.
- `git commit -n`, do NOT push. One commit per column (fn, then file).

## Escape hatch
If a join cannot be kept sound within the parity gate, STOP that column and
report the exact rel + join in the summary. A silently-changed flow graph is far
worse than a still-text `fn`.

## Final summary
Per-commit shas; strategy (A/B) chosen per column + why; the parity-gate result
(row sets identical, list the rels checked); df_node write ms before/after and
the dbstat byte drop (from rel_stats); full-suite pass/fail; anything skipped.
