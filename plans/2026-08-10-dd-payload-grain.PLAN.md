# DD payload grain

## Context

Commit `f4ff55ed` added `sqlite(Refs, Statements)` to every `op/3`. A join therefore repeated its rule SQL on `map_1` and `join_1_1`. The emitter selects SQL by head reference in [`6_emit_dd_plan.pl:212`](../v6/prolog/compile/6_emit_dd_plan.pl#L212); relational descriptions are emitted separately by [`rule_operator_terms/4`](../v6/prolog/compile/6_emit_dd_plan.pl#L198).

## Type and lifetime

The emitted type is `op(Id, RelationalDescription, sqlite(Refs, Payload))`. A map has `Payload = Statements`. A join, filter, reduce, or iterate from the same rule has `Payload = owner(MapId)`. `Refs` remains the sorted head-plus-body set. The SQL bundle is created once while emitting the rule, lives on its map node, and owner nodes name that map for the duration of the plan.

For a SQLite runner, tick execution is one SQL bundle per map. The runner needs no executed-set:

```text
for op in tick_order:
  if op.sqlite is sqlite(_, Statements): execute Statements
  if op.sqlite is sqlite(_, owner(MapId)): continue
```

Option A requires a dedupe set, keyed by statement identity, on every tick:

```text
ran = set()
for op in tick_order:
  for statement in op.sqlite.statements:
    if statement not in ran: execute(statement); ran.add(statement)
```

Option C requires the lowerer to produce operator-cut SQL before the same loop can execute one statement per node. `edge_delta_project_sql/11` builds one complete delta projection from its trigger plus all positive, negative, and guard uses ([`lower.pl:2945`](../v6/prolog/lower.pl#L2945)). `level_statement_groups/4` groups adjacent rules by head and emits one delete plus all insert clauses ([`lower.pl:3038`](../v6/prolog/lower.pl#L3038), [`lower.pl:3053`](../v6/prolog/lower.pl#L3053)). `level_fixpoint_ir/5` builds head-scoped fixpoint walks ([`lower.pl:4051`](../v6/prolog/lower.pl#L4051)). None exposes SQL fragments for individual map, join, reduce, filter, or iterate nodes.

The pure-RAM kernel compiles the relational descriptions into map/join/reduce plus arrangements. It does not read `sqlite/2`. A leaves duplicated inert data, B leaves one bundle plus owner references, and C would add a second SQL-specific cut; none changes the RAM compiler's relational input.

## Option matrix

| Option | SQLite executions for a rule with N ops | Per-tick state | RAM effect | Join golden bytes | Emitter delta from `f4ff55ed` |
| --- | ---: | --- | --- | ---: | ---: |
| A: duplicate | N without a guard; 1 with statement-identity dedupe | `ran` set | none | 4,804 | 0 lines |
| B: map owns, siblings point | 1 | none | none | 2,899 | +8/-6 lines |
| C: per-op SQL | N, each distinct | none | none | no prototype | not measured |

Byte receipts from `wc -c`: A goldens were 2,162 (mirror), 4,804 (join), and 4,949 (average), total 11,915. B goldens are 2,162, 2,899, and 2,851, total 7,912. C has no measured golden because the current lowerer supplies rule/head SQL only, as cited above.

Multi-rule heads require an amendment for B: `level_statement_groups/4` has one `levelstmt` for an adjacent same-head group, so a future emitter must choose one map owner for that head group and point every other member at it. Rules sharing body relations require no B term amendment because each map names its own bundle and refs. Fixpoint subgraphs and filters retain the same owner form: iterate and filter are sibling descriptions and do not receive a SQL list. C requires new lowerer products for all four shapes. A can process every shape only with its identity set.

## Deep comparison: A and B

A keeps every node self-contained, but a payload-walking SQLite runner executes a repeated bundle once per node unless it carries statement identity state. That state determines whether SQL runs and is absent from the plan term. A join golden records the whole `levelstmt` twice.

B places execution authority on the map that writes the rule head. The operator graph remains fully visible, owner nodes retain the same relation boundary, and a runner's walk has a direct `Statements` branch. The join golden fell from 4,804 to 2,899 bytes. The implementation is +8/-6 emitter lines and +13/-3 test lines from `f4ff55ed`.

## Decision

Winner: B. The emitted plan contains one executable SQL bundle for each emitted rule map and an explicit owner edge for its sibling operators. The structural test in [`6_emit_dd_plan.test.pl:36`](../v6/prolog/compile/test/6_emit_dd_plan.test.pl#L36) checks that every payload statement occurs on its map only and that each sibling points to that map.

The join term is:

```prolog
dd_plan(float_exact_join_has_no_epsilon,rels([rel(left/2,[name,value],set),rel(matched/1,[name],set),rel(right/2,[name,value],set)]),arrangements([arr(arr_left_2,left/2,[name,value],[],signed),arr(arr_matched_1,matched/1,[name],[],signed),arr(arr_right_2,right/2,[name,value],[],signed),arr(arr_left_2_join_1_1_left,left/2,[name],[value],signed),arr(arr_right_2_join_1_1_right,right/2,[name],[value],signed)]),operators([op(map_1,map(matched/1),sqlite([left/2,matched/1,right/2],[levelstmt(matched/1,'DELETE FROM "matched"',['INSERT OR IGNORE INTO "matched" ("name") SELECT b0."name" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value"'],'INSERT OR IGNORE INTO "matched" ("name") SELECT DISTINCT d0."name" FROM "__frontier_left" d0, "right" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" UNION ALL SELECT DISTINCT d0."name" FROM "__frontier_right" d0, "left" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" RETURNING "name"',refcountsql('DELETE FROM "__support_next_matched"','INSERT INTO "__support_next_matched" ("name", "__refcount") SELECT "name", sum("__refcount") FROM (SELECT b0."name" AS "name", count(*) AS "__refcount" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value" GROUP BY b0."name") GROUP BY "name"','UPDATE "matched" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_matched" n WHERE n."name" = h."name"), 0)','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT -1, row_number() OVER () - 1, "name" FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "__new_matched"','INSERT INTO "__new_matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n LEFT JOIN "matched" h ON n."name" = h."name" WHERE h."name" IS NULL','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT 1, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__next_frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT OR IGNORE INTO "matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n',none,none,none,[]),none,[])])),op(join_1_1,join(left/2,right/2,arr_left_2_join_1_1_left,arr_right_2_join_1_1_right),sqlite([left/2,matched/1,right/2],owner(map_1)))]),wires([wire(left/2,join_1_1,delta),wire(right/2,join_1_1,delta),wire(join_1_1,map_1,delta),wire(map_1,matched/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

## Verification

- `swipl -q -g run_tests -t halt v6/prolog/compile/test/plunit_tests.pl`
- `v6/tsv2/scripts/sweep.sh`, then `git status --short -- v6/prolog/compile/out/ compile/out/`
- Emit every DD fixture twice and compare bytes.

## Staffing

Implementation: Codex, current worktree. Base: `f4ff55ed`. No open items.
