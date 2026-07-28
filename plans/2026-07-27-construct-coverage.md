# CONSTRUCT-COVERAGE: the language construct budget, added to ARCH.pl

Answers the user's question ("where is the automated map of language features
and what they cover"): `v6/prolog/tools/arch_map.pl` already derived an atlas
from `v6/prolog/ARCH.pl`, but every existing dimension (`graph/4`,
`algorithm/4`, `task/3`, `capability/3`, `prior_art/4`) is about ARCHITECTURE.
Nothing tracked the LANGUAGE CONSTRUCT BUDGET itself, or which conformance
fixture / user ruling actually exercises each construct. This arc adds
`construct/3` and `covers/2` to ARCH.pl, two new `check/2` rows wired into the
existing `go/0`, and a fifth atlas dimension in `arch_map.pl`.

## 1. Count reconciliation: 29, not 28

The task brief cited "the 28-budget." The actual documented count is 29, and
the gap is traceable to a specific line the source docs disagree on.

Chronology (commit timestamps, `git log --follow`):

| time | file | states |
|---|---|---|
| 15:38 | `plans/2026-07-27-aggregate-analysis.md` (commit 502456bc) | 1d's own count table: T0=18, T1=4, T2=1, T3=0, T4=7, **total 30** |
| 16:06 | `v6/prolog/conformance/rulings.pl` (commit f26fc6ef) | line 102: "Construct-budget cuts: `\|>` deferred... `quote()` cut... Inventory: **30 - 2 cuts + 1 departure form = 29**." (cuts at `cut_pipe`/`cut_quote`, lines 103-104; the departure addition at `r4_departure`, lines 84-89) |
| 18:07 | `plans/2026-07-27-extraction-spellings.md` (commit 82ab289f) | line 25: "The construct count stays at **28**." line 444-445: "Budget stands at **28** after today's cuts. This lab proposes zero grammar constructs, so it stays at 28." |

extraction-spellings.md was filed nearly two hours AFTER rulings.pl's own
arithmetic landed at 29 (the `r4_departure` ruling, adopting `departed/1` as a
new T4 construct, is dated in the same rulings.pl commit at 16:06). Its "28"
does not carry the +1 departure forward — it reads as `30 - 2 = 28`, silently
dropping the departure addition that had already landed. Nothing in the
07-27 doc set reconciles this; there is no third doc that explains the
departure as zero-cost or folds it into an existing row.

**Decision: this arc's `construct/3` table has 29 rows** (the fuller
accounting, matching rulings.pl's own arithmetic comment), not 28. Per the
task brief's own instruction ("if you count something else, your .md explains
the delta line by line") — this is that explanation. If the user's intent was
literally 28, the fix is one row: drop `departure_form` and the count matches
extraction-spellings.md's stated number, at the cost of ignoring a ruling
(`r4_departure`) that plainly is a new construct with its own grammar row in
FIXTURES.md (`departed(Atom)`, FIXTURES.md:23-26).

### Per-tier breakdown (29 total)

| tier | count | names |
|---|---|---|
| T0 | 17 | enum_decl, struct_decl, rel_decl, key_type, option_type, level_rule, fact, negation, aggregate_head_forms, comparison_ops, arithmetic_ops, bind_goal, fn_application, interpolation, named_column_atoms, wildcard, snapshot_ask |
| T1 | 4 | from_world_modifier, bind_decl, quoted_region, grammar_import |
| T2 | 1 | graph_operator_position |
| T3 | 0 | (none — diagnostics is a library + CLI convention, zero syntax, per aggregate-analysis.md row) |
| T4 | 7 | edge_rule, rel_kind_decl, trigger_marker, now_read, pre_read, retention_clause, departure_form |

T0 = 18 (aggregate 1d) minus 1 (`cut_quote`, `quote(...)` cut, rulings.pl:104)
= 17. T4 = 7 (aggregate 1d) minus 1 (`cut_pipe`, `\|>` deferred, rulings.pl:103)
plus 1 (`r4_departure`, rulings.pl:84-89) = 7. T1/T2/T3 unchanged from
aggregate 1d. 17+4+1+0+7 = 29.

