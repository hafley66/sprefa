# RACE-LOG (entrant: opus)

Base `84541acd`, verified by `git rev-parse HEAD` before any read.
Append-only. One entry per milestone.

## TOC

- [Milestone 1 — I-A](#milestone-1--i-a-ddl--decode-view--the-gun)
- [Milestone 2 — I-D](#milestone-2--i-d-the-ir-encoding-slot)
- [Milestone 3 — I-J](#milestone-3--i-j-the-reserved--namespace)
- [Milestone 4 — I-B](#milestone-4--i-b-the-ingest-door)
- [Milestone 5 — I-J rev-3 remainder + G11](#milestone-5--i-j-rev-3-remainder--g11)
- [Contract defects and ambiguities](#contract-defects-and-ambiguities)
- [Stop condition](#stop-condition)
- [Summary table](#summary-table)

---

## Milestone 1 — I-A: DDL + decode view + the gun

**2026-08-07T22:05Z.**

### What landed

| # | thing | where |
|---|---|---|
| 1 | `intern_mode/2`, `interned_column/2`, `string_dictionary_table/1`, `intern_ddl/2` | `v6/prolog/lower.pl:891-...` |
| 2 | `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`, emitted once per program, first in the DDL list, only when a text column exists | `lower.pl:program_intern_ddl/3` |
| 3 | `column_def/4`: a text column stores `INTEGER NOT NULL` under `intern(dict)` | `lower.pl:column_def/4` |
| 4 | `text_view_ddl/6` + `text_view_ddls/6`: the `__txt_<table>` decode view, returned in the SAME `Ddls` list as its table, built from the one `Columns`/`ColumnTypes` pair | `lower.pl` |
| 5 | `rel_ddl/5` -> `rel_ddl/6`; both arms (log, set) return `[Table \| Views]` | `lower.pl:rel_ddl/6` |
| 6 | delta tables carry their own `__txt___delta_<rel>` view; both tick-log reads swap `FROM` to the views | `lower.pl:delta_ddl/3`, `delta_statement/3` |
| 7 | `__ref_<type>`'s `__rendered` decodes interned columns, so a struct render is text, not ids | `lower.pl:relation_render_column_expr/5` |
| 8 | THE GUN: `intern(dict\|direct)` threaded `program_plan/3` -> `plan/8` -> `lower_program/2` -> every DDL predicate. No prolog flag, no env var | `compile.pl`, `sweep.pl`, `lower.pl`, `emit_ts.pl` |
| 9 | `--intern=dict\|direct` on the one-file compile door; `sweep/1` on the corpus door | `compile/scripts/compile_dl6.sh`, `sweep.pl` |
| 10 | `IGenProgram.internMode` + `IInternMode`; every emitted module stamps the mode that built it | `v6/tsv2/runtime/types.ts`, `emit_ts.pl:program_export_lines/3` |
| 11 | The mode crossing REFUSED at attach, before any DDL runs, by comparing the module's own dictionary DDL against the database's `sqlite_master` | `v6/tsv2/serve/3_engine.ts:internCrossingFailure` |
| 12 | G9 A/B gate: two compiles of ONE commit, canonicalized per §10.2 class, byte-equal after | `v6/tsv2/scripts/intern-ab.sh`, `scripts/intern-ab-classify.ts` |
| 13 | plunit unit `interning`, 10 tests | `compile/test/plunit_tests.pl` |

### The deviation, stated first because it is the load the grader carries

`default_intern_mode(direct)` (`compile.pl`), NOT the contract's `dict`
(§15.3 `Options carries intern(dict) | intern(direct). Default dict.`).

Reason, and it is mechanical rather than a preference: at the I-A commit the
door does not intern arrivals (I-B), literals still lower to quoted text
(I-C), and built strings are not interned on write (I-K). A `dict`-mode
program therefore writes TEXT into columns its own DDL declares INTEGER;
SQLite affinity stores it silently and every `__txt_` decode answers NULL.
Defaulting to `dict` here makes the brief's own sweep gate red BY
CONSTRUCTION, and a milestone with red gates counts as zero.

The dict lowering is not untested because of it. It is exercised by the
plunit unit (both modes, side by side) and by G9, which compiles all 211
modules at `dict` and proves the output differs from `direct` only in the
interning classes. The default flips to `dict` in the milestone where the
chain first supports it, and that flip is one atom.

### Gate outputs

```
$ cd v6/tsv2 && bash scripts/sweep.sh
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
real    0m3.9s
```

```
$ cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
% All 379 (+46 sub-tests) tests passed in 0.960 seconds (0.934 cpu)
real    0m1.1s
```
(baseline was 369; the 10 added are the `interning` unit.)

```
$ cd v6/tsv2 && pnpm exec tsgo --noEmit
(no output, 0 errors)
real    0m1.0s
```

```
$ bash v6/tsv2/scripts/intern-ab.sh                       # G9 A/B at one commit
INTERN_AB modules=211 decode-read=1863 decode-subquery=23 decode-view=1242 \
          dictionary-ddl=184 mode-stamp=422 unexplained=0
real    0m3.7s
```

```
$ cd v6/prolog && swipl -g go -t halt ARCH.pl
PASS  roadmap_is_total / construct_status_closed / construct_tier_known / covers_endpoints_ground
real    0m0.03s

$ cd v6/tsv2 && pnpm test
ℹ tests 159  ℹ pass 157  ℹ fail 0  ℹ skipped 2   duration_ms 6344

$ cd v6/sprefa-store/js && pnpm test
ℹ tests 75   ℹ pass 75   ℹ fail 0                duration_ms 8253
```

### §15.4's one-time landing receipt, recorded here as the section asks

`intern(direct)` at this commit reproduces base `84541acd`'s emitted corpus
byte for byte EXCEPT one added line per module:

```
$ git diff --stat v6/prolog/compile/out
211 files changed, 211 insertions(+)      # each insertion: `  internMode: "direct",`
```

That single line is §15.5's mode stamp, which the contract requires and which
therefore cannot be absent from a direct-mode artifact. Everything else is
identical. Recorded once; the standing gate is G9's A/B, not this diff.

### The 10-second law

Every gate above is under 10s. Slowest: `sprefa-store` tests at 8.25s, which
is pre-existing and unchanged by this milestone.

### Numbers this milestone measured

| quantity | value | how |
|---|---|---|
| modules that grow a dictionary at `intern(dict)` | 184 of 211 | G9 `dictionary-ddl` class |
| decode views emitted across the corpus | 1,242 | G9 `decode-view` class |
| boundary reads swapped to a view | 1,863 | G9 `decode-read` class |
| `__ref_` render decodes (struct-typed rels) | 23 | G9 `decode-subquery` class |
| corpus compile wall, one mode | ~1.6s | half of `intern-ab.sh`'s 3.7s |

---

## Milestone 2 — I-D: the IR encoding slot

**2026-08-07T22:12Z.** Sequencing: §20.4's `I-A -> {I-B, I-C, I-D, I-F, I-J}`.
I-D taken first of that set because it lives in `lower.pl`, which I-A had just
finished with, and because §20.4 records it as the lane rev 3 SHRANK.

### What landed

| # | thing | where |
|---|---|---|
| 1 | `ir_column_storage/5`: one clause replaced by one clause. `text` reports `storage integer, encoding dict('__str')` under `intern(dict)`, `storage text, encoding direct` under `intern(direct)` | `lower.pl:ir_column_storage/5` |
| 2 | `Collation` falls out with no edit: an interned column's storage class is `integer`, so the existing `StorageClass == text -> binary` test answers `none` | `lower.pl:ir_column_class/4` |
| 3 | Mode threaded down the IR path: `level_statement_groups/4` -> `level_statement_group/4` -> `level_ref_count_sql/5` -> `level_fixpoint_ir/5` -> `ir_storage/5` -> `ir_rel_storage/4` -> `ir_column_class/4` -> `ir_column_storage/5` | `lower.pl` |
| 4 | `interned_literals_absent/2`: the `eq_lit` fence. A walk carrying `eq_lit(_, lit(text(_)))` emits `fixpointIr: null` under `dict`, because phase 1's IR has no node for a literal that has to resolve through `__str` | `lower.pl:level_fixpoint_ir/5` |
| 5 | `ir-encoding` class added to the G9 classifier, so the colclass flip is a NAMED class rather than an unexplained line | `v6/tsv2/scripts/intern-ab-classify.ts` |
| 6 | 4 plunit pins, including the anti-drift one | `compile/test/plunit_tests.pl` |

`ir_rel_storage/3`'s signature did NOT need `Decls` (§8's "No signature change
in the IR path"). It needed the mode, which is a different thing and which the
gun requires regardless.

### The plunit pins, and what each would catch

| test | catches |
|---|---|
| `fixpoint_ir_text_column_encodes_dict` | a text column reporting `storage: text` while its DDL says INTEGER |
| `fixpoint_ir_text_column_stays_direct_at_direct` | the gun failing to reach the IR |
| `fixpoint_ir_encoding_agrees_with_ddl` | drift between the two halves of one decision. It runs `column_def/4` and `ir_column_class/4` over the SAME relplans in ONE run and compares their answers, at both modes; it does not compare two hardcoded strings |
| `text_literal_filter_fences_the_ir_at_dict` | the fence silently not firing: the same rules emit an IR at `direct` and `none` at `dict` |

`column_def/4` and `ir_column_class/4` are exported from `lower.pl` for that
third test alone; it is the only way one run can produce both outputs.

### Gate outputs

```
$ cd v6/tsv2 && bash scripts/sweep.sh
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
```

```
$ cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
% All 383 (+46 sub-tests) tests passed in 0.996 seconds (0.962 cpu)
```

```
$ cd v6/tsv2 && pnpm exec tsgo --noEmit          -> 0 errors
$ cd v6/prolog && swipl -g go -t halt ARCH.pl    -> 7 PASS
$ bash v6/tsv2/scripts/intern-ab.sh
INTERN_AB modules=211 decode-read=1863 decode-subquery=23 decode-view=1242 \
          dictionary-ddl=184 ir-encoding=32 mode-stamp=422 unexplained=0
```

`ir-encoding=32` is 16 text columns across the corpus's two `fixpointIr` heads,
counted once per mode. The fence fired on ZERO corpus modules: both flagship
`flow_reach` heads carry text columns but no text-literal filter, so no module
lost its `fixpointIr`. That is the number to watch when I-C lands, because I-C
is what makes the literal path real.

---

## Milestone 3 — I-J: the reserved `__` namespace

**2026-08-07T22:17Z.** §20.4 refocused this lane: `mixed_encoding_join` left
its brief (rev 3 made the state unreachable), so §18's `reserved_rel_namespace`
plus its fixtures is the whole lane.

### What landed

| # | thing | where |
|---|---|---|
| 1 | `compiler_owned_contract/1`, DERIVED from `catalog_ddl_contract/2` rather than listed twice | `v6/prolog/compile.pl` |
| 2 | `reserved_namespace_violation/3` + `check_reserved_namespace/1`, throwing `reserved_rel_namespace(Name)` | `compile.pl` |
| 3 | Two fail-first fixtures with their RED-before receipt in the header | `conformance/fixtures/5_compiler_quality.pl` |
| 4 | 5 plunit tests, including the derivation pin | `compile/test/plunit_tests.pl` |

The rule, as §18.1 states it: a `__` name in a DECLARATION or a rule HEAD is
refused whatever it is; in a rule BODY it is refused unless it is a registered
contract name. Reading is allowed, writing is not, which generalizes the
subtraction `compile.pl:180-183` already does for `__rel` alone.

### Where it runs, and why not where §18.3 says

§18.3 places the check at `check_supported_subset_expanded/1`. Measured: by
that point `1_host_expand.pl` has already minted `__host_demand_*` heads (5
corpus modules) and `materialize_catalog_rel/2` has already injected
`col_type('__rel'/11, ...)` decls, so the check would refuse the compiler's
own writes in its own namespace. It runs on the AUTHOR's `SugaredProg`, at the
head of `program_plan/3`, which is the position where "a user rel" is still a
distinguishable thing. Recorded as defect 6 below.

### Gate outputs

```
$ cd v6/tsv2 && bash scripts/sweep.sh
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=2 removed=0
  ADDED    reserved_namespace_declared_rel [unsupported]
  ADDED    reserved_namespace_derived_head [unsupported]
```

Two additions, both the new fixtures, both naming a program shape absent from
the corpus: exactly §10.1's "only by addition". The check is INERT on the 306
pre-existing fixtures (buckets 211/95 before, 211/95 + the 2 new after).

```
$ cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
% All 388 (+46 sub-tests) tests passed in 0.979 seconds (0.949 cpu)

$ cd v6/tsv2 && pnpm exec tsgo --noEmit          -> 0 errors
$ cd v6/prolog && swipl -g go -t halt ARCH.pl    -> 7 PASS
$ bash v6/tsv2/scripts/intern-ab.sh              -> unexplained=0
```

### G10, and the one fixture I did NOT write

§10.3 asks for three fixtures: the two refusals, plus "a program READING
`__rel` that still compiles". I wrote it, ran it, and deleted it:

```
FINAL_WRONG reserved_namespace_read_of_the_catalog
  actual={"final":{"__rel":[[1,0,0,"text","primitive",...
  oracle={"final":{}}
```

The catalog table is seeded by DDL and the reference engine holds no `__rel`
at all, so ANY conformance fixture that reads it is final-state-wrong by
construction, independent of this lane. The read/write split keeps its
receipt in plunit (`reserved_namespace_admits_a_catalog_read`), where the
oracle is not the referee. Recorded as defect 7 below.

---

## Milestone 4 — I-B: the ingest door

**2026-08-07T22:28Z.** The door is what makes a `dict` module able to accept a
row at all: a stored text column is INTEGER, and SQLite stores a string in one
without complaint.

### What landed

| # | thing | where |
|---|---|---|
| 1 | `text_intern_plan/3` + `program_text_intern_plan/3`, emitting §6.2's two statements verbatim and the per-rel column flags | `v6/prolog/lower.pl` |
| 2 | `ITextInternPlan`, `ITextPlane` in the package's header types | `v6/tsv2/runtime/types.ts` |
| 3 | `TextPlane`, interface-bound, one `defer` so a malformed row's refusal reaches the tick's error channel instead of the caller's stack | NEW `v6/tsv2/runtime/textPlane.ts` |
| 4 | `TEXT_INTERN_PLAN` emitted per module, plus the `TextPlane.intern` stage in all THREE tick shapes (naive, ordered, incremental), placed before `StructPlane.intern` | `v6/prolog/emit_ts.pl` |
| 5 | The COUNT rail, 11 tests, with three sabotage receipts in the file header | NEW `v6/tsv2/tests/textIntern.test.ts` |
| 6 | 6 plunit pins, including the emitted-order one | `compile/test/plunit_tests.pl` |
| 7 | `door-plan` and `door-call` classes in the G9 classifier | `scripts/intern-ab-classify.ts` |

### Two statements, and the third one that is NOT copied

`StructPlane` runs three per type: a same-key/different-row preflight, the
insert, the lookup. `__str`'s key IS the whole value, so that conflict cannot
exist and the preflight is deleted rather than copied. The COUNT rail asserts
the number directly:

| input | statements |
|---|---|
| 1 distinct value, 1 rel | 2 |
| 3 distinct values, 1 rel | 2 |
| 50 distinct values, 1 rel | 2 |
| 1 / 3 / 50 distinct values across 4 rels | 2 |
| empty batch | 0 |
| a batch with no interned column | 0 |

### Ordering, and why it is a compiler-side pin

Text intern runs BEFORE struct intern, because a `__ref_<type>` target table
carries text columns inside its own UNIQUE key. This file cannot see the
wiring (it drives `TextPlane` directly), so the ordering receipt is a plunit
test over the EMITTED module text:
`text_intern_runs_before_struct_intern` asserts the `TextPlane.intern` offset
is below the `StructPlane.intern` offset in `relation_depth2_dot_read`, the
corpus fixture that carries both a struct type and a text column.

### Gate outputs

```
$ cd v6/tsv2 && bash scripts/sweep.sh
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0

$ cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
% All 393 (+46 sub-tests) tests passed in 1.003 seconds

$ cd v6/tsv2 && pnpm exec tsgo --noEmit          -> 0 errors
$ cd v6/tsv2 && pnpm test
ℹ tests 170  ℹ pass 169  ℹ fail 0  ℹ skipped 1   (6.5s)

$ cd v6/prolog && swipl -g go -t halt ARCH.pl    -> 7 PASS
$ bash v6/tsv2/scripts/intern-ab.sh
INTERN_AB modules=211 decode-read=1863 decode-subquery=23 decode-view=1242 \
  dictionary-ddl=184 door-call=772 door-plan=2093 ir-encoding=32 \
  mode-stamp=422 unexplained=0
```

### What the door does NOT yet cover, named so it is not mistaken for done

| gap | who owns it |
|---|---|
| text LITERALS in comparisons and head projections still lower to quoted text | I-C |
| `boot_seed_statement/*` writes Initial rows as literal VALUES, outside the door | unowned; recorded below |
| strings BUILT at run time (`concat`, `norm`) are not interned on write | I-K |
| the `__str_stats` telemetry row and the door's third statement | I-F, I-G |

Those four are exactly why `default_intern_mode` is still `direct`.

---

## Milestone 5 — I-J rev-3 remainder + G11

**2026-08-07T22:31Z.** §20.4 gives I-J a second deliverable I did not close in
milestone 3: `mixed_encoding_join` leaves the lane's brief and becomes a
plunit unit test on `uniform_text_encoding/1`. Closing it here, plus the G11
gate §12.2 names and nothing was running.

### What landed

| # | thing | where |
|---|---|---|
| 1 | `uniform_text_encoding/1`, called at `ir_rel_storage/4`, the single place a column's encoding is chosen | `v6/prolog/lower.pl` |
| 2 | 3 plunit unit tests that CALL the predicate, one of them with a hand-built mixed list | `compile/test/plunit_tests.pl` |
| 3 | G11 as a running gate inside the A/B script | `v6/tsv2/scripts/intern-ab-classify.ts` |

### Why the invariant has no fixture, and that is the point

§5.6 argues it and I followed it: with `interned_column/2` a single clause, no
program can produce two text encodings, so a `.dl6` fixture for
`mixed_text_encoding` could never be RED. A refusal whose fixture cannot fail
is untested code a reader trusts. The test calls the predicate directly:

```prolog
uniform_text_encoding([ colclass(path, text, integer, none, dict('__str')),
                        colclass(name, text, text,    binary, direct) ])
  -> unsupported_construct(mixed_text_encoding([direct, dict('__str')]))
```

### G11, and the sabotage that proves it is not vacuous

G11 asserts that at `intern(dict)` every text column is an id: no
`"x" TEXT NOT NULL` outside a `json_valid` CHECK, and every IR text column
reporting `storage integer, collation null, encoding dict("__str")`. The
dictionary's own `content` column is excluded by dropping the `__str` DDL
line, not by name, so a user column called `content` is still checked.

Fed the DIRECT corpus as if it were the dict one:

```
INTERN_AB modules=211 ir-encoding=32 mode-stamp=422 unexplained=4738
  aggregate_count_min_max_track_arrivals_and_retraction.ts:
    text-storage column "repo" survived intern(dict)
```

4,738 findings. Fed the real dict corpus: 0.

### Gate outputs

```
$ cd v6/tsv2 && bash scripts/sweep.sh
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0

$ cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
% All 396 (+46 sub-tests) tests passed in 1.034 seconds

$ cd v6/tsv2 && pnpm exec tsgo --noEmit          -> 0 errors
$ cd v6/prolog && swipl -g go -t halt ARCH.pl    -> 7 PASS
$ bash v6/tsv2/scripts/intern-ab.sh              -> unexplained=0  (G9 + G11 in one run)
```

---

## Contract defects and ambiguities

| # | where | what | what I did |
|---|---|---|---|
| 1 | `plans/2026-08-08-interning-contract.md:214-216` | §4.1 states `Ddls = [TableDdl]` or `[TableDdl, ViewDdl]` and "NO OTHER SHAPE", with the plunit gate a LENGTH assertion. That is unsatisfiable: `lower.pl:928-938` already returns `[Ddl, ViewDdl]` for a declared struct type's `__ref_` view, so an interned struct-typed rel must return three | Read the rule as its stated intent (no table without its view) rather than as a literal length. The plunit gate is `every_interned_table_ships_its_view`: a NAMED-view assertion per interned relplan, which is strictly stronger than a length and does not false-fail on the pre-existing `__ref_` view |
| 2 | §4.3, the boundary-read row | "the delta table's own text columns are interned identically; same swap" does not say the delta table gets its own view, but the swap needs one | Emitted `__txt___delta_<rel>` from `delta_ddl/3`, carrying `_sign` and `_sequence` through verbatim. The alternative (inlining the decode into `canonical_column_expr/3`) was rejected: the text arm of that predicate does compound-term rendering OVER the value, so the decode must happen first, which is exactly what a view gives for free |
| 3 | §15.3 vs §11's sequencing | "Default dict" at the I-A commit contradicts the sweep gate the same document demands, because I-B/I-C/I-K have not landed | `default_intern_mode(direct)`, documented above and in `compile.pl` |
| 4 | §15.5 | `internMode` is required on `IGenProgram`, which makes four checked-in hand-written modules fail typecheck | Added `internMode: "direct"` to `gen/demand_laziness_effect_rows.ts`, `gen/scale_generated.ts`, `gen/switch_as_keyed_replace.ts`, `gen_emitted/door-handwritten.ts` |
| 5 | §15.5 | The contract says serve refuses the crossing but names no place the DATABASE records its mode | Derived it from the physical fact instead of adding a marker table: a database holds `__str` or it does not, and the module's own DDL either builds it or does not. Zero new DDL, and it is exactly the fact that would otherwise corrupt |
| 6 | §18.3 | "runs at `check_supported_subset_expanded/1`" is too late: host expansion has already minted `__host_demand_*` heads and the catalog has already injected `col_type('__rel'/11, ...)`, so the check refuses the compiler's own writes | Runs on the author's `SugaredProg` at the head of `program_plan/3` |
| 7 | §10.3, "reserved namespace" row | The third fixture ("a program READING `__rel` that still compiles") cannot exist: the oracle has no `__rel`, so its final state is empty while the emitted program's is not | Kept the two refusal fixtures; the allowed-read receipt is a plunit test |

### Carried forward, found while building I-A, owned by later lanes

| finding | lane that owns it |
|---|---|
| `departure_read_sql/3` (`lower.pl:193`) reads raw columns from `__departure_frontier_<rel>`; those values cross into JS and back as binds. Identity round-trips, so it is correct under interning, but it is the one path where an id leaves the database | I-C-R's audit list |
| `boot_seed_statement/*` writes Initial rows as literal VALUES; under `dict` those literals need interning before the insert, and that is NOT the arrival door | I-B / I-C boundary; neither lane's brief names it |
| `triggerOccurrences` compares JS arrival values against stored rows | I-B (the door must intern before anything downstream sees the batch) |

---

## Stop condition

**2026-08-07T22:33Z.** Stopping after five milestones, with every gate green,
rather than opening the next lane.

### What the sequencing allows next, and why each is not a milestone I can close

| lane | state | the blocker |
|---|---|---|
| **I-C** | not started | the contract's own largest single scope (§19: "the largest rev-2 scope increase"). Two rules, twelve call sites, a boot-intern statement per module, and an `EXPLAIN QUERY PLAN` receipt. Half of it is not a milestone, and a half-landed literal path is the exact silent-wrong-answer class §5.2 row 11 is about |
| **I-F** | not started | it is not the one-clause job the lane table suggests. `catalog_ddl_contract/2` has ONE row today and three callers written for one name: `compile.pl:materialize_catalog_rel/2` (an if-then that takes the first solution), `compile.pl:program_plan/3`'s ArrivalTargets subtraction, and `analyze.pl:catalog_mentions_atom/1` (which hardcodes `'__rel'/11`). `__str_stats` also needs `kind(log)` and `keep(count(4096))` injected, which no contract row carries today. Generalizing that family is its own arc |
| **I-K** | blocked | §20.4 sequences it after I-C merges; both touch the head projection path |
| **I-G** | blocked | needs I-F's contract row |
| **I-E** | blocked | §11 sequences it behind I-B-R, I-C-R and I-D-R |

### The one number that says how far the arc actually is

`default_intern_mode` is still `direct`. Four things stand between here and
flipping it, all named in milestone 4's gap table: literals (I-C), boot seed
rows (unowned), built strings (I-K), and telemetry (I-F + I-G). Milestones 1-5
are the DDL, the decode views, the gun, the IR handshake, the ingest door, the
namespace refusal and two gates. What they are not is a runnable dict mode.

### Every gate, one final run

```
sweep            SWEEP 308/211/97 crash=0; RUN wrong=0; FINAL final_wrong=0;
                 MANIFEST_REASON_DIFF added=0 removed=0 bucket_moved=0    3.9s
plunit           All 396 (+46 sub-tests) passed                           1.0s
tsgo --noEmit    0 errors                                                 1.0s
ARCH.pl          7 PASS                                                   0.03s
intern-ab.sh     G9 + G11, unexplained=0 over 211 modules                 3.8s
tsv2 pnpm test   170 tests, 169 pass, 0 fail, 1 skipped                   6.5s
store pnpm test  75 tests, 75 pass, 0 fail                                8.3s
```

Every gate under the 10-second law. The slowest, `sprefa-store` at 8.3s, is
pre-existing and untouched by this branch.

## Summary table

| milestone | lane | gates | commit |
|---|---|---|---|
| 1 | I-A | sweep wrong=0, plunit 379, tsgo 0, G9 unexplained=0, ARCH pass, tsv2 157/0, store 75/0 | `863fe1d5` |
| 2 | I-D | sweep wrong=0, plunit 383, tsgo 0, G9 unexplained=0, ARCH 7 PASS | `67a6af43` |
| 3 | I-J | sweep 308/211 wrong=0 (added=2), plunit 388, tsgo 0, G9 unexplained=0, ARCH 7 PASS | `49b76b26` |
| 4 | I-B | sweep wrong=0, plunit 393, tsgo 0, tsv2 169/0/1skip, G9 unexplained=0, ARCH 7 PASS | `a07030ba` |
| 5 | I-J remainder + G11 | sweep wrong=0, plunit 396, tsgo 0, G9+G11 unexplained=0, ARCH 7 PASS | `794b2f46` |

### Defects found, by severity

| # | defect | severity | where |
|---|---|---|---|
| 1 | §4.1's `Ddls` length assertion is unsatisfiable against the existing `__ref_` view arm | would have made I-A's own gate impossible to write | contract §4.1 |
| 2 | §18.3 places the namespace check where the compiler's own `__` writes already exist | would refuse 5 corpus modules and every catalog reader | contract §18.3 |
| 3 | §10.3's third reserved-namespace fixture cannot exist | a fixture that is FINAL_WRONG by construction | contract §10.3 |
| 4 | §15.3's `dict` default is red at the I-A commit by construction | the deviation this log opens with | contract §15.3 vs §11 |
| 5 | §15.5 names no place the DATABASE records its mode | closed without new DDL | contract §15.5 |
| 6 | §4.3's delta-read swap needs a view the contract does not name | closed by emitting one | contract §4.3 |
| 7 | I-F is scoped as a one-row addition; the catalog family is single-name today | would have surprised the lane mid-task | contract §17 |

### Numbers this branch measured that the contract did not have

| quantity | value |
|---|---|
| modules that grow a dictionary at `intern(dict)` | 184 of 211 |
| decode views emitted across the corpus | 1,242 |
| boundary reads swapped to a view | 1,863 |
| `__ref_` render decodes | 23 |
| IR text columns carrying an encoding | 16 (32 across both modes) |
| door plan lines / door call lines | 2,093 / 772 |
| corpus modules losing `fixpointIr` to the `eq_lit` fence | 0 |
| corpus modules affected by the namespace refusal | 0 |
| G11 findings when fed the direct corpus as if it were dict | 4,738 |
