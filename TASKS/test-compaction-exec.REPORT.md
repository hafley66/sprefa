# test-estate compaction, ranks 2-5

Branch `chore/test-estate-compaction`, base `3202379b8` (= `origin/main` at start).
Ranks 1, 6, 7, 8 out of scope.

## Contents

1. [What changed, per rank](#1-what-changed-per-rank)
2. [Gates, verbatim](#2-gates-verbatim)
3. [Corpus counts, per segment](#3-corpus-counts-per-segment)
4. [Rank 4: what was merged and what was skipped](#4-rank-4)
5. [Findings that are not this lane's work](#5-findings)
6. [Base drift](#6-base-drift)

---

## 1. What changed, per rank

| rank | commit | rows removed | verdict |
|---|---|---:|---|
| 3 | `4c8ff25aa` | 11 | done, all 7 KEEP fixtures annotated |
| 2 | `61f388e30` | 0 | done, leak-reason red NOT re-provable at this head |
| 5 | `1df65a27f` | 0 | KEEP, fold impossible, receipt written into the leg |
| 4 | `2887ebf7e` | 17 | 7 clusters merged, 30 surplus compiles left unmerged with reasons |

### Rank 3, the 11 unsupported duplicates

Every dropped fixture's `.reason` string in `manifest.json` is also reached by a
kept sibling, so no reason string, throw-site functor, or construct leaves the
corpus. Each KEEP now carries a `siblings folded 2026-08-20` line naming what
went into it and, where the dropped program differed in shape, what that shape
was.

| kept | dropped | file |
|---|---|---|
| `desugared_trace_equals_hand_written` | `trigger_marker_is_what_stops_backlog_replay`, `unmarked_chain_replays_to_late_subscriber`, `unmarked_first_stage_refires_on_late_watch`, `pipe_stage_costs_one_tick` | `temporal_pipe.pl` |
| `guard_stage_fires_on_negation_and_comparison` | `guard_stage_silent_when_muted`, `guard_stage_silent_below_threshold` | `temporal_pipe.pl` |
| `list_interned_set_relation_element_refused` | `option_of_interned_set_of_rel_is_refused` | `10_list_elements.pl` / `14_option_wrapper_walk.pl` |
| `ghcacher_host_program_term` | `ghcacher_json_normalization` | `2_hosts_wiring.pl` |
| `struct_column_type_unknown_rejected` | `struct_host_output_type_unknown_rejected` | `4_struct_values.pl` |
| `scan_is_a_named_unsupported` | `scan_is_a_named_unsupported_at_five_arguments` | `body_words.pl` |
| `json_array_groups_and_nests` | `json_array_keeps_bag_duplicates` | `json_arm.pl` |

Two of the eleven reach their shared reason through a DIFFERENT program, not
just different dead text, and the sibling comment says so in each case:

| dropped | the path only it drove |
|---|---|
| `option_of_interned_set_of_rel_is_refused` | the element spelled `option(list_interned_set(fighter_summary))`; its own header records that this spelling used to hide the rel element from `check_interned_set_rel_elements/1` and stop as `column_type_unknown` |
| `struct_host_output_type_unknown_rejected` | a decl-B host output column (`sh_decl scan_span`, `at: spann`); its own header carries a HOST-OUTPUT-SEAM fail-first receipt |

The plan's zero-information argument (`compile throws before the schedule runs`)
holds for the runner. It does not cover the two rows above, whose input PROGRAM
differs. They were dropped because the plan lists them and Chris approved the
deletion; the receipts now live in the kept fixtures' headers.

### Rank 2, memory-soak

`TSV2_SOAK_DURATION_S` default 100 -> 50, in `v6/tsv2/scripts/memory-soak.ts:255`
and `v6/tsv2/scripts/memory-soak.sh:32`. 1250 ticks, 51 samples, quarters of 12.
The four-quarter assertion shape and every finding name are untouched.

The brief asked for proof that the `__str` red is still visible at half length.
It is not provable at this head, for a reason upstream of this lane: see
section 5.

### Rank 5, roundtrip

The plan's own row already says the fold is impossible, and the evidence checks
out. `roundtrip.sh` G1 walks EVERY fixture in `conformance/fixtures/*.pl`;
`compile/scripts/text_door_receipt.pl:3-6` grades only the fixtures the TERM
door compiles. Every fixture in the `unsupported` bucket is therefore printed
and reparsed in G1 and nowhere else. The leg keeps its ~1s wall and now carries
that receipt in its own header, so the next compaction does not re-propose it.

---

## 2. Gates, verbatim

### conformance, before and after

```
# before, at 3202379b8
cd v6/prolog/conformance && swipl -g go -t halt go.pl
PASS  461
fail  nested_zero_column_child_is_one_row_per_parent
FAILURES  1

# after, at HEAD of chore/test-estate-compaction
PASS  433
fail  nested_zero_column_child_is_one_row_per_parent
FAILURES  1
```

461 -> 433 is 28 rows: 11 deleted (rank 3) and 17 folded (rank 4). The one
known-red is the same row, unchanged.

Per-rank, measured as each landed:

| after | PASS | delta | cause |
|---|---:|---:|---|
| base `3202379b8` | 461 | | |
| rank 3 | 450 | -11 | the 11 deletions |
| json_flex merges | 447 | -3 | 5 rows -> 2 |
| merge_family merge | 445 | -2 | 3 rows -> 1 |
| occurrence_identity merge | 443 | -2 | 3 rows -> 1 |
| json_patch merge | 435 | -8 | 9 rows -> 1 |
| ordered_aggregates merge | 434 | -1 | 2 rows -> 1 |
| recursive_enum merge | 433 | -1 | 2 rows -> 1 |

### plunit

```
cd v6/prolog/compile && swipl -q -l test/plunit_tests.pl -g run_tests -g halt
# before, at 3202379b8:  ERROR: [Thread main] 21 tests failed   (36.7s)
# after,  at branch HEAD: ERROR: [Thread main] 21 tests failed   (70.2s, under
#                         a concurrent sweep from another lane)
```

21 before, 21 after, and the 14 tests plunit names are the same 14 in both
runs (`diff` of the two name lists is empty). PR #379's receipt is not
regressed.

`catalog_plane_rail:level_plane_family_corpus_counts` at
`compile/test/plunit_tests.pl:1695` asserts hardcoded per-kind counts over the
whole fixture corpus. It was already red at `3202379b8` with stale numbers, so
this compaction did not turn it red; whoever re-measures it next should
re-measure against 434 rows, not 462.


### sweep

```
cd v6/tsv2 && bash scripts/sweep.sh
```

**NOT OBTAINED.** Stage 1 did not finish in this lane's window, before or
after, and the cause is machine contention rather than anything in the diff.

| run | last artifact written | next fixture, never written | wall before giving up |
|---|---|---|---:|
| before, at `3202379b8`, detached worktree | `recursive_enum_acyclic_tree_round_trips.ts` | `recursive_list_arg_parent_holds_child_node_values` (`18_recursive_list_arg.pl`) | ~18 min |
| after, at branch HEAD | `recursive_enum_tree_and_cycles_round_trip.ts` | the same fixture | ~9 min |

Two receipts say this is not a hang in the corpus and not a hang this lane
introduced:

1. `sample <pid> 3` on the stalled process puts every frame in
   `growLocalSpace___LD -> growStacks -> stack_realloc -> tmp_realloc ->
   _platform_memmove`. The process is copying its own local stack as it grows,
   not looping in a rule. `sweep.sh:41-47` already records the shape: one swipl
   process compiles the whole corpus and peaks near 2.4 GB.
2. That fixture compiles ALONE in 12ms:

```
cd v6/prolog && swipl -q -l compile.pl \
  -g "compile_fixture(recursive_list_arg_parent_holds_child_node_values,
      'conformance/fixtures/18_recursive_list_arg.pl', /tmp/one.ts)" -g halt
COMPILE-TRACE program=recursive_list_arg_parent_holds_child_node_values
  parse=0/0 plan=6/54664 lower=1/3920 boot=0/249 emit=4/39098 write=1/92
  total=12/98023
COMPILED
real 0m0.179s
```

Machine context for the whole window: four other swipl processes at ~100% CPU
each, `uptime` load average 9 to 15, three other lanes sweeping. The before-run
stalled at the same point on the UNMODIFIED corpus, which is the control.

What that leaves unmeasured, and what can be said without it:

| gate | status |
|---|---|
| `RUN total=/identical=/wrong=` before and after | not obtained |
| sweep wall before and after | not obtained; both runs were abandoned mid-stage-1 under load |
| rank 3's `jq ... \| sort -u \| wc -l` = 99 | DERIVED, not regenerated. All 18 fixtures in B3's table were read out of the committed manifest first: every one of the 11 drops carries a `.reason` string byte-identical to its kept sibling's. 110 unsupported rows carried 99 distinct reasons, so the 11 duplicate rows are exactly the 11 deleted, and a regen must print 110 - 11 = 99 rows and 99 distinct reasons |

`3993e44aa` on `origin/main` (sharded stage 1, digest-skip every stage) landed
during this lane and is aimed at exactly this cost. It is not in this branch's
base, so the sweep should be re-run on the merged base before landing.


---

## 3. Corpus counts, per segment

| segment | before (`3202379b8`) | after | command |
|---|---:|---:|---|
| fixture files | 66 | 65 | `ls v6/prolog/conformance/fixtures/*.pl \| wc -l` |
| `fixture/5` rows | 462 | 434 | `grep -h '^fixture(' .../fixtures/*.pl \| wc -l` |
| distinct programs (alpha-equivalence) | 400 | 394 | script, section 4 |
| surplus compiles per corpus pass | 62 | 40 | same |
| duplicate-program clusters | 38 | 29 | same |
| all-compiled clusters | 33 | 26 | same |
| conformance PASS | 461 | 433 | `swipl -g go -t halt go.pl` |
| conformance FAILURES | 1 | 1 | same |
| manifest rows | 461 | 433 (derived) | `jq length .../manifest.json` |
| manifest `compiled` | 351 | 334 (derived) | `jq -r '.[].bucket' ... \| sort \| uniq -c` |
| manifest `unsupported` | 110 | 99 (derived) | same |
| distinct `unsupported` reasons | 99 | 99 (derived) | rank 3's gate command |

The distinct-programs row is the point of rank 4: it falls by 6, and every one
of those 6 is a rank 3 deletion whose program differed from its sibling's while
its reason string did not. The 7 merges removed 17 fixture rows and zero
distinct programs.

---

## 4. Rank 4

Clustering method: each fixture's `prog(Decls, Rules)` term is read, copied,
`numbervars`ed and written canonically, so two fixtures cluster when their
programs are alpha-equivalent. That is a different measure from the plan's
whitespace-stripped text compare and it disagrees with the plan's numbers:

| statistic | plan (text compare) | this lane (alpha-equivalence) |
|---|---:|---:|
| distinct programs at base | 389 | 400 |
| duplicate-program clusters | 39 | 38 |
| surplus compiles | 73 | 62 |
| clusters of size >= 3 | 14 | 13 |
| all-compiled clusters | 35 | 33 |
| all-compiled surplus | 55 | 47 |

Merged, 7 clusters, 24 fixture rows -> 7:

| file | members | merged name |
|---|---:|---|
| `json_patch_fold.pl` + `json_null_is_none.pl` | 9 | `json_patch_fold_rfc7396_clauses` |
| `8_json_flex.pl` | 3 | `json_document_encoder_edges_round_trip` |
| `merge_family.pl` | 3 | `keyed_head_fold_across_two_rules` |
| `occurrence_identity.pl` | 3 | `log_occurrences_and_set_projection` |
| `8_json_flex.pl` | 2 | `json_literal_keys_survive_capture` |
| `9_ordered_aggregates.pl` | 2 | `json_object_groups_and_orders_keys` |
| `17_recursive_enum.pl` | 2 | `recursive_enum_tree_and_cycles_round_trip` |

`json_null_is_none.pl` held two members of the json_patch cluster and nothing
else, so the file is gone and its two defect receipts moved into the merged
header. Corpus files 66 -> 65.

Method for each merge, so a reader can check it rather than trust it:

- members that drove ONE key are re-keyed onto disjoint keys (json_patch: nine
  greek-letter sessions; merge_family: `cli`/`hub`/`api`; occurrence_identity:
  three stream ids with their own paths; recursive_enum: id ranges 1-3 and
  11-19). Disjoint keys are what make the concatenated ticks independent.
- `final` is the msorted union of the members' own finals.
- `deltas` is each member's own per-tick list at its offset in the merged
  schedule, `[]` where another member's key is quiescent.
- no expectation was copied from engine output. Every merged fixture passed
  conformance on its first run, which is the check that the derivation was
  right rather than that the engine was recorded.

### Skipped, with the reason each is not mechanical

Size >= 3, all-compiled, left as they are:

| cluster | why not merged |
|---|---|
| `15_string_split.pl` split_* (3) | the expectations pin `__gen__list_text_*__member` rows whose first column is an interned-dictionary surrogate id assigned by global mint order. Merging renumbers every id, so the merged text would no longer state what each split case proves, and `21_list_mint_order.pl` exists to pin that order |
| `20_parent_chain.pl` (3) | two of the three end in `throws(parent_cycle(...))`. `engine.pl:730-732` runs the program once and matches ONE thrown ball, so a run cannot observe two throws; anything scheduled after the first throw never runs |
| `timeless_rail.pl` over_baseline/new_file (3) | `new_file_no_exceeded_diag` asserts an ABSENCE (`diag/7` has no row for its file). Silence is a property of the whole rel, so a merged run cannot spell it without restating another member's rows |
| `timeless_rail.pl` unwrap_* (3) | same shape: `unwrap_below_budget_silent` asserts `final(diag/7, [])` |

Size 2, 22 clusters left (30 surplus compiles). Three of them are named in the
plan's own KEEP-because table and were not touched on that ground:
`relation_depth2_*`, `relation_depth3_*` (7 of 7 born with a fail-first
receipt, birth commit `4b0bc2793`) and `7_coalesce.pl` (ledger entry 40 names
that file as the site where a rail is MISSING). `one_vs_any.pl`'s pair is
KEEP-because too. The rest were left because the plan prices rank 4's benefit
as readability rather than wall (rank 1 captures the wall and is another lane's
work), and a size-2 merge trades two self-naming PASS rows for one that does
not name which half regressed.

---

## 5. Findings

Neither is this lane's work; both are receipts the coordinator asked for.

### F1. The serve door cannot boot any program at `3202379b8`

`memory-soak` dies before its first sample:

```
memory-soak: port=17571 duration=50s ticks=1250 arrival_interval=40ms ...
Error: POST /program -> 400 {"error":"ir_version_mismatch: program main was
emitted at ir_version none and this runtime interprets 1"}
```

Identical at `TSV2_SOAK_DURATION_S=100`, so it predates the rank 2 edit.

| receipt | where |
|---|---|
| the check | `v6/tsv2/runtime/irVersion.ts:6` `RUNTIME_IR_VERSION = 1` |
| the stamp that is gone | `65607a8d5` removed `ir_version(1)` from `v6/prolog/emit_ts.pl` and `v6/prolog/emit_rust.pl` |
| grep proving it | `ir_version` has zero hits under `v6/prolog/**/*.pl` |
| freshly emitted module missing it | `v6/tsv2/gen_served/<hash>.ts`, written by this run, contains no `ir_version` |

Same root cause as the pre-commit dl6 comment-budget rail the brief warned
about (`SPREFA_COMMENT_RAIL_DL6=0` was used for every commit on this branch and
is noted in each). The blast radius is wider than the rail: every serve-door
leg is red at this head for this reason.

Consequence for rank 2: `.github/CI-KNOWN-RED.md`'s memory-soak row records
`FAIL sqlite_page_count_flat` from the `__str` dictionary never releasing. That
finding is unreachable today, so the leg's red reason MOVED and the allowlist
row is stale. The assertion shape was preserved rather than re-proved.

### F2. Corpus rows the committed manifest still disagrees with

The plan's section 2 already recorded that `manifest.json` was a commit behind
the fixture files. Re-measured at `3202379b8`, unchanged: 462 `fixture/5` rows
against 461 manifest rows, with `option_in_key_column_normalizes` and
`nested_zero_column_child_is_one_row_per_parent` carrying no manifest row and
`option_in_key_column_is_refused` naming a fixture that no longer exists.
---

## 6. Base drift

The branch is cut at `3202379b8`, the sha the brief named. While this lane ran,
`origin/main` advanced 7 commits:

```
3993e44aa perf(sweep): shard stage 1, digest-skip all stages, snapshot oracle,
          timings report (#382)
ecf6b84fe obs(dl6): library(debug) topics per phase, print_message failure
          diagnosis (#381)
b4ac34c64 boop-start: node_modules skip keyed on pnpm-lock digest
54b3b5be0 issues: sweep-timings-report + plunit-jobs cards
918e74ad6 rulings: oracle demoted to snapshot minter; sweeps diff frozen
          snapshots (user 2026-08-20)
31a972834 issues: write-verb-interface epic + 4 cards
759678776 fix(compile): a program carrying a query reaches the plan phase again
```

No file this lane touched is touched by any of them, so the merge is clean.
Two of them matter to whoever lands this:

| commit | why it matters here |
|---|---|
| `918e74ad6` | the oracle is now a SNAPSHOT MINTER and sweeps diff frozen snapshots. The 11 deleted and 17 folded fixtures had tracked `out/<name>.oracle*.jsonl` snapshots; the 7 merged fixtures need theirs minted. This lane's sweep ran on the base's sweep implementation, so the snapshot set should be re-minted on the merged base |
| `3993e44aa` | stage 1 is now sharded and every stage digest-skips, so the wall figures below are from the pre-shard sweep and are not comparable to a post-merge run |

