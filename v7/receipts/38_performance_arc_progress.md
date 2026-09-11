# 38. Performance arc progress audit

Date: 2026-09-11. Read-only native Astra audit of `main` and its concurrent dirty
checkout. Initial audited HEAD:
`c54c0b07d014735066b1cb98b801ad13f65d6587`.
Acceptance-update HEAD: `00f6f80d46ff0eb55d0a4cfdcc3c84f4cf96d0e4`.
The user accepted the parser patch after classifying the measured 1.06 ms wall
difference as noise. Its source, test, and revised receipt 36 are now committed
in `62a25b380`; the checker change subsequently committed in `00f6f80d4`.

Evidence: verified `git log --reverse --name-only 32d4de862^..HEAD -- v7`,
receipts 23 through 36 read completely, supporting receipts 14, 15, 18, 19,
20, current status/diffs, compiler/evaluator/checker/parser/tracer seams,
the CI workflow, benchmark checks, and generated profile summary. Receipt 37
appeared during final inspection and was also read completely. No SWI
process, build, or test ran in this audit. Runtime numbers below are explicitly
attributed prior measurements, not newly executed validation. Only this receipt
was written. No commit or push.

## Committed timeline

Nearest-shadow numbers are cold trace-off benchmark inferences unless marked
otherwise. A recorded before/after pair belongs to its own receipt harness.
Instrumentation changes and different canonical enclosing terms prevent treating
every row as one uninterrupted experimental series.

| Commit | Committed change | Recorded inference/output result | Evidence |
| --- | --- | --- | --- |
| `32d4de862` | Replace materialized ordering closure with grounded `int_lt` | 14,581,429 -> 6,369,740; 14,586 -> 790 rows | receipt 14 |
| `bab0c80e5` | Five additional grounded integer comparisons | 6,388,944; nearest 790 -> 810 and partial 890 -> 910 rows, exactly 20 metadata rows per fixture | receipt 15 |
| `5d37a0a28` | Read-only cold comptime attribution | Trace-steps capture 6,448,850; identified repeated stratification joins; no source change | receipt 18 |
| `af240bb3f` | Dependency-indexed stratification worklist | 6,388,944 -> 3,370,854; same 810 rows; partial 16,196,538 -> 8,013,214, same 910 rows | receipt 19 |
| `d9e8b6dbc` | Compile-scoped exact-rule stratification memo | 3,370,854 -> 3,133,836; warm 2,148 -> 2,169; same output | receipt 20 |
| `31b5aed7d`, `681b2f5be` | Structured debug trace, then compile-scope instrumentation gating | Reporting/lifetime implementation; no isolated inference saving claimed here | receipt 21 and verified changed paths |
| `c86c7fcd4`, `0d8267a20`, `0f018320a`, `312563dbd` | Flamechart generation, path normalization, deterministic profile checks, renderer execution fix | Profiled baseline later reported as 3,753,882; this includes profiling overhead | receipts 22/23 and verified changed paths |
| `2a92096b7` | Separate wall artifact and visible timing labels | Same deterministic structural/inference contracts; 25 profile test cases reported | receipt 23 |
| `3405509c6` | Share immutable lower rows within each evaluate scope | Capture pair 3,136,313 -> 2,978,072; live after gate 2,978,066; same 810 rows | receipt 24 |
| `2e6df4724` | Bound current-stratum collection roots, then union lower snapshot | 2,978,066 -> 2,927,019; same 810 rows | receipts 25/26 |
| `00c14d180` | Full SWI gotcha audit | Read-only receipt, no compiler change | receipt 27 |
| `41c5e7867` | Owner-count assoc for dense pending-edge checks | 2,927,019 -> 2,408,708; same 810 rows | receipt 28 |
| `fb7f76894` | Empty-enqueue fast path in stratification queue | 2,408,708 -> 2,403,099; same 810 rows | receipt 29 |
| `53778e948` | Indexed demand-cone expansion | 2,403,099 -> 2,040,156; same 810 rows | receipts 30/31 |
| `70b250b3d` | Build demand-cone indexes once per evaluate call | 2,040,156 -> 2,010,858; same 810 rows | receipts 32/33 |
| `8f76b9ace` | Current-head attribution | Independent unwrapped harness 2,010,865; same 810 rows; receipt only | receipt 34 |
| `c54c0b07d` | Parser attribution | 378,841 inclusive parser inferences across three compiler calls; receipt only | receipt 35 |
| `62a25b380` | Accepted parser open-tail source-row assembly | Prelude parser 344,089 -> 328,876; exact parser/compiler output parity; user accepts wall variation | receipt 36 |
| `00f6f80d4` | Structured checker debug case commits to one solution | First/second check 17,737/145,989; combined parser/checker live gate 1,994,058, 810 rows | receipt 37 |

