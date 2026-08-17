# dl6 language stress audit, 2026-08-17

Base `7d22a3cbf5ca95bce62680e0c52593c13b033486`. Audit plus a 34-program probe
battery. No compiler change, no language design call; every finding is a row
with a throw site.

## Contents

- [1. Headline](#1-headline)
- [2. Manifest reason classification](#2-manifest-reason-classification)
- [3. Throw-site reach census](#3-throw-site-reach-census)
- [4. Docs vs parser vs registry](#4-docs-vs-parser-vs-registry)
- [5. Existing stress inventory](#5-existing-stress-inventory)
- [6. Probe coverage matrix](#6-probe-coverage-matrix)
- [7. Probe results](#7-probe-results)
- [8. Findings, ranked](#8-findings-ranked)
- [9. Cards to file](#9-cards-to-file)

## 1. Headline

| question | answer | receipt |
|---|---|---|
| how hard is the language stressed | every construct once, almost no pair | `v6/dl/fixtures/golden-flex.dl6:3-9` states this in its own header |
| what does the corpus reach | 43 of 120 `unsupported_construct` throw sites | section 3 |
| do the two doors agree | no, on 29 of 452 fixtures | section 2 |
| did 34 hand probes find anything | 3 wrong answers, 6 unnamed parse errors | sections 7 and 8 |

The corpus is wide and shallow. 452 fixtures cover 71 of 83 registry
constructs (`golden_coverage.pl` exit 0) and 401 of them are single-construct
programs. The composition axis is one file, `golden-flex.dl6`, and its own
header names the defect class that shape lets through.

## 2. Manifest reason classification

`v6/prolog/compile/out/manifest.json`, 452 rows, buckets from `sweep.pl:264`
(`compiled` / `unsupported` / `crash`).

| bucket | rows |
|---|---|
| compiled | 342 |
| unsupported | 110 |
| crash | 0 |

`conformance/go.pl:13-22` loads every `fixtures/*.pl` and grades every
`fixture/5` term against the oracle with no skip list, so the ORACLE door and
the COMPILER door are graded over the same 452 rows. That makes the fixture's
own expectation list the classifier: a fixture expecting
`throws(unsupported_construct(...))` means both doors agree; a fixture
expecting deltas, `final(...)` or `ticks(...)` while the compiler stops means
the doors disagree.

Script: `v6/prolog/conformance/probes/2026-08-17-stress/classify_unsupported.py`.

| class | rows | meaning |
|---|---|---|
| both doors agree | 81 | the fixture's expectation IS the stop; intended negative |
| DOOR SPLIT | 29 | the oracle grades behaviour, the compiler stops |

81 is an upper bound and 29 a lower bound: the classifier looks for the string
`throws(` anywhere in the fixture chunk, so a trailing comment mentioning it
counts as agreement.

### 2.1 The 81 agreed negatives, by what they probe

| probe kind | example row | throw site |
|---|---|---|
| typo of a declared name | `column_type_unknown(spann)`, `column_type_unknown(treee)`, `column_type_unknown(fighter_summry)`, `json_capture_type_unknown(itn)` | `0_type_plane.pl:137`, `lower.pl:5601` |
| name collision | `enum_variant_name_collision(page)`, `option_companion_name_collision(...)`, `duplicate_host_decl(look)`, `host_column_shadows_runtime(...)` | `0_enum_expand.pl:209`, `0_option_expand.pl:134` |
| arrival shape law | 11 `type_arrival_shape_mismatch(...)` rows across `4_struct_values.pl` and `5_value_plane.pl` | `compile.pl:317-318` |
| the no-coercions decision | `comparison_type_mismatch`, `join_column_type_mismatch`, `arith_operand_not_number` | `lower.pl:2305`, `lower.pl:335`, `lower.pl:849` |
| rel-kind placement law | `keyed_log_rel`, `log_on_level_headed_rel`, `keep_on_non_log_rel`, `edge_into_unkeyed_set`, `latest_in_level_rule`, `pre_in_level_rule`, `keyed_level_head` | `analyze.pl`, `lower.pl:3651` |
| coalesce placement law | 4 `coalesce_*` rows | `0_coalesce_expand.pl:157-262` |
| regexp subset law | 4 `regexp_*` rows | `lower.pl:2264-2268` |
| reserved word pinned | `zip`, `lifecycle_arm(subscribe|unsubscribe|complete|error)`, `removed_word(scan)` x2 | `registry.pl:43-48`, `analyze.pl:1544` |
| recursion law | `built_text_in_recursive_head`, `built_list_in_recursive_head`, `recursive_cte_multiple_self_reads` | `lower.pl:5260-5264` |

None of these is a real impossibility. They are the language's declared laws,
each with a fixture holding it in place. This half of the corpus is healthy.

### 2.2 The 29 door splits

| fixture file | rows | stop | what the oracle does instead |
|---|---|---|---|
| `temporal_pipe.pl` | 9 | `edge_body_needs_json_destructure` | runs the whole ghcacher chain and grades its tick log |
| `json_arm.pl` | 6 | `level_body_goal(json_each)` x2, `aggregate_head(json_array)` x2, `decode_source_not_struct` x2 | fans out and rolls up |
| `state_machine.pl` + `scopes.pl` + `door_split_trigger_literal.pl` | 4 | `trigger_arg_not_var` | matches the constructor structurally in trigger position |
| `2_hosts_wiring.pl` | 2 | `level_body_goal(json_each)` | normalizes the host's json |
| `5_compiler_quality.pl` | 2 | `reserved_rel_namespace` | admits `__txt_reach` / `__str_stats` as rel names |
| `expressions.pl` | 2 | `comparison_type_mismatch`, `join_column_type_mismatch` | returns zero rows rather than rejecting the program |
| `10_list_elements.pl` | 1 | `list_of_relation_refs(span)` | grades `final(doc/2, [...])` |
| `4_struct_values.pl` | 1 | `decode_field_unknown(span,beginning)` | grades behaviour |
| `merge_family.pl` | 1 | `edge_body_with_negation` | grades the disjoint seed/transition |
| `operators.pl` | 1 | `compound_pattern_on_arrival_rel` | binds the fork-join error arm |

Exactly ONE of these 29 is pinned as a door split anywhere in the repo:
`conformance/fixtures/door_split_trigger_literal.pl:9-40`, whose header says
"Programs therefore exist that GRADE on one door and cannot compile on the
other" and leaves the call to the user. The other 28 carry no such header.

Two of the 29 also carry a fixture NAME that contradicts their own body:
`list_of_relation_refs_still_refused` and `struct_decode_field_unknown_rejected`
are named for a stop and graded for a result.

The largest cluster has a stale owner. `ARCH.pl:830`:

    task(json_edge_body_unblock, unbuilt, []). % REASON WENT STALE, unowned.

The row's own text says the encoding question that motivated
`edge_body_needs_json_destructure` was ruled on 2026-07-29
(`compound_storage = struct_as_rows`), that the guard seam for edge bodies
already landed for negation, comparisons and binds, and that "json is not a
time thing, so edge bodies should accept decode". Nine fixtures wait on it.

## 3. Throw-site reach census

`compile/scripts/arm_census.pl:31-53` already answers this for `lower.pl`. The
same static method extended to every prolog module
(`grep throw(unsupported_construct(` cross manifest reason functors):

| module | throw sites | reached by corpus | unreached |
|---|---|---|---|
| `lower.pl` | 49 | 16 | 33 |
| `analyze.pl` | 13 | 4 | 9 |
| `0_generic_expand.pl` | 12 | 6 | 6 |
| `0_dot_expand.pl` | 9 | 2 | 7 |
| `0_coalesce_expand.pl` | 7 | 4 | 3 |
| `0_option_expand.pl` | 6 | 4 | 2 |
| `0_enum_expand.pl` | 5 | 1 | 4 |
| `0_match_expand.pl` | 5 | 1 | 4 |
| `compile.pl` | 5 | 3 | 2 |
| `0_type_plane.pl` | 3 | 2 | 1 |
| `0_seq_expand.pl` | 3 | 0 | 3 |
| `6_profile.pl` | 1 | 0 | 1 |
| `use_resolve.pl` | 1 | 0 | 1 |
| `compile/parse_dl_dcg.pl` | 1 | 0 | 1 |
| **total** | **120** | **43** | **77 (64%)** |

Per the standing law that a named stop is a hypothesis, 77 named stops in this
compiler have never been shown to fire. One of them,
`aggregate_operand_not_number` at `lower.pl:5864`, was reached for the first
time by probe `n11` in this audit, and reaching it is what exposed finding 1:
it fires for a struct reference and does not fire for the enum column sitting
next to it.

## 4. Docs vs parser vs registry

| check | result |
|---|---|
| every `SYNTAX.md` construct row exists in `registry.pl` | yes, 59 of 59 |
| every `registry.pl` surface row appears in `SYNTAX.md` | NO, 59 of 60 |
| every live registry row is exercised by `golden-flex.dl6` | yes, `golden_coverage.pl` prints `71 exercised + 12 named absences`, exit 0 |
| every parser production has a registry row | NO, one hardcoded bypass |

### 4.1 Mismatch table

| mismatch | evidence | effect |
|---|---|---|
| `pre/2` has a live registry row and no `SYNTAX.md` row | `registry.pl:72` vs `SYNTAX.md:107-165` (only `pre/1` at :118) | the generated construct table is stale; `1_emit_registry_docs.pl` has not been re-run since the row landed |
| `parse_dl_dcg.pl:1000` hardcodes `pre/2` ahead of the registry lookup | `{ Name = pre, Arity = 2, Shape = rel_atom_default ; surface(...) }` | dead disjunct: `wrapper_lower_role/3` at `registry.pl:558` already projects `wrapper(rel_atom_default, lower)` into the same shape. One construct bypasses the single-inventory law |
| `SYNTAX.md` names `parse_dl.pl` as the canonical parser | title line plus 4 body references; the file does not exist | the doc points readers at a deleted module. 16 files repo-wide still cite it, `registry.pl` and `CONSTRUCT-REFERENCE.md` included |
| `CONSTRUCT-REFERENCE.md` documents 12 of 60 surface rows | it is generated from LOOSE COMMENTS preceding registry rows, per its own line 3 | a construct without a comment run is undocumented by construction, and nothing fails |
| `probe/4` has a registry row and no distinct `.dl6` spelling | `registry.pl:195`; a host call in a rule body IS the probe | no text analysis of the corpus can count it. It reads as an ordinary rel atom |

The construct table itself is generated from `registry.pl`, so sections 4.1
row 1 is a regeneration lag rather than a hand-maintained divergence. The
mechanism is right; nothing runs it.

## 5. Existing stress inventory

| tool | what it varies | what it judges | shape |
|---|---|---|---|
| `conformance/go.pl` | nothing; fixed programs and schedules | the ORACLE's deltas / final / ticks against 452 hand expectations | acceptance |
| `tsv2/scripts/sweep.sh` | nothing | the COMPILER's bucket per fixture, plus emitted-vs-oracle tick log and final state, both emitter modes | differential, two doors |
| `compile/scripts/text_door_receipt.pl` | the SPELLING: term door vs printed `.dl6` re-parsed | byte-identical emitted TypeScript | metamorphic, two doors |
| `compile/scripts/roundtrip.sh` | print then re-parse | `parse(print(T)) =@= T` for every fixture; regenerates `dl_view/*.dl6` | metamorphic, one door |
| `compile/scripts/metamorphic_rename.pl` | every rel / variable / module segment renamed (camelCase, dunder, ALLCAPS, max length) | emitted artifacts identical modulo the rename map | metamorphic |
| `compile/scripts/golden_coverage.pl` | nothing | every live registry row is exercised by `golden-flex.dl6`, and every named absence is explained in the golden's header | inventory gate |
| `compile/scripts/arm_census.pl` | nothing | which `lower.pl` throw sites and clause arms no corpus program reaches | coverage census |
| `v6/dl/fixtures/golden-flex.dl6` | cardinality (0 / 1 / >=100 rows per input rel) plus a perturbed schedule | oracle vs both emitter modes, tick log and final state, then again through the served HTTP engine | composition |
| `v6/tsv2/goldens/scip_combo` | nothing | both doors byte-diffed | end-to-end |
| `compile/test/plunit_tests.pl` | nothing; 640 unit cases | compiler internals | unit |
| `sprefa-engine-rs/grade.sh` | the TARGET: TypeScript vs Rust | byte-clean per fixture | differential, two targets |

Nothing in that table varies the PROGRAM. Every metamorphic leg varies
spelling, naming or target and holds the construct set fixed. The one axis
that is never perturbed is which constructs appear together, which is exactly
what card `construct-pair-matrix` proposes and what section 6 samples by hand.

Open cards already on this axis: `fuzz-grammar-threedoor`,
`construct-pair-matrix`, `naive-selfdiff-random`, `schedule-permutation`,
`kill9-midtick`, all `status: open`, all under epic `bug-mining`.

## 6. Probe coverage matrix

`conformance/probes/2026-08-17-stress/coverage_matrix.py` reads all 401
`compile/dl_view/*.dl6` renders plus `golden-flex.dl6` and reports, per
construct pair, how many single files carry both.

| measure | value |
|---|---|
| files scanned | 402 |
| construct pairs where both sides occur somewhere | 435 |
| pairs never occurring in one file | 71 (16%) |

The uncovered set is not spread evenly. It concentrates on the corpus's
loneliest live constructs:

| construct | files carrying it | uncovered pairs |
|---|---|---|
| `module_path` (dotted rel path) | 17 | 25 |
| `spread` (`[... p]`) | 5 | 21 |
| `bind_decl` | 4 | 4 |
| `combine` | 3 | 3 |
| `next` | 3 | 3 |
| `seq` | 3 | 3 |
| `ts_query` | 2 | 3 |

Two caveats on the matrix, both stated so the number is not read as more than
it is:

1. `dl_view/*.dl6` DROPS `: type` annotations and whole `rel` lines
   (`CLAUDE.md`), and `compile/out/text-door/*.dl6` is gitignored and absent
   from the tree, so the TYPE PLANE (`enum_decl`, `option(T)`, `list(T)`,
   struct refs) is invisible to any text scan of the committed corpus. Probes
   `p21`, `p22`, `n9`, `n10`, `n11` cross it directly instead. Finding 1 came
   out of that blind spot.
2. `probe/4` has no distinct `.dl6` spelling, so its column in the matrix is a
   false detector matching rels literally named `probe`. Its pairs were
   replaced with `sh_decl` pairs, which spell the same axis.

Pairs chosen for the battery: the 12 `module_path` pairs that reach a live
wrapper or a body construct, 6 `spread` pairs, the 2 stops with the largest
door-split cluster behind them, and 2 type-plane pairs the matrix cannot see.

## 7. Probe results

34 programs under `v6/prolog/conformance/probes/2026-08-17-stress/`
(23 `p*` pair probes, 11 `n*` narrowing controls). Driver: `run.sh`, compiling
each on the text door via `compile/scripts/compile_dl6.sh`. Whole battery
6.4s wall; slowest single program 0.25s.

| bucket | probes |
|---|---|
| compiled, answer correct | 18 |
| compiled, ANSWER WRONG | 3 (`p21`, `n9`, `n10`) |
| stopped with a named construct | 7 |
| crashed with a raw parse error | 6 |

| probe | pair | bucket | reason |
|---|---|---|---|
| `p00_smoke` | aggregate x typed columns | compiled | |
| `p01_modulepath_x_edge_keyed` | module_path x edge+keyed | compiled | |
| `p02_modulepath_x_pre` | module_path x `pre/2` | **crash** | parse error at 10:28 |
| `p03_modulepath_x_not` | module_path x `not` | compiled | |
| `p04_modulepath_x_aggregate` | module_path x aggregate | compiled | |
| `p05_modulepath_x_decode` | module_path x decode | compiled | |
| `p06_modulepath_x_latest` | module_path x `latest/1` | **crash** | parse error at 8:24 |
| `p07_modulepath_x_coalesce` | module_path x `coalesce/2` | **crash** | parse error at 10:25 |
| `p08_modulepath_x_match` | module_path x match | unsupported | `unresolvable_path` |
| `p09_modulepath_x_regexp` | module_path x regexp | compiled | |
| `p10_modulepath_x_seq` | module_path x seq | compiled | |
| `p11_modulepath_x_shdecl` | module_path x sh host | compiled | |
| `p12_modulepath_x_spread` | the two loneliest, crossed | compiled | |
| `p13_spread_x_edge` | spread x edge rule | unsupported | `edge_body_needs_json_destructure` |
| `p14_spread_x_cmp` | spread x comparison | compiled | |
| `p15_spread_x_not` | spread x negation | compiled | |
| `p16_spread_x_keyed` | spread x keyed head | unsupported | `keyed_level_head(latest_item/2)` |
| `p17_spread_x_bind` | spread x `:=` | compiled | |
| `p18_spread_x_aggregate_nested` | two nested spreads x aggregate | compiled | |
| `p19_decode_in_edge_minimal` | decode x edge rule | unsupported | `edge_body_needs_json_destructure` |
| `p20_trigger_literal_minimal` | literal in edge trigger | unsupported | `trigger_arg_not_var` |
| `p21_option_x_aggregate` | `option(int)` x `sum()` | **compiled, WRONG** | see finding 1 |
| `p22_enumrel_x_edge_keyed` | enum rel x edge+keyed | compiled | |

### 7.1 Narrowing controls

| probe | question | bucket | answer |
|---|---|---|---|
| `n1_flat_pre` | is p02 the dot or the `pre/2`? | compiled | the dot; the same program flat compiles |
| `n2_modulepath_x_next` | does the gap reach `next/1`? | **crash** | yes |
| `n3_modulepath_x_combine` | does it reach the `atom_list` shape? | **crash** | yes |
| `n4_modulepath_x_finalize` | does it reach `finalize/1`? | **crash** | yes |
| `n5_match_flat_source_dotted_arm` | is p08 the arm head? | compiled | no |
| `n6_match_dotted_source_flat_arm` | is p08 the source? | compiled | no |
| `n7_match_dotted_both_one_arm` | is p08 the combination? | compiled | no |
| `n8_dotted_head_undeclared` | is p08 the missing declaration? | unsupported | YES, `unresolvable_path` |
| `n9_option_x_comparison` | does the comparison path catch what `sum()` let through? | **compiled, WRONG** | no |
| `n10_enum_x_aggregate` | is the hole specific to `option`? | **compiled, WRONG** | no, any enum column |
| `n11_structref_x_aggregate` | does a struct ref sum too? | unsupported | no, `aggregate_operand_not_number` fires |

## 8. Findings, ranked

### F1 (wrong answer) An enum-typed or option-typed column silently joins numeric aggregates and comparisons

`sum()` over a column typed `option(int)` or typed with a declared enum
compiles and sums the DISCRIMINATOR ROW ID.

Emitted, `p21`:

    CREATE TABLE "tree" (..., "grams" INTEGER NOT NULL, ...)
    CREATE TABLE "__opt_int_tag" ("__id" INTEGER PRIMARY KEY, "id" INTEGER NOT NULL, "tag" INTEGER NOT NULL, ...)
    SELECT sum(b0."grams") FROM "tree" b0 HAVING count(*) > 0

`tree.grams` holds an `__opt_int` instance id. The sum is a sum of surrogate
ids. `n10` shows the same for a hand-written enum
(`sum(b0."grade")` over `grade_v_tag.id`), and `n9` shows the comparison path
is equally open (`WHERE (b0."grams" > 100)` against the option id).

RCA. `0_enum_expand.pl:194-197` retypes every enum-typed column to `int`:

    retarget_enum_column_type(EnumToTag,
                              col_type(Ref, Column, EnumName),
                              col_type(Ref, Column, int)) :- ...

After that the type plane cannot tell an enum instance id from an int, so
`lower.pl:5860-5864`'s `memberchk(Type, [int, float])` says yes. A STRUCT
reference keeps its type name and is correctly stopped, which `n11` shows by
firing `aggregate_operand_not_number` on the identical shape.
`option(scalar)` desugars into an enum at `0_option_expand.pl:96-101`, so it
inherits the hole.

This contradicts the pinned user decision "no coercions: an untyped column
does not silently take part in a comparison or a numeric aggregate". The
decision is enforced for text-vs-int (`lower.pl:2305`, `lower.pl:335`, both
fixture-pinned) and unenforced for the enum arm. No fixture covers it: the
type plane is invisible to `dl_view`, so no corpus-text analysis could have
found it either.

### F2 (unnamed stop) A module path inside any surface wrapper is a raw parse error

Seven LIVE constructs cannot take a dotted rel path, and the stop is a
`dl_parse_error`, not a named `unsupported_construct`.

| construct | wrapper shape | probe |
|---|---|---|
| `latest/1` | `rel_atom` | `p06` |
| `finalize/1` | `rel_atom` | `n4` |
| `next/1` | `rel_atom` | `n2` |
| `pre/1`, `pre/2` | `rel_atom`, `rel_atom_default` | `p02` |
| `coalesce/2` | `rel_atom_default` | `p07` |
| `combine/variadic` | `atom_list` | `n3` |

RCA. `parse_dl_dcg.pl:1104-1107`:

    rel_atom_term(Term) -->
        ident(Name), #`(`,
        args(expr, Args), #`)`,
        { Term =.. [Name | Args] }.

`ident(Name)` is a single identifier. Every other atom position uses
`dotted_path(Segs)` plus `path_atom/4` (`parse_dl_dcg.pl:877-887` for heads,
:1170 for body atoms), which is why `p01`, `p03`, `p04`, `p05`, `p09`, `p10`,
`p11`, `p12` all compile with dotted rels. `n1` isolates it: the same program
with the dot removed compiles.

Weight: the repo's posture is that every stop is named with its construct.
This one is a column number. A user writing `latest(orchard.roster(X, Y))`
gets "parse error at line 8, column 24: statement".

### F3 (asymmetry, 29 rows) The doors disagree on 29 fixtures and one says so

Section 2.2. 29 of 452 fixtures are graded for behaviour on the oracle and
stopped on the compiler; `door_split_trigger_literal.pl` is the only one whose
header states the split. The largest cluster (9 `temporal_pipe.pl` fixtures)
sits behind an ARCH row that marks its own reason stale
(`ARCH.pl:830`, `json_edge_body_unblock`, `unbuilt`, unowned).

Sub-finding: `state_machine.pl:1-7` calls itself "the rxjs-jutsu receipt,
written entirely in already-ruled constructs", and both of its fixtures stop
on the compiler with `trigger_arg_not_var`. `scopes.pl:11-13` says "All twelve
promote cleanly; nothing was dropped", and one of the twelve stops the same
way. The corpus's two flagship idiom receipts are oracle-only.

### F4 (blind spot, 64%) Most named stops have never fired

Section 3. 77 of 120 `unsupported_construct` throw sites are unreached by the
whole 452-fixture corpus; `lower.pl` alone contributes 33 of 49 unreached.
`arm_census.pl` already computes this for `lower.pl` and is not wired into any
gate, so the number drifts silently. F1 is a direct consequence: the one
aggregate type check that DOES exist was unreached until this audit's `n11`.

### F5 (asymmetry) Declaration inference works for flat heads and not for dotted ones

An undeclared FLAT derived head compiles (`p00`'s `budget`, `p09`'s `tagged`).
An undeclared DOTTED derived head is `unsupported_construct(unresolvable_path)`
(`n8`). `p08` is only this: two match arms whose dotted heads carry no `rel`
line, and adding the declarations makes the identical program compile (`n7`).

RCA. `0_dot_expand.pl:168-175` resolves a path against SCOPES built from
declarations; a head with no declaration has no scope entry and there is no
synthesis step for the dotted case. The message names the path and not the
fix, and its location renders as "rule-index unavailable"
(`0_unsupported_messages.pl:6-9`).

### F6 (drift) Three doc surfaces disagree with the code they describe

Section 4.1. `pre/2` missing from the generated `SYNTAX.md` table; `SYNTAX.md`
naming a deleted `parse_dl.pl` as canonical in its title and 4 body lines;
`CONSTRUCT-REFERENCE.md` documenting 12 of 60 surface rows because it is
generated from loose comments rather than from the rows. Nothing gates any of
the three, while `golden_coverage.pl` DOES gate the registry against the
golden and passes.

### F7 (tooling) The committed corpus render hides the type plane

`dl_view/*.dl6` drops `: type` annotations and whole `rel` lines; the faithful
`out/text-door/*.dl6` is gitignored and absent. Any analysis of the corpus's
own text, including section 6's matrix and any future pair-matrix tool, is
blind to `option(T)`, `list(T)`, enum rels and struct refs unless it reads the
`fixture/5` terms instead. F1 lives exactly there.

## 9. Cards to file

| slug | one line | owner |
|---|---|---|
| `lang-enum-column-coercion` | `0_enum_expand.pl:194-197` retypes enum columns to `int`, so `sum()` and `>` read the discriminator id; add the type-plane check the struct-ref arm already has, and a fixture per arm | coordinator, needs Chris for the type-plane call |
| `lang-modulepath-in-wrapper` | `parse_dl_dcg.pl:1104` `rel_atom_term//1` uses `ident` where every other atom position uses `dotted_path`; 7 live constructs take a raw parse error on a dotted argument | lane, mechanical |
| `lang-door-split-census` | 28 of 29 oracle-vs-compiler splits carry no header saying so; put the census in a gate that fails when the count moves, and give each cluster a header or a close | lane |
| `lang-dotted-head-decl-inference` | an undeclared dotted derived head is `unresolvable_path` while an undeclared flat one compiles; either synthesize or say so in the message | lane |
| `docs-registry-doc-regen` | `pre/2` missing from the generated `SYNTAX.md` table, `parse_dl.pl` cited in 16 files after deletion, `CONSTRUCT-REFERENCE.md` covering 12 of 60 rows; make the emitters a gate | lane, mechanical |
| `arm-census-in-the-gate` | `arm_census.pl` computes the unreached-throw-site count and nothing runs it; ratchet it like `no-new-eprintln` | lane |
| `text-door-render-committed` | commit `out/text-door/*.dl6` or teach corpus analysis to read `fixture/5` terms, so the type plane stops being invisible | lane |

Card `construct-pair-matrix` (open, epic `bug-mining`) is validated by this
audit rather than superseded: 22 hand pairs out of 435 found 3 wrong answers,
6 unnamed parse errors and 1 asymmetry. Its `related` link to
`fuzz-grammar-threedoor` is the right sequencing; the pair matrix is the
cheaper half and should land first.

### What a real battery adds over what exists

| axis | varied today by | gap |
|---|---|---|
| spelling | `roundtrip.sh`, `text_door_receipt.pl` | none |
| naming | `metamorphic_rename.pl` | none |
| target | `grade.sh` (ts vs rust) | none |
| cardinality | `golden-flex.dl6` (0 / 1 / >=100) | one program only |
| schedule order | nothing | card `schedule-permutation` |
| CONSTRUCT SET | nothing | card `construct-pair-matrix`, this audit's sample |
| grammar surface | nothing | card `fuzz-grammar-threedoor` |
| crash consistency | nothing | card `kill9-midtick` |
