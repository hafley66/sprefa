---
created: 2026-08-20
updated: 2026-08-20
type: task
status: testing
priority: high
epic: write-verb-interface
labels: [compiler]
commits:
- hash: 845af307e
  summary: write verbs, the six-verb projection and the shared support ledger
- hash: a4b03b6ec
  summary: tsv2 write-verb interface, flag branches deleted
- hash: 4cf9ebaca
  summary: engine-rs WriteVerbs trait, flag branches deleted
- hash: 42673a70e
  summary: retraction battery, three arms, both doors
---

# define the six-verb write interface (arrive, stage, read_staged, recount, publish, clear) in program_data and both runtimes

## Description

Every transient write a tick makes is one of six verbs. The compiler names them
in the lowering projection; each runtime declares one interface with two
implementations, `per_rel` and `shared`, and the tick loop stops asking which
storage mode it is in.

| verb | contract | per_rel | shared |
| --- | --- | --- | --- |
| arrive(rel, rows, sign) | `lower.pl:6986` `relation_write_verbs/6`, `types.ts:352` `IWriteVerbs`, `write_verbs.rs:43` | `writeVerbs.ts:219`, `write_verbs.rs:171` | `writeVerbs.ts:267`, `write_verbs.rs:262` |
| stage(rel, rows) | same | per-rel `__frontier_<t>` insert | `__frontier` insert joining the head for `__id` |
| read_staged(rel) | same | EXISTS per rel's next frontier | one EXISTS on `__next_frontier` |
| recount(rule) | `lower.pl:7025` `rule_write_verbs/3` | no extra statement | the `__support_count` clear + per-rule writes |
| publish(rel) | same | the rel's boundary select | identical |
| clear(tick) | same | per-rel DELETE/INSERT trio | one DELETE/INSERT trio on the shared pair |

Strategy selection is metadata, resolved once per program: `writeVerbs.ts:307`
`write_verbs_for` memoizes on the relations array; `write_verbs.rs:79` reads
`shared_frontier` off the plan. The four `shared_frontier.is_some()` branches
that lived inside `prepare_tick`, `merge_next_into_current`,
`promote_frontiers` and the frontier stage builder are gone from both runtimes.

Joins stay compiler-produced (the plan's own Decisions list), so the `recount`
verb carries its seed SQL rather than a rebuild recipe, and `read_staged`
renders the SAME text in both modes, which is what the TEMP views buy.

## Acceptance Criteria

- [x] Six verbs named once, in the compiler and in both runtimes
- [x] `lowered_program_data/3` rows reference verbs plus the compiler-specialized SQL
- [x] One interface per runtime, I-prefixed in TS (`IWriteVerbs`), a trait in Rust
- [x] Two implementations each: per_rel and shared
- [x] No flag branch left inside a tick loop; the strategy object is chosen at program load
- [x] Flag off, every emitted program byte-identical to origin/main
- [x] COMPILE-TRACE untouched

## Implementation Notes

- Compiler: `v6/prolog/lower.pl:6951-7035` (the `write_verb/1` set,
  `lowered_program_data/2,3`, `relation_write_verbs/6`, `rule_write_verbs/3`).
  `refcountsql/15` became `/16`; the new field is `none` under per_rel and
  neither emitter renders a byte for it
  (`v6/prolog/emit_ts.pl:1408` `support_count_sql_field/2`,
  `v6/prolog/emit_rust.pl:337` `support_count_field/2` plus the `put_dict` at
  `emit_rust.pl:281`).
- TS: interface `v6/tsv2/runtime/types.ts:352`, with `WriteVerbStrategy:321`,
  `IDeltaEvent:325`, `IFrontierCopy:335`, `TickBoundary:342`,
  `IWriteSupportCountPlan:190`. Implementations and the SQL builders they share
  moved into `v6/tsv2/runtime/writeVerbs.ts` (a one-way dependency:
  `1_incremental.ts` imports it, never the reverse, so no ESM cycle).
- Rust: trait `v6/sprefa-engine-rs/src/write_verbs.rs:43`, impls at `:171` and
  `:262`, `write_verbs_for` at `:79`, plan struct
  `v6/sprefa-engine-rs/src/types.rs:553`.
- Byte identity: stage-1 compile sweep over the whole corpus rewrote every
  output; `git status v6/prolog/compile/out` reported 0 changed files.

## Tests Run

- `v6/prolog/compile/test/shared_frontier.test.pl` 15/15 (8 new units for the
  verb rows, the ledger DDL, and read_staged text identity across modes)
- `v6/tsv2/scripts/shared-frontier-gate.sh` 8/8 PASS
- `v6/sprefa-engine-rs/shared-frontier-gate.sh` 8/8 PASS
- `just plunit` 8 red, the same 8 names as origin/main 3993e44aa
- conformance 1 red, the known-red `nested_zero_column_child_is_one_row_per_parent`

## Decisions

### 2026-08-20T15:48:31Z · @write-verb-interface-lane

Joins stay compiler-produced: the recount verb carries its seed SQL (lower.pl:7025), the runtime never rebuilds a join. read_staged renders identical text in both modes, which is the TEMP-view contract in one assertion (compile/test/shared_frontier.test.pl, read_staged_text_is_identical_in_both_modes).