The intervening `dec79ba0e`, `94965ce7a`, `c398808dd`, and `4cb1542e6`
change only the architecture inventory receipt 16. Their verified file lists
contain no compiler source.

From comparison-family gate through the pre-parser demand-cone implementation,
6,388,944 -> 2,010,858 is **68.526% fewer cold inferences**, with the same
810-row checkpoint. The earlier 14,586 -> 790 migration deliberately changed
published ordering rows and must remain separate from output-preserving work.

The recorded pre-parser committed partial gate is 4,493,644 cold inferences with
`DL7_TRACE=collect`, 910 rows, eight closure rounds, and empty diagnostics
(receipt 32). Earlier partial measurements mix trace-off capture and collect
harnesses. Receipt 25's 6,423,729 -> 6,557,302 comparison, for example, crosses
those boundaries and does not isolate a closure-collector regression.

## Work state and approval boundaries

| State | Exact scope | Audit disposition |
| --- | --- | --- |
| Committed | Comparison family, stratification worklist/memo, debug/profile tools, shared lower store, bound collection, dense counts, both demand-cone patches | Verified by commit file inventories, not historical receipt phrases such as “no commit” |
| Accepted and committed | `v7/src/0_reader/0_parser.pl`, `v7/test/0_reader.test.pl`, receipt 36 | User accepts deterministic inference reduction and exact output; classifies the wall difference as noise. Complete patch restored, reader 13/13 passed per parent, committed in `62a25b380` |
| Approved, committed, locally tested | One cut in `v7/src/2_comptime/1_checker.pl:105`; additions to `v7/test/1_entrypoints.test.pl` and `v7/test/3_compiler_trace.test.pl`; receipt 37 | Committed in `00f6f80d4`. Receipt 37 reports focused entrypoint 5/5, trace 5/5, comparator 4/4; performance observations measure parser and checker changes together |
| Blocked by checkpoint | Live partial benchmark | Actual 910 versus expected 15,562 causes exit 1; cannot report a passing live gate |
| Historical unresolved behavior | Partial forwarding and shared closed-view entrypoint cases; `5_curry` source refreeze limit | Receipts 19/20 and 26 document failures. They were not rerun here; current failure status is unverified |
| Unrelated dirty work, preserved | `scripts/machine-guard.sh`, `v6/sprefa-extract/src/types.rs`, `.recovery/` | Outside compiler audit ownership |
| Generated untracked output | `v7/out/` | Existing profile summary is stale; no regeneration performed |

Receipts 19 and 20 explicitly record approved kernel implementation. Receipt 15
records approved comparison metadata. Existing committed evaluator changes are
reviewed as landed code. This audit grants no additional authorization to alter
binding, rule eligibility, negation, aggregate snapshots, clocks, or phase
semantics. No new kernel change is needed for the two follow-ups below.

## Accepted parser patch: measurements and acceptance

Current `read_dl7(+Path,+Text,-Forms,-SourceRows,-Diagnostics)` remains exported
with the same result shape. Private result terms gain an open-tail row pair.
`continue_top_forms_dl/3` and `continue_form_items_dl/5` join tails by
unification; `close_top_rows/2` closes the successful final tail. The existing
`read_top_forms/5` compatibility wrapper remains. A repository Prolog search
found these changed helper names only inside the parser.

Receipt 36 supplies these two experiments:

| Prelude parser metric | Baseline | Candidate | Decision evidence |
| --- | ---: | ---: | --- |
| Fresh-process inferences | 344,089 | 328,876 | -15,213, or -4.42% |
| Nine-run wall median | 29.80 ms | 30.27 ms | +0.47 ms, or +1.58% |
| Ten interleaved-pair wall medians | 29.03 ms | 30.09 ms | +1.06 ms, or +3.65%; candidate slower in 9 of 10 listed pairs |
| Source rows | 3,929 | 3,929 | Unchanged |

The initial review applied a strict reject-on-wall-regression gate. The user
subsequently superseded that classification, treated the 29.03 -> 30.09 ms
variation as noise, and explicitly accepted the patch's deterministic inference
reduction and exact-output parity. The measurements remain unchanged.
Statistical significance was not established. Receipt 36 records the user's
acceptance; commit `62a25b380` contains the complete patch and reader test.

Receipt 36 reports byte-identical parser manifests across 16 inputs, unchanged
canonical compiler hashes for nearest/partial, reader 13/13, syntax expander
4/4, and comparator 17/17. The added reader test pins node order and occurrence
count, but leaves complete SourceRows terms unconstrained in its final pattern.
Its name therefore exceeds its exact automated protection for source spans and
origins; those are covered by the reported manual manifest comparison.