`extraction_ops` (scan/regex/comment/ast/sg/json body syntax) and "regex +
path literals" are explicitly NOT counted: `plans/2026-07-27-extraction-
spellings.md` (the whole document) argues both cost zero new grammar
constructs — they resolve into existing T1 rows (`quoted_region`,
`grammar_import`) plus a stdlib rel library, never a new construct
(extraction-spellings.md:442-458, "New constructs and their budget cost"
table, every row reads "0"). T5 and above (effect signature arrow, envelope
enums, `Stream`/`Tail` wrappers, tail asks) are excluded on purpose —
aggregate-analysis.md's own count table (section 1d) stops at T4, and this
budget follows it exactly.

### Status enum

`construct_status/1` is closed to three values (coarser than the aggregate
doc's inconsistent per-row prose — "(add)", "(add to Surface)", "NEW",
"RESPECIFIED" — because the coarse split is what `check/2` can mechanically
verify without parsing English):

- `kept` (24 of 29) — name and semantics carried from LANG.md or an earlier
  ruled lab, unchanged this arc.
- `respecified` (1: `edge_rule`) — the name `<+` survives; R2 changed what it
  means (arrow = trigger, rel-kind = storage; consequences no longer
  universally "never retract," only Log-headed ones do).
- `new` (4: `from_world_modifier`, `rel_kind_decl`, `trigger_marker`,
  `departure_form`) — no prior spelling, either invented this arc
  (`rel_kind_decl`, the 1b convergence construct) or a replacement spelling
  for a keyword LANG.md itself killed (`source` → `from_world_modifier`;
  `delta()` → `trigger_marker`, ruling Q6).

## 2. Coverage: 19 covered, 10 uncovered

`covers(FixtureFileOrRuling, Construct)` links each conformance fixture file
(basename, matching `fixture_file/1` in `tools/arch_map.pl`) and each ruling
id (matching `ruling/4` in `conformance/rulings.pl`) to the constructs it
exercises or decides. One atom can be both — `json_arm` names a fixture file
*and* a ruling; a `covers(json_arm, _)` fact is grounded either way, and the
`covers_endpoint_exists/1` check in ARCH.pl accepts it under both readings.

Every edge is grounded in one of: a fixture file's header comment, a line in
`FIXTURES.md` (the shared grammar the reference engine actually implements),
or a ruling's own comment in `rulings.pl`. Where I could not find a citable
line, the construct is listed as uncovered below rather than linked on
naming-alone resemblance ("no vibes edges").

### Covered (19)

| construct | covered by | citation |
|---|---|---|
| `enum_decl` | state_machine | fixture header: "Envelope arms are matched STRUCTURALLY... exactly enum-match-as-rules" (state_machine.pl:20-22) |
| `struct_decl` | json_arm, state_machine, ruling `json_arm` | FIXTURES.md:28 (braces json literal); json_arm.pl `braces_literal_canonicalizes`/`braces_in_head_position`; rulings.pl:75-80 |
| `key_type` | check_eventing, engine_core, merge_family, occurrence_identity, shell_stream, spine_semantics, state_machine, ruling `q8_key_vs_arrow`, `r_equal_row_write`, `s2_file_rels` | `keyed(Ref, Positions)` in Decls, FIXTURES.md:19; grep confirms real (non-op-decl) usage in each file; rulings.pl:47-51, 68-69, 108-109 |
| `option_type` | timeless_rail | explicit `none` sentinel values, timeless_rail.pl (e.g. lines 84,88,133...); matches review_timeless_rail.md:18 "explicit `none` suffices" cited in aggregate-analysis.md row 5 |
| `level_rule` | timeless_rail, ruling `s3_dirtiness` | `<-` FIXTURES.md:20; timeless_rail.pl is the promoted eprintln rail (all `<-`); rulings.pl:110-112 ("dirty/1 is always a level rule") |
| `fact` | operators, spine_semantics | non-empty `InitialRows` lists, e.g. operators.pl `[ reading(cpu, 12), reading(disk, 4) ]` |
| `negation` | check_eventing, engine_core, timeless_rail | `not(Goal)` FIXTURES.md:21; timeless_rail.pl `not(eprintln_waived(...))`/`not(eprintln_baseline(...))`, the literal worked lint-rail example from extraction-spellings.md section 3a |
| `aggregate_head_forms` | check_eventing, engine_core, json_arm, timeless_rail, ruling `q7_aggregate_multiplicity`, `q9_aggregate_heads`, `json_arm` | FIXTURES.md:29-31; `count/sum/min/max/json_array/json_object`; rulings.pl:41-45, 53-56 |
| `comparison_ops` | expressions, operators, temporal_pipe, timeless_rail | `< =< > >= == \==` FIXTURES.md:22; expressions.pl is the primary promotion |
| `arithmetic_ops` | expressions, operators | `+ - * / mod` FIXTURES.md:27; operators.pl `Value * 2` |
| `bind_goal` | expressions, json_arm, merge_family, occurrence_identity, operators, state_machine | `Var := Expr` FIXTURES.md:22 |
| `fn_application` | expressions | `concat([..])` FIXTURES.md:27-28 — only 1 of the 12 stdlib names is exercisable; FIXTURES.md:81-82 puts "stdlib string fns beyond concat" out of scope, so this is a PARTIAL receipt, noted not hidden |
| `interpolation` | expressions | fixture `interpolation_desugars_to_concat`, expressions.pl:175-189, exercising the `${...}` → `concat([..])` lowering named in FIXTURES.md:27-28 |
| `edge_rule` | check_eventing, engine_core, merge_family, occurrence_identity, shell_stream, spine_semantics, state_machine, temporal_pipe, ruling `q4_edge_propagation` | `<+` FIXTURES.md:20; rulings.pl:26-28 |
| `rel_kind_decl` | check_eventing, engine_core, merge_family, occurrence_identity, operators, shell_stream, spine_semantics, state_machine, ruling `q2_scoping`, `q3_rel_kind_shape`, `r7_boundary_diff` | `kind(Ref, set\|log)` FIXTURES.md:19; rulings.pl:17-24, 63-67 |
| `trigger_marker` | check_eventing, engine_core, occurrence_identity, operators, shell_stream, spine_semantics, state_machine, temporal_pipe, ruling `q6_trigger_marker` | `only(Atom)` FIXTURES.md:21, 45-46; rulings.pl:35-39; temporal_pipe.pl is the heaviest user (its whole point per header) |
| `now_read` | check_eventing, engine_core, spine_semantics | `now(Tick)` FIXTURES.md:22; engine_core.pl header names it directly: "now() (R3)" |
| `pre_read` | merge_family, occurrence_identity, state_machine, ruling `r1_rider_pre_chains`, `r6_pre_visibility` | `pre(Atom)` FIXTURES.md:21; rulings.pl:70-73, 91-94 |
| `retention_clause` | check_eventing, engine_core, merge_family, occurrence_identity, operators, shell_stream, spine_semantics, state_machine, temporal_pipe, ruling `q10_retention` | `keep(Ref, all\|count(N))` FIXTURES.md:19; engine_core.pl `retention_count_prunes_oldest`/`log_without_retention_rejected`; rulings.pl:58-61 |
| `departure_form` | ruling `r4_departure` only | rulings.pl:84-89 — **no fixture file uses it** (see uncovered note below); the ruling itself is the only citable ground |

### Uncovered (10)

| construct | why uncovered, cited |
|---|---|
| `rel_decl` | expressions.pl:13-14, verbatim: "nowhere here needs a `rel_decl`/column type, so every `Decls` list is `[]`" — the reference engine has no HM/enum type checker and never exercises required-column-type declarations |
| `named_column_atoms` | no fixture file's `prog(Decls, Rules)` uses named-column rel-atom syntax anywhere; every rel call across all 12 fixture files is positional prolog functor application (`event_a(Item)`, `phase(Endpoint, fetching)`); the json braces literal (`{key: Value}`) is a distinct construct (`struct_decl`), not this one |
| `wildcard` | no fixture distinctly exercises `_` as the DSL wildcard construct; `_` appears only as ordinary Prolog anonymous-variable convenience in fixture authoring, which is not the same claim |
| `snapshot_ask` | FIXTURES.md's Expectations vocabulary (`final`/`deltas`/`ticks`/`throws`, lines 61-67) has no ask-snapshot expectation type at all |
| `from_world_modifier` | FIXTURES.md:79-84 ("Out of scope for fixtures... Model effect fills as canned scheduled arrivals") — world-fed rels are stood in for by canned arrival schedules, never the literal `from world` modifier syntax |
| `bind_decl` | same citation as above; the bind-at-link law means every fixture's "what a bind would deliver" is a canned row, never a `bind` declaration |
| `quoted_region` | FIXTURES.md:81, verbatim: "pattern/grammar matching (astgrep)... out of scope" |
| `grammar_import` | same citation; no fixture loads a grammar or a node-types.json fact set |
| `graph_operator_position` | `closure(` appears only in `conformance/engine.pl` and `conformance/level_eval.pl` (the interpreter's OWN implementation of the checker fold), never inside a fixture's `Rules` list |
| `departure_form` | spine_semantics.pl:25-27, verbatim: "Engine constraint honored throughout: no departed/gone body form exists yet (concurrently landing); every retraction here is graded only through `deltas(...)` on a level view, never a departed-body rule." The ruling (`r4_departure`) landed the same day but no fixture was updated to exercise it |

## 3. Self-checks added to ARCH.pl, wired into `go/0`

Three new `check/2` rows (ARCH.pl's `go/0` already `forall`s every `check/2`
fact, so no change to `go/0` itself was needed):

- `construct_status_closed` — every `construct/3` status is one of
  `{kept, respecified, new}`.
- `construct_tier_known` — every `construct/3` tier is one of
  `{t0,t1,t2,t3,t4}`.
- `covers_endpoints_ground` — every `covers/2` fact names a declared
  `construct/3` AND a subject that is either a `ruling/4` id (from
  `conformance/rulings.pl`, now `use_module`d by ARCH.pl) or an on-disk
  fixture file under `v6/prolog/conformance/fixtures/`.

All three PASS (receipt below), and the pre-existing four checks are
untouched.

## 4. arch_map.pl: the fifth atlas dimension

- New d2 container `budget: the construct budget { ... }`, one node per
  `construct/3` name.
- Coverage edges: `conformance.<fixture> -> budget.<construct>` or
  `ruling.<id> -> budget.<construct>` for every `covers/2` fact, reusing the
  EXISTING `conformance.*`/`ruling.*` node ids rather than duplicating them.
- Annotations: `# @ budget.NAME : tier T -- status S -- covered|UNCOVERED`,
  plus `# tag budget.NAME : T` (tier grouping) and, for the 10 uncovered
  constructs, an additional `# tag budget.NAME : uncovered` — the callout.
- New derived tour, `"the construct budget and its receipts"`: one step per
  construct, tier-ordered, `focus: "budget.NAME"`, note = tier, status, and
  the live list of covering subjects (or `UNCOVERED`). Computed straight from
  `construct/3`/`covers/2` at emit time, so a future cut or fixture
  promotion moves the tour with no hand edit.

## 5. Receipts

```
$ swipl -q -l v6/prolog/ARCH.pl -g go -g halt
PASS  sugar_grounds_out
PASS  species_are_four
PASS  graphs_refine_ast
PASS  roadmap_is_total
PASS  construct_status_closed
PASS  construct_tier_known
PASS  covers_endpoints_ground
```

```
$ swipl -q -l v6/prolog/tools/arch_map.pl -g emit -g halt
wrote /Users/chrishafley/projects/anim/atlases/sprefa-v6-arch.atlas.js
```

```
$ cd ~/projects/anim && npm run atlas:file atlases/sprefa-v6-arch.atlas.js
> anim@0.1.0 atlas:file
> node bin/atlas-file.mjs atlases/sprefa-v6-arch.atlas.js
/Users/chrishafley/projects/anim/atlases/sprefa-v6-arch.html  (10390 KB, standalone)
```

```
$ swipl -q -l v6/prolog/conformance/go.pl -g go -g halt | grep -c '^fail'
0
$ swipl -q -l v6/prolog/conformance/go.pl -g go -g halt | grep -c PASS
97
```

Conformance stays 97/97 — ARCH.pl's edits never touch anything the
conformance runner loads (`conformance/engine.pl`, `conformance/body.pl`,
`conformance/level_eval.pl`, `conformance/go.pl`, `conformance/fixtures/*.pl`
are all unmodified this arc).

## 6. Worktree note

This worktree (`agent-a2f4d9d196e91a1c9`) was created from commit `20d7d33a`
(2026-07-19), 263 commits behind the `v6` work this task depends on — the
`v6/prolog/` tree did not exist in the worktree at task start. It carried zero
unique commits of its own (`git log --oneline 45da3d2e..HEAD` was empty, and
`git merge-base 20d7d33a 45da3d2e` == `20d7d33a`), so it was fast-forwarded
(`git merge --ff-only 45da3d2e`) to the tip that has `v6/prolog/ARCH.pl`
before any of this work started. No commits were rewritten or discarded.
