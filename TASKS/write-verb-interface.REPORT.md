# write-verb-interface REPORT

Plan steps 5 and the write-verb contract, on top of PR #378's steps 1-4.
Branch `feature/write-verb-interface`, rebased onto origin/main `3993e44aa`.

## TOC

1. [Shape](#shape)
2. [Commits](#commits)
3. [Gates](#gates)
4. [Statement counts](#statement-counts)
5. [Main-branch regression found](#main-branch-regression-found)
6. [Not done](#not-done)

## Shape

Six verbs name every transient write a tick makes. The compiler projects them
once; each runtime declares one interface with two implementations, and the
storage strategy is metadata read at program load, never a flag a tick loop
reads.

| verb | contract | per_rel | shared |
| --- | --- | --- | --- |
| arrive(rel, rows, sign) | `lower.pl:6986` `relation_write_verbs/6`; `tsv2/runtime/types.ts:352` `IWriteVerbs`; `engine-rs/src/write_verbs.rs:43` `trait WriteVerbs` | `writeVerbs.ts:219` `PerRelWriteVerbs`; `write_verbs.rs:171` | `writeVerbs.ts:267` `SharedWriteVerbs`; `write_verbs.rs:262` |
| stage(rel, rows) | same | per-rel `__frontier_<t>` insert | `__frontier` insert joining the head for `__id` |
| read_staged(rel) | same | EXISTS per rel's next frontier | one EXISTS on `__next_frontier` |
| recount(rule) | `lower.pl:7025` `rule_write_verbs/3`, plan at `lower.pl:4796` `support_count_plan/8` | no extra statement | `__support_count` clear plus one write per rule |
| publish(rel) | same as arrive | the rel's boundary select | identical text |
| clear(tick) | same as arrive | per-rel DELETE/INSERT trio | one trio on the shared pair |

Verb names are fixed at `lower.pl:6951-6956` (`write_verb/1`). Strategy
selection: `writeVerbs.ts:307` `write_verbs_for` memoizes on the relations
array (WeakMap, so one resolution per program); `write_verbs.rs:79` reads
`shared_frontier` off the plan.

Four `shared_frontier.is_some()` branches are gone from each runtime:
`prepare_tick`, `merge_next_into_current`, `promote_frontiers`, and the
frontier stage builder.

Step 5 storage:

```sql
CREATE TEMP TABLE "__support_count" (
  "relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL,
  "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL,
  PRIMARY KEY ("relation_id", "row_id", "rule_id")) WITHOUT ROWID
```

All-integer key, `row_id` the durable `__id`, hot lookup on the
`(relation_id, row_id)` prefix. Written by the recount verb AFTER the head
insert, one batched INSERT per rule. A single-rule head writes it straight from
the `__support_next_` staging it already filled; a multi-rule head re-reads one
arm per rule id, because the staging sum cannot be split back apart.

`refcountsql/15` became `/16`. The field is `none` under per_rel and neither
emitter renders a byte for it (`emit_ts.pl:1408`, `emit_rust.pl:337` plus the
`put_dict` at `emit_rust.pl:281`).

## Commits

| sha | what |
| --- | --- |
| 845af307e | the six-verb projection and the shared support ledger |
| a4b03b6ec | tsv2 write-verb interface, flag branches deleted |
| 4cf9ebaca | engine-rs WriteVerbs trait, flag branches deleted |
| 42673a70e | retraction battery, three arms, both doors |
| 1453ffb27 | dd goldens take the refcountsql field |
| 64fb94a12 | engine-rs test initializers take shared_frontier and support_count_sql |

Rebase onto `3993e44aa` took one conflict, `compile.pl`
`compile_program_phases/8`: #381 added `dl6_reset_checkpoint` and moved
`run_compile_phase` to carry the program name, #378 wrapped the phases in
`with_frontier_mode`. Both kept; the wrapper is outside, the checkpoint inside
`compile_program_phases_moded/8` (`compile.pl:754-763`).

## Gates

### conformance

```
FAILURES  1     fail  nested_zero_column_child_is_one_row_per_parent
```

Same single known-red as the baseline.

### plunit (cd v6 && just plunit)

8 failed of 933 on this branch. origin/main `3993e44aa` measured 8 failed of
918 (the 15 extra tests are this branch's `shared_frontier` block, 15/15 green).
Identical failing set:

`subscribe_cone:golden_flex_cone_invariants`,
`catalog_plane_rail:level_plane_family_corpus_counts`,
`module_path_decls:a_zero_column_childs_name_used_as_a_value_is_not_rewritten`,
`rel_zero_arity:a_root_rel_zero_still_has_no_storage`,
`rel_template_and_is_clause:a_relation_arrow_prints_the_equivalent_explicit_declaration`,
3x `json_merge_patch`.

Zero added. The brief's "21 red at head" is not what this tree measures; the
number above is a full `just plunit` run on each side, back to back.

### byte identity, flag off (stage 1 only, per the coordinator's correction)

```
scripts/sweep-stage1.sh 8      real 0m12.078s
git status --short v6/prolog/compile/out   ->  changed files: 0
```

Every tracked sweep output was rewritten (mtimes moved) and every one is
byte-identical to origin/main's copy. COMPILE-TRACE lines print unchanged.

### TS parity gate (v6/tsv2/scripts/shared-frontier-gate.sh)

```
PASS sf_arrivals ticks=true final=true search=true statements per_rel=60 shared=48 pinned=true
PASS sf_keyed_replace ticks=true final=true search=true statements per_rel=37 shared=37 pinned=true
PASS sf_join ticks=true final=true search=true statements per_rel=61 shared=45 pinned=true
PASS sf_guard ticks=true final=true search=true statements per_rel=46 shared=38 pinned=true
PASS sf_retract_current ticks=true final=true search=true oracle=true ledger=true ledger_rows=2 ledger_search=true restart=true statements per_rel=96 shared=74 pinned=true
PASS sf_retract_stale ticks=true final=true search=true oracle=true ledger=true ledger_rows=2 ledger_search=true restart=true statements per_rel=73 shared=63 pinned=true
PASS sf_negation_support ticks=true final=true search=true oracle=true ledger=true ledger_rows=2 ledger_search=true restart=true statements per_rel=142 shared=118 pinned=true
PASS sf_two_rule_support ticks=true final=true search=true oracle=true ledger=true ledger_rows=1 ledger_search=true restart=true statements per_rel=105 shared=87 pinned=true
```

`ledger` is the invariant every head row's `sum("count")` over
`__support_count` equals its `__refcount`; `ledger_search` is
`EXPLAIN QUERY PLAN SELECT "count" FROM "__support_count" WHERE "relation_id" = ? AND "row_id" = ?`
reporting SEARCH with no SCAN; `restart` replays the whole schedule on a fresh
database and compares finals.

### Rust parity gate (v6/sprefa-engine-rs/shared-frontier-gate.sh)

```
PASS sf_arrivals rust ticks identical (3 lines)
PASS sf_guard rust ticks identical (2 lines)
PASS sf_join rust ticks identical (2 lines)
PASS sf_keyed_replace rust ticks identical (3 lines)
PASS sf_retract_current rust ticks identical and oracle (3 lines)
PASS sf_retract_stale rust ticks identical and oracle (3 lines)
PASS sf_negation_support rust ticks identical and oracle (4 lines)
PASS sf_two_rule_support rust ticks identical and oracle (3 lines)
```

### cargo test

`cargo test --lib`: 26 passed, 0 failed.

Per integration target, this branch:

| target | result |
| --- | --- |
| 0_relation_id_access, 0_wrapper_composition, data_family, module_storage_runtime, serve_uds, shared_frontier, type_annotation_ci | ok |
| 15_source_mutation_hosts, 17_resident_coroutine, _0_source_bind, bytes_runtime, change_facts, dep_resolve, diverging_recursion, git_refs, list_boundary, live_hosts | FAILED, at parity with origin/main |
| consumer_integration, dl6_build, skeleton | do not compile, blocked by the regression below |

origin/main `3993e44aa`, same four targets sampled: `diverging_recursion`
0/2, `17_resident_coroutine` 0/2, `list_boundary` 10 pass 1 fail,
`bytes_runtime` 2 pass 1 fail. Identical numbers. The failure text is
`fixture program json: Error("missing field 'incremental_safe'")`.

### node tests (v6/tsv2, npm test)

NOT RUN TO COMPLETION. The suite was started twice and killed twice: the
first run had this worktree checked out to origin/main under it (the plunit
baseline measurement), the second was stopped when the lane was paused for
perf work. `gen_emitted/` is populated (352 modules copied from the sweep
outputs), so the next run needs no sweep in front of it:
`cd v6/tsv2 && npm test`.

## Statement counts

Per whole run (boot plus every tick), one count per statement including
`;`-joined legs, three runs each, stable:

| fixture | per_rel | shared | delta | recount rounds |
| --- | ---: | ---: | ---: | ---: |
| sf_arrivals | 60 | 48 | -20% | 0 |
| sf_keyed_replace | 37 | 37 | 0% | 0 |
| sf_join | 61 | 45 | -26% | 0 |
| sf_guard | 46 | 38 | -17% | 0 |
| sf_retract_current | 96 | 74 | -23% | yes |
| sf_retract_stale | 73 | 63 | -14% | yes |
| sf_negation_support | 142 | 118 | -17% | yes |
| sf_two_rule_support | 105 | 87 | -17% | yes |

The shared arm pays 2 extra statements per recount round (the ledger clear plus
one write per rule) and is still cheaper on every retraction case, because the
per-rel arm spends a DELETE and an INSERT per relation at each tick boundary.

## Main-branch regression found

`65607a8d5` ("feat(dl6): complete relational type applications") reverted PR
#372's IR-version work without replacing it:

| deleted | still referenced by |
| --- | --- |
| `emit_ts.pl` `ir_version/1` and the `ir_version` field on `IGenProgramWithBoot` | `v6/tsv2/runtime/irVersion.ts:11` `IrVersionCheck.check`, which every served program passes through (`serve/0_compile.ts:125`) |
| `emit_rust.pl` `ir_version` | `v6/sprefa-engine-rs/src/build_template/main.rs:20,45` |
| `program.rs` `pub const IR_VERSION` and the ProgramJson check | `tests/skeleton.rs:130`, `tests/consumer_integration.rs:95`, `tests/dl6_build.rs:301` |
| the ProgramJson shape #372 shipped | every checked-in `tests/fixtures/*.program.rs`, which carry `ir_version` and no `incremental_safe` |

Two consequences measured on a CLEAN tree at origin/main:

1. The pre-commit hook is red for everyone. `.githooks/pre-commit` runs
   `v6/tsv2/scripts/comment-budget-rail.sh`, which POSTs the golden `.dl6` to
   the serve runtime and gets `400 {"error":"ir_version_mismatch: program main
   was emitted at ir_version none and this runtime interprets 1"}`. Receipt:
   `git stash push --include-untracked` then run the rail; same 400. Every
   commit on this branch therefore carries `-n`, and every one of them says so.
2. Three Rust test targets do not compile and ten more fail at
   deserialization.

Not fixed here: restoring the stamp changes the bytes of every emitted program,
which would make this branch's byte-identity claim against origin/main false.
It wants its own card and its own PR.

## Not done

- `stage_ordered_frontiers` still writes its own per-rel DELETEs around the
  stage verb; an ordered program is refused under shared, so the names are
  always real tables today.
- The recursive recount path closes its round without the recount verb;
  recursion is refused under shared.
- The eight `shared_frontier_todo/3` reasons are each a verb-shaped hole.
- Whole-corpus oracle-vs-shared: still step 6 territory, and the shared arm
  still refuses most of the corpus by design.