Two attribution limits remain: the saving is measured for the prelude parser,
not a fresh whole-compiler candidate pair, and additional open-tail fields/
unifications are a source-level explanation for overhead rather than isolated
CPU attribution. Receipt 35 says child rows precede the enclosing row in one
paragraph; actual `finish_form/5` at parser line 136 and receipt 36 establish
enclosing-row-first order. Preserve that actual order.

## Current architectural flow

```text
compile_dl7(Path, Rows, Runtime, Diagnostics)                 2_compiler.pl:70
  compile scope: trace state + exact-rule stratification memo
  read prelude/macrotime/program text, content-keyed compiler cache
  parse -> reader expansion -> syntax reification
  bootstrap checked macrotime program -> syntax expansion
  lower modules -> bind/name/type/mode/rule checks
  comptime rounds                                           2_compiler.pl:1156
    check complete resolved rules
    authored seeds + previous frozen edge/request snapshot
    evaluate(Rules, Seeds, Closure, Diagnostics)             0_evaluator.pl:70
      evaluate-owned lower store; dependency/strata result
      one static demand-cone index pair                     :125
      per stratum                                          :139
        select eligible positive plain-rule cone
        aggregate/negative reads use completed lower snapshot
        fresh EvaluationId: rules, seeds, variant proof tables
        assert only new lower rows into evaluate-owned store
        prove bound current roots; union requests + lower rows
        abolish this stratum's proof tables, erase owned clauses
      close lower store
    freeze generated edges/requests/rules for next round
  final re-lower/re-check; source refreeze when required      2_compiler.pl:740
  published compiler rows + checked runtime program
```

Indexes and lower rows share one evaluate lifetime. Proof identities and
tables retain per-stratum lifetime. Generated rules trigger another round with
new indexes. None of the committed physical optimizations establishes reuse of
proof tables across rounds or removes final rule checking.

## Remaining measured costs

Committed compiler source is unchanged between receipt 34's `70b250b3d` and
this audit's `c54c0b07d`; intervening commits add receipts only. Thus receipt
34 is the pre-parser committed-source attribution. The accepted parser commit
`62a25b380` follows it, so the table is a historical attribution baseline and
does not claim a fresh parser/checker breakdown of the combined current code.
Each row below was independently instrumented, with its own compile denominator.

| Inclusive target | Calls / outer calls | Inferences | Own-pass compile share |
| --- | ---: | ---: | ---: |
| evaluator `evaluate/4` | 3 / 3 | 489,188 | 24.33% |
| parser `read_dl7/5` | 3 / 3 | 378,841 | 18.84% |
| checker `check_resolved_rules/5` | 5 / 5 | 279,671 | 13.91% |
| checker `check_datalog/4` | 4 / 4 | 276,172 | 13.73% |
| lowerer `lower_datalog_mode/6` | 6 / 6 | 178,388 | 8.87% |
| evaluator `collect_closure/4` | 15 / 15 | 173,421 | 8.62% |
| evaluator `proves/2` | 3,806 / 167 | 170,457 | 8.10% |
| checker `resolve_rules/8` | 268 / 4 | 160,632 | 7.98% |
| syntax `reify_syntax/4` | 4 / 4 | 142,676 | 7.09% |
| checker `check_goal_sequence_failures/7` | 2,224 / 648 | 138,573 | 6.78% |

Do not sum these inclusive rows. Collection/proofs are nested under evaluation;
resolution is nested under checking; goal checks are shared across checker entry
points. Installation is 110,881 within evaluation, and shared lower-row install
100,336 is within installation. Demand-cone selection is now 32,697, static
index construction 6,280, checker bind diagnostics 31,052, dense checks 16,250.
These replace receipt 27's old leading hotspots. Their parent/child costs also
must not be added.

Parser input is chiefly the 30,829-byte prelude, not the 116-byte fixture.
`lower_expression/7` cost 1,748 on mostly literal/variable arguments; all
executable lowering cost 116,662. Syntax materialization and TSI acceptance
had zero calls on this fixture, although receipt 27 demonstrates synthetic
quadratic growth for both. Foreign `sort/2`, `msort/2`, and `keysort/2` are not
priced by per-element inference counts, so their clamped zero readings do not
establish zero CPU cost.

The pre-checker-cut trace phase intervals overlap: receipt 34 measured main
check at 1,201,235 including subsequent comptime, while a temporary debug-helper
commit produced check 145,992 and comptime 1,055,144 with unchanged output.
The committed checker cut targets that measured choicepoint cause. Receipt 37 now
reports first/second check 17,737/145,989 and a passing immediate-finalization
test. The old phase percentages are unsuitable for independent component totals.

