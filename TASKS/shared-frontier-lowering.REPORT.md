# shared-frontier-lowering REPORT

Plan steps 1-4 of plans/2026-08-19-shared-sqlite-frontier.md behind
`frontier(shared)` on `compile_dl6/3`. Default `per_rel`; absent option =
byte-identical output.

Worktree note: the brief named ~/projects/sprefa-worktrees/shared-frontier-lowering;
the harness refused git operations outside this agent's own isolated worktree
(`.claude/worktrees/agent-ab5e5d2292e606d9d`), so the branch here is
`feature/shared-frontier-fable` from the same base `942cf1443`.

## TOC

1. [Shape](#shape)
2. [Commits](#commits)
3. [Gates](#gates)
4. [Statement counts](#statement-counts)
5. [Not done](#not-done)

## Shape

| piece | where | what |
| --- | --- | --- |
| option | `compile.pl` `frontier_option/2`, `with_frontier_mode/2` wrap of the phases | `frontier(shared)` / `frontier(per_rel)` / absent |
| shared DDL | `lower.pl` `shared_frontier_ddl/1` | `__frontier` + `__next_frontier`, plain heaps, index `(relation_id, _phase)`; row identity = durable `__id` |
| read compat | `lower.pl` `shared_frontier_view_ddl/3` | per-rel `__frontier_<t>` / `__next_frontier_<t>` become TEMP VIEWS joining the shared table to the typed table, so every compiled read keeps its text |
| compiled writes | `lower.pl` `stage_frontier_sqls/9` | refcount-dance stage inserts write `(relation_id, ?, n.rowid-1, h.__id)` joining `__new_<t>` to the head |
| runtime writes | `1_incremental.ts` `shared_frontier_stage_statement`, `incremental.rs` same name | one batched INSERT per rel per stage, row_id resolved by joining the typed table on all columns with `IS` |
| clears/promote | both runtimes | `prepare_tick`: one `DELETE __next_frontier`; `promote_frontiers` and `merge_next_into_current`: one statement over the shared pair instead of 3 per rel |
| plan metadata | both emitters | relation entries gain `shared_frontier: { relation_id: N }` only under the flag |
| projection | `lower.pl` `lowered_program_data/2` | `program_data(relation_data/6 rows, rule_data/4 rows, [], [], [])` |
| guard | `lower.pl` `shared_frontier_todo/3` | loud `unsupported_construct(frontier_shared_todo(Reason))` for edge_rules, retention, aggregate_head, recursion, departure, non-set rels, bytes columns, tick, hosts |

## Commits

| sha | what |
| --- | --- |
| 4cecf2e11 | compiler option, shared DDL + views, stage SQL, emitter fields |
| 5ba330709 | tsv2 runtime branches |
| 2b769cdb1 | engine-rs runtime branches |
| 655640788 | parity gates both doors |
| 8add7ec11 | plunit block (7 units) + cargo parity test |

## Gates

### TS parity gate (scripts/shared-frontier-gate.sh)

```
PASS sf_arrivals ticks=true final=true search=true statements per_rel=60 shared=48 pinned=true
PASS sf_keyed_replace ticks=true final=true search=true statements per_rel=37 shared=37 pinned=true
PASS sf_join ticks=true final=true search=true statements per_rel=61 shared=45 pinned=true
PASS sf_guard ticks=true final=true search=true statements per_rel=46 shared=38 pinned=true
```

### Rust parity gate (sprefa-engine-rs/shared-frontier-gate.sh)

```
PASS sf_arrivals rust ticks identical (3 lines)
PASS sf_guard rust ticks identical (2 lines)
PASS sf_join rust ticks identical (2 lines)
PASS sf_keyed_replace rust ticks identical (3 lines)
```

### cargo test --test shared_frontier

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
```

### plunit (full battery, cd v6 && just plunit)

```
% [930/930] relation_id_acces.._a_different_target .. passed (0.000 sec)
ERROR: [Thread main] 7 tests failed
```

The 7 failures are the pre-fix known-red set, none in the new block:
`subscribe_cone:golden_flex_cone_invariants`, `catalog_plane_rail (family_corpus_counts)`,
`module_path_decls (value_is_not_rewritten)`, `rel_zero_arity (still_has_no_storage)`,
3x `json_merge_patch`. Baseline in `.github/CI-KNOWN-RED.md` (PR #373) also
counted 7. The new `shared_frontier` block: 7/7 passed.

### conformance (cd v6/prolog/conformance && swipl -g go -t halt go.pl)

```
PASS  tightened_baseline_catches_regrowth
FAILURES  1
```

Same single failure as the known-red baseline
(`nested_zero_column_child_is_one_row_per_parent`, group A).

### byte identity, flag off

- tiny probe: compiled `sf`-shape program with the branch stashed vs applied,
  `diff` empty (`BYTE-IDENTICAL` receipt in commit 4cecf2e11's message).
- corpus: PENDING sweep receipts below.

### sweep.sh (3 runs, flag off)

PENDING - run 1 in flight at save; runs recorded here when complete, plus
`git diff --stat v6/prolog/compile/out` after each (empty = corpus-wide byte
identity).

### grade.sh

PENDING - queued after sweep to avoid measuring under load.

## Statement counts

Per whole gate run (boot + 2-3 ticks), counted by a wrapping runner, one count
per statement including `;`-joined legs:

| fixture | per_rel | shared | delta |
| --- | --- | --- | --- |
| sf_arrivals (2 rels, no rules, 3 ticks incl replace + dels) | 60 | 48 | -20% |
| sf_keyed_replace (1 rel, no rules, 3 ticks) | 37 | 37 | 0 |
| sf_join (3 rels, 1 join rule, 2 ticks) | 61 | 45 | -26% |
| sf_guard (2 rels, 1 guard rule, 2 ticks) | 46 | 38 | -17% |

EXPLAIN QUERY PLAN on every shared view read: SEARCH via
`__frontier_rel_phase (relation_id=?)`, no SCAN of `__frontier`.

## Not done

- Step 5 (retraction/support parity at the SHARED support_count table):
  the per-rel `__support_next_`/`__new_`/`__delta_` scratch stays per-rel in
  this arc; retraction through rules still runs the existing recount dance,
  which the parity fixtures exercise only through keyed replacement on
  rule-free rels. Step 5 needs: the shared `support_count(relation_id,
  row_id, rule_id, count)` table, the recount rewritten against it, and a
  retraction battery fixture set.
- Oracle leg for the 4 new fixtures: the per_rel arm IS the shipping path the
  corpus oracle grades; a direct oracle-vs-shared sweep over the corpus needs
  the whole-corpus shared compile and is step 6 (default flip) territory.
- `lowered_program_data/2` fills Boot/Hosts/Queries with `[]` and
  `rule_data` SQL with `pending`; the emitters do not read it yet (plan step
  1 says "beside", step 6 switches).
