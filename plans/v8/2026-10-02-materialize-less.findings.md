# Findings: which sqlite_ivm nodes own tables, and why (before the change)

Measured at sqlite_ivm `164f414`, corpus `crates/ivm-sqlite/tests/corpus/8_c15_program.json`,
`cargo test -p ivm-sqlite --test 6_install_growth c15_shaped` (dev profile, `RCARGO_OFF=1`):
`creates=4499 objects=4497 install=645ms`. Object census from `sqlite_master` after install.

## IR shape

| op | nodes | readers by op |
|---|---|---|
| Get | 1495 (946 of a Source, 545 of a Derived, 4 of a Constructor; 334 distinct relations) | Join 1050, Mfp 398, Union 178, Antijoin 148, Mint 15 |
| Join | 986 | Join 449, Mfp 290, Mint 147, Antijoin 93, Reduce 7 |
| Mfp | 861 (360 pure projection, 286 filter only, 215 with map) | Union 473, Join 269, Mfp 73, Antijoin 46 |
| Mint | 209 | Mfp 93, Join 63, Mint 40, Antijoin 9 |
| Union | 172 | Threshold 133 |
| Antijoin | 148 | Join 141, Mfp 7 |
| Threshold | 133 | none (stratum bodies) |
| Reduce | 7 | |

Relations: 188 Source (161 are `__ir_seed_n*` one-row ground seeds), 187 Derived, 15 Constructor.
Strata: 150 Let, 14 LetRec.

## dl8 lowering shape (`src/_6_eval/_9a_ir_lower.rs`)

- Each positive goal `(rel ...)` emits one `Get(rel)` (`:1281`) and one two-input `Join` onto the
  running left-deep chain. Repeated variables add a filter `Mfp` on the right (`b.mfp(right,
  repeated, ..)`); each ground argument adds a `Join` with a `__ir_seed_n*` source (`mapped_ground`).
- The head is one projection `Mfp` per rule; a relation's rules meet in one `Union`, then one
  `Threshold` (set semantics) as the stratum body.
- A product declaration plus rules over it therefore costs, per rule: Get + Join per goal, one
  head Mfp, one Union branch; per relation one Union + one Threshold.

## Which ops get tables today (`crates/ivm-sqlite/src/3_nodes.rs`)

- `_d` (delta) table: every node not marked `inline`. Inline (CTE, `AS MATERIALIZED` in each
  reading statement): Mfp, Union, Negate, Threshold, decode, filtering/Int `Str`. Physical: Source,
  Join, Mint, text StrCons/Str, Antijoin (recursive left), Reduce, TopK, Window, LetRec variables,
  bodies and results, SCC copies.
- `_i` (integrated image, `WITHOUT ROWID`, PK = all columns): any node passed to `integrate`.
  Callers: every Source (`SqlRel::new`), both Join inputs (`join`), Threshold input, Reduce input
  with an `Any` key, TopK/Window input and output, Antijoin keys and recursive left input, LetRec
  variables and bodies, outputs, Constructor gets.
- Index per distinct integrate key: `CREATE INDEX ivm_nK_xJ ON ivm_nK_i (key)`.
- `Get` creates nothing: a Source `Get` returns the source node, a Derived `Get` returns the
  stratum's node, so identical Gets already share one SQL node (brief item 3 holds today).
- Identical joins (same input nodes, same predicates) already share one node (`e39fe50`).

## Census of the 4499 CREATEs

| object | count | cause |
|---|---|---|
| `_i` on an inline node (no `_d`) | 706 | Mfp/Union image read by a Join term or Threshold; its integration statement recomputes the CTE chain |
| `_i` on a physical node | 801 | Sources 188, Join inputs/outputs, Mint, SCC tables |
| `_d` | 1413 | physical nodes; 606 of them have no `_i` (Joins read only by Mfp/Mint/Antijoin) |
| index on `_i` | 928 | 408 of them are a prefix of the table's own PK (every Source's all-column index, single `c0` keys) |
| `_hold` | 128 | SCC copies of outer inputs |
| `_nx`/`_del`/`_acc` | 37 each (+6 `_acc`) | LetRec DRed |
| `_g` | 7 | Reduce |
| host source tables + unique index | 162 + 209 other indexes | 161 seeds + `__ir_*` sources, dictionary and constructor tables |

## What must keep state

Sources, LetRec members, Reduce accumulators, Antijoin right keys (a Threshold), Threshold
inputs (point lookup of the consolidated weight), TopK/Window inputs, outputs, SCC copies.
Everything read only by bilinear Join terms is a function of its inputs' images:
`I(mfp(x)) = mfp(I(x))`, `I(a ⋈ b) = I(a) ⋈ I(b)`, unconsolidated weights are fine because each
fill consolidates with `GROUP BY ... HAVING SUM(w) <> 0`.

## Plan

1. Skip indexes that are a prefix of the image's PK (408).
2. Image views: a non-recursive Mfp or Join whose image only Join terms read gets no `_i` table;
   the reading statement inlines `(SELECT ... FROM <input images>)` in place of `ivm_nK_i`, and the
   join key is indexed on the physical image underneath (pushed through passthrough columns).
   Bounded nesting (table count per view) keeps SQLite's flattener and 64-table join limit safe.
3. Join deltas read by exactly one settle statement become CTEs like Mfp; join work is still
   counted (the count statement expands the CTE).
4. ivm-dd: arrangement count measured before/after; DD arranges Join/Antijoin/Threshold/Reduce
   inputs and never an Mfp.

## Result (sqlite_ivm `234524b` on `perf/materialize-less`)

| change | CREATEs removed on c15 |
|---|---|
| views for join-read Mfp/Join/decode/Str images; Threshold image is a view over its stored input | 706 inline `_i` + their indexes, 256 Threshold `_i` |
| indexes a PK or longer index serves | 408 |
| join delta read by one fill is that fill's CTE; a union read twice is stored when two join deltas under it inline | about 240 `_d` |
| source delta read by at most `SOURCE_READS` statements is a CTE over the stage | most of 188 source `_d` |
| one `_hold`, `_nx`, `_del`, `_acc` per SCC | 128 + 111 tables down to 14 each |
| host source table WITHOUT ROWID with PK, no separate unique index | 188 |

- c15 install: 4499 -> 2232 CREATE, 645 ms -> 235-255 ms (`6_install_growth` pin updated).
- Inline join work: the reading fill leads its FROM with a one-row
  `(SELECT frontier_<program>_work(slot, count(*)) FROM <join CTE>)`, so the CTE is materialized
  and counted once per execution even when the fill reads none of its rows. Counts and output deltas
  matched the stored-delta plan on ten c15 frontiers.
- First cut without `SOURCE_READS` regressed the registry build to 4.4 s: a source read by hundreds
  of statements regrouped its stage rows in each. Sources read by more keep a `_d` filled once.
- ivm-dd: Mfp, Union, Negate, Mint and decode are `map`/`flat_map`/`concat`; DD arranges Join
  inputs (2 each, 702 non-decode joins), Antijoin (3 each, 148), Threshold (133), Reduce (7):
  1988 arrangements on c15, none on a single-reader pure node. No ivm-dd change.