Receipt 37's live gate is 1,994,058 cold inferences, 810 rows, empty diagnostics;
its outer trace is 1,993,610. Both measure the accepted parser patch and checker
cut together. Its -16,806 trace delta against receipt 34 is a combined-current
measurement, not an isolated checker delta. The smaller check intervals establish the intended
boundary correction; the earlier isolated runtime-wrapper probe established no
material ordinary first-result compile saving. Receipt 37's direct consecutive-
run output equality demonstrates repeatability with both changes present.
An isolated old-checker/new-checker comparison would hold the accepted parser
implementation fixed. The audit notified the checker lane and parent of this
attribution distinction.

## Stale gates, artifacts, and CI protection

1. `v7/bench/0_compiler_performance.pl:171` pins partial to 15,562. Receipt 14
   removed ordering rows; receipt 15 documents actual partial 890 -> 910 but
   mechanically moves the older checkpoint 15,542 -> 15,562. Receipt 19 already
   identifies the mismatch. The present comparator's `partial_boundaries` test
   injects 15,562 at `v7/test/20_compiler_performance.test.pl:64`; it validates
   comparison arithmetic without compiling the fixture. Passing it cannot
   validate that checkpoint.
2. Cold budgets remain 16,000,000 for nearest and 88,000,000 for partial at
   benchmark lines 160/169. Those are 7.96x and 19.58x their latest recorded
   pre-parser cold counters. Existing budgets permit substantial inference regressions.
   Changing them is an explicit benchmark-policy decision; this audit neither
   raises nor tightens them. The warm-baseline comment still says 2,148, while
   receipt 32 reports 2,236 for nearest.
3. `v7/out/compiler-profile/4_summary.txt` records 3,534,140 profiled inferences,
   17,794 total occurrences, and 11,741 closure occurrences. This matches the
   receipt 24 shared-store stage before bound collection. Receipt 25 records
   2,923 closure occurrences. The generated file supplies no current-HEAD
   provenance and cannot describe the current compiled source.
4. `.github/workflows/dl7-userland.yml:28` runs suites 2, 15, 16, 17, 18, 19,
   then comparator suite 20 and the nearest live gate. It omits suites 0, 1,
   1a, 3, 21 and the live partial gate. Thus new entrypoint demand-cone parity,
   lower-store lifetime/collision, stratification/memo, and checker determinism
   tests are not directly executed by this workflow. Reader, trace, and profile
   guards likewise lack that workflow protection. Other integration tests may
   traverse these paths but do not replace those exact assertions.
5. `v7/test/21_compiler_profile.test.pl:216` still quantifies child category
   universally and stratum existentially. One complete stratum can satisfy its
   claimed all-strata hierarchy invariant. This known coverage gap remains.
6. Trace-finalizer reporting can still throw before reset at
   `v7/src/2_comptime/1b_compiler_tracer.pl:86`. The checker cut addresses phase
   completion, not this separate receipt-27 cleanup defect. Callback qualification,
   duplicate cons proofs, subprocess pipes, and backend boundary findings likewise
   have no corresponding fix in the verified commit arc.

Historical receipt status statements are point-in-time records: “no commit” in
receipt 32 is superseded by `70b250b3d`, and transient undefined-helper errors in
receipt 33 are superseded by its final review. Historical passing suites do not
establish current dirty-checkout CI success. This audit adds no CI coverage.

## Next two bounded tasks

1. **Refresh attribution on the committed parser/checker combination.**
   Scope: fresh nearest/partial canonical comparison plus trace/profile generation,
   retaining the structured-head cut and its two exact regression tests. Supported
   mode is `debug_checker_input(+ground Basement,+Origins)`. It reduces the
   overlapping successful debug paths from two to one, with no promised ordinary
   first-result compile speedup. `check_datalog_structured_input_has_one_solution_in_trace_modes`
   and `check_phase_finalizes_immediately_without_once` now pass per receipt 37;
   retain the focused trace failure/exception/output tests named in receipt 34.
   Keep the accepted parser fixed when isolating the checker delta, and label
   combined-current totals separately. **Kernel status: explicitly approved
   by parent coordination; no new language contract requested.**
2. **Reconcile the partial live checkpoint and wire bounded regression coverage.**
   Scope: confirm current committed 910-row/eight-round output and exact canonical
   bytes in a serial 15-second probe; update only the obsolete expected row count
   and corresponding injected comparator cases with the receipts 14/15 reason;
   add the bounded live partial invocation and selected existing evaluator lifetime/
   selector parity tests to the workflow. Keep cold/warm budgets unchanged until
   a separate explicit budget choice. Do not add an unbounded full entrypoint
   suite. **Kernel status: no kernel behavior change; checkpoint edit is justified
   by the already documented program-row change, pending coordinator authorization
   of this follow-up.**

The parser acceptance decision is complete and its patch is committed.
No source, test, SWI process, commit, or push was performed by this audit update.
