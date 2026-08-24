# plan: split the large prolog files into hundreds-scale parts

## TOC

1. [What lands](#1-what-lands)
2. [The mechanism, copied from 0_generic_expand.pl](#2-the-mechanism-copied-from-0_generic_expandpl)
3. [The receipt every PR carries](#3-the-receipt-every-pr-carries)
4. [Base-sha gate measurements](#4-base-sha-gate-measurements)
5. [The tooling and where its output lives](#5-the-tooling-and-where-its-output-lives)
6. [Landing order and what blocks what](#6-landing-order-and-what-blocks-what)
7. Per-file cuts
   1. [print_dl.pl](#71-print_dlpl)
   2. [0_dot_expand.pl](#72-0_dot_expandpl)
   3. [0_type_plane.pl](#73-0_type_planepl)
   4. [compile.pl](#74-compilepl)
   5. [0_program_check.pl](#75-0_program_checkpl)
   6. [analyze.pl](#76-analyzepl)
   7. [compile/parse_dl_dcg.pl](#77-compileparse_dl_dcgpl)
   8. [ARCH.pl](#78-archpl)
   9. [emit_ts.pl](#79-emit_tspl)
   10. [lower.pl](#710-lowerpl)
8. [Everything else over 600 lines](#8-everything-else-over-600-lines)
9. [Open questions for the user](#9-open-questions-for-the-user)

---

## 1. What lands

Ten module files keep their name, their `:- module` line, their exports and
their imports. Each grows a folder named after the file, holding numbered parts
in pipeline order, pulled back in with `:- include`. No predicate moves between
modules, no export changes, no clause text changes, no clause order changes.

| file | now | parts | part lines | cross-part pairs | head kept | directives to hoist | predicate split across parts |
|---|---:|---:|---|---:|---|---:|---|
| `v6/prolog/lower.pl` | 7795 | 35 | 66-573 | 154 | 1..215 | 4 | none |
| `v6/prolog/emit_ts.pl` | 2786 | 15 | 50-482 | 34 | 1..37 | 0 | none |
| `v6/prolog/analyze.pl` | 1891 | 14 | 43-308 | 38 | 1..52 | 1 | none |
| `v6/prolog/compile/parse_dl_dcg.pl` | 1776 | 11 | 42-447 | 26 | 1..46 | 0 | `lex_token/2` |
| `v6/prolog/0_type_plane.pl` | 1037 | 6 | 99-284 | 15 | 1..66 | 0 | none |
| `v6/prolog/ARCH.pl` | 1026 | 6 | 56-288 | 2 | 1..149 | 5 | none |
| `v6/prolog/compile.pl` | 997 | 8 | 39-219 | 13 | 1..79 | 1 | none |
| `v6/prolog/0_program_check.pl` | 985 | 5 | 62-359 | 11 | 1..38 | 0 | `program_violation/3` |
| `v6/prolog/print_dl.pl` | 905 | 8 | 53-145 | 10 | 1..47 | 0 | none |
| `v6/prolog/0_dot_expand.pl` | 835 | 6 | 74-168 | 6 | 1..76 | 0 | none |

114 parts. Largest is 573 lines (`lower/28_fixpoint_ir.pl`), smallest 39
(`compile_pl/3_fixture_entry.pl`). Nothing over 700.

Both predicates that go discontiguous across parts already carry the directive
that permits it: `compile/parse_dl_dcg.pl:42` `:- discontiguous lex_token/2.`
and `0_program_check.pl:33` `:- discontiguous program_violation/3.`

---

## 2. The mechanism, copied from 0_generic_expand.pl

`v6/prolog/0_generic_expand.pl:6-28` keeps the module declaration and its
eleven exports, `:29-42` keeps every `use_module`, `:44-47` keeps the two ops
and the two `discontiguous` directives, and `:49-72` is nothing but twelve
`:- include('0_generic_expand/<part>.pl')` lines. The parts are numbered by
pipeline order: `0_expand`, `0a_type_apply_requests`, `0b_expansion_pipeline`,
`1_annotations`, `2_compiler_plane`, `3_enum_templates`, `4_type_views`,
`5_type_freeze`, `6_type_conformance`, `7_generic_instances`,
`8_type_rewrite`, `8a_key_wrappers`. Landed at `b5c5effa0`.

Every rule that shape implies, restated so an implementing lane does not have
to infer it:

| rule | why |
|---|---|
| parts are included in the same order the clauses had in the original file | `include` splices terms at the directive, so clause order is file order; a reordered include list changes first-solution semantics for `program_violation/3`, `lex_token/2`, `compiler_unsupported/3` and every other first-match predicate |
| a part carries clauses only, never a directive | see the hoist column above; each hoisted directive moves verbatim into the head, above the includes |
| `:- op` and `:- set_prolog_flag` stay in the head, above every include | later parts do not parse without them: `compile/parse_dl_dcg.pl:14` sets `back_quotes` to `codes` and `:22-28` declares `<-`, `<+`, `:=` and the `# @ ~` sigils |
| the folder is named after the file, minus `.pl` | Chris's word, and the precedent |
| a part file has no module line and no `use_module` | it is not a module; it is text spliced into one |

Measured hazard, `prolog_load_context/2` under `include`:

```
step 0  main.pl                :- prolog_load_context(directory, D)  -> /tmp/lctest
step 1  main.pl                :- include('parts/p.pl')
step 2  parts/q.pl  DIRECTIVE  :- prolog_load_context(directory, D)  -> /tmp/lctest/parts
step 3  main.pl                :- prolog_load_context(directory, D)  -> /tmp/lctest
```

A directive that runs inside an included file reports the INCLUDED file's
directory. `ARCH.pl:603` is exactly that directive and it pins `arch_dir/1`,
which `covers_endpoint_exists/1` (`ARCH.pl:605-612`) resolves fixture paths
against. Hoisting it into the head is not cosmetic, it is the difference
between `arch_dir('.../v6/prolog')` and `arch_dir('.../v6/prolog/ARCH')`.
Nothing else in the ten targets calls `prolog_load_context/2`.

The organization gate tolerates this shape, measured rather than assumed.
`v6/prolog/tools/prolog_lint.pl:54-61` walks `v6/prolog/**/*.pl` recursively,
so it already reads all twelve `0_generic_expand/` parts, and produces zero
findings against them. Its duplicate-module check only records files that
declare a module (`prolog_lint.pl:107-109`), so part files never take part in
it, and the undefined/cross-module checks run over the LOADED database
(`prolog_lint.pl:88-99` versus `lint_loaded/1`), where an included part has
already dissolved into its module.

---

## 3. The receipt every PR carries

Four gates, plus one structural check that is stronger than any of them.

**The structural check.** `plans/2026-08-24-prolog-split/modsnap.pl` loads a
module and prints every clause of every predicate the module itself defines,
through `portray_clause/1`, predicates sorted, clauses in source order. Source
positions never reach `portray_clause`, so an include-only split that preserves
clause order prints byte-identical output. Run it on the file before the edit
and after, in the same worktree, and diff:

```bash
P=plans/2026-08-24-prolog-split
swipl -g main -t halt $P/modsnap.pl -- v6/prolog/print_dl.pl print_dl > /tmp/before.listing
# ... apply the split ...
swipl -g main -t halt $P/modsnap.pl -- v6/prolog/print_dl.pl print_dl > /tmp/after.listing
diff /tmp/before.listing /tmp/after.listing && echo LISTING-IDENTICAL
```

Measured on the base sha: deterministic across two runs
(`print_dl` -> `2df299e418371079bd39a482fdebce30dfec6c36` twice), and it reads
the already-split module fine (`generic_expand` -> 4346 lines, `lower` -> 8571,
`parse_dl_dcg` -> 2745, `analyze` -> 1676).

Module name per target: `lower`, `emit_ts`, `analyze`, `parse_dl_dcg`,
`type_plane` (`0_type_plane.pl:6`), `compile`, `program_check`, `print_dl`,
`dot_expand`. `ARCH.pl` declares no module; pass `user` and accept that the
dump also carries `modsnap.pl`'s own predicates and the SWI hook stubs, which
are constant between the two runs.

**The four gates**, each with its base-sha number:

```bash
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 445 PASS, 0 FAIL
cd v6 && just plunit                                       # passed=1115 failed=0
bash v6/sprefa-engine-rs/grade.sh                          # graded=445 byte-clean=341
swipl -g go -t halt v6/prolog/ARCH.pl                      # 7 PASS, 0 FAIL
```

**The two digest files the brief names do not exist on a clean tree.**
`v6/prolog/compile/out/sweep.digests` is gitignored at `.gitignore:76` and is
minted by `v6/prolog/sweep.pl:66` `digest_store_path/1` when a sweep runs;
`compile/out/oracle.digests` is minted by `compile/oracle_dump.pl:28-33`.
Neither is present after a fresh checkout, so "unchanged" has nothing to
compare against. For `emit_ts.pl` the substitute receipt is a content manifest
over the emitted TypeScript, taken before and after in one worktree:

```bash
find v6/prolog/compile/out -name '*.ts' | LC_ALL=C sort | xargs shasum > /tmp/ts.before
# ... apply the split ...
find v6/prolog/compile/out -name '*.ts' | LC_ALL=C sort | xargs shasum > /tmp/ts.after
diff /tmp/ts.before /tmp/ts.after && echo TS-BYTE-IDENTICAL
```

If a sweep has already run in that worktree, add `sweep.digests` and
`oracle.digests` to the same before/after diff; they are then real files and
the comparison is exact.

---

## 4. Base-sha gate measurements

All four measured on `9e4b468157bb2a189960b8ec69daad10af372862` in this
worktree, each once, before any planning.

| gate | command | result |
|---|---|---|
| conformance | `cd v6/prolog/conformance && swipl -g go -t halt go.pl` | `445` PASS, `0` FAIL |
| plunit | `cd v6 && just plunit` | `PLUNIT jobs=12 declared=1069 results=1115 passed=1115 failed=0 timeout=0 wall=5.04s` |
| rust grade | `bash v6/sprefa-engine-rs/grade.sh` | `RUST-GRADE graded=445 byte-clean=341` |
| arc gate | `swipl -g go -t halt v6/prolog/ARCH.pl` | `7` PASS, `0` FAIL |

One extra leg measured because the split touches every file it reads:

| gate | result | status |
|---|---|---|
| prolog-lint | `PROLOG_LINT findings=18 baseline=0 FAIL` | allowed red at `.github/CI-KNOWN-RED.md:150`, but that row records `findings=14` at `:117`, so the allowlist is stale by four |

The four new findings are all `private_cross_module_call` out of plunit test
modules: `emit_rust:ir_version/1`, `emit_ts:ir_version/1`,
`executor_modules:claimed_by/3`, `generic_expand:compiler_derived_relation_shapes/2`,
`generic_expand:elaborate_compiler_rules/5`, `parse_dl_dcg:dotted_path/3`.
None of them is a split concern and none is this lane's to fix; the row needs
re-measuring by whoever owns the allowlist. Every split PR must leave
`findings=18` exactly, not 19.

---

## 5. The tooling and where its output lives

Everything under `plans/2026-08-24-prolog-split/`.

| file | what it does |
|---|---|
| `predmap.pl` | reads one source with `read_term/3` + `subterm_positions`, so heads come from SWI's own parser and never a regex; character offsets become lines through a binary search over the file's newline table; executes the file's own `op/3` and `set_prolog_flag/2` as it reads, because later terms do not parse otherwise; emits JSON with every term, every predicate's clause spans, and the in-file functors each body mentions |
| `partition.py outline <predmap>` | prints the ordered predicate list with line spans and clause counts; this is what the cuts were designed against |
| `partition.py report <cuts>` | resolves anchors to line ranges, assigns every clause term to a part, and prints the part table, the over-700 check, the split-predicate list, the hoist list, the cross-part call edges and the ownership table |
| `cuts/<name>.cuts.json` | the cut itself: folder, module, and per part a file name, an ANCHOR predicate and one sentence of ownership |
| `reports/<name>.md` | the graded output, one per target |
| `receipts.md` | all ten reports concatenated |
| `heads.md` | the exact include block for each module head |
| `modsnap.pl` | the byte-identical receipt described in section 3 |
| `run.sh` | regenerates every predmap, every report and `receipts.md` from the tree |
| `<name>.predmap.json` | the raw structural map, checked in so a reviewer can re-derive any number in this plan |

A part is anchored by the name/arity of its FIRST predicate, never by a hand
typed line number. `partition.py` derives the range, so a cut cannot drift out
of agreement with the source, and re-running `run.sh` on a changed tree
re-grades the whole plan.

Two limits stated rather than hidden. `calls[]` is functor-name matching over
the whole clause body, arguments included, so a data term wearing a predicate's
functor reads as a call: the edge tables orient a reader and are not a call
graph oracle. And `partition.py` grades a cut, it does not perform one; the
actual file surgery is a lane's `sed`-range work checked by `modsnap.pl`.

---

## 6. Landing order and what blocks what

One PR per file. Smallest risk first, `lower.pl` last.

| # | file | why it sits here | blocked by |
|---:|---|---|---|
| 1 | `print_dl.pl` | 8 parts, max 145 lines, zero straddles, zero directives to hoist; only the text door printer reads it | nothing |
| 2 | `0_dot_expand.pl` | 6 parts, max 168, zero straddles, zero hoists | nothing |
| 3 | `0_type_plane.pl` | 6 parts, max 284, zero straddles, zero hoists | nothing |
| 4 | `compile.pl` | 8 parts, one `meta_predicate` to hoist, and the ONE folder-name collision | nothing; needs the user's word on the folder name, see section 9 |
| 5 | `0_program_check.pl` | 5 parts, `program_violation/3` goes discontiguous on purpose | nothing |
| 6 | `analyze.pl` | 14 parts, one `:- table` to hoist | nothing |
| 7 | `compile/parse_dl_dcg.pl` | 11 parts, `lex_token/2` goes discontiguous, and four ops plus a `back_quotes` flag must sit above every include | nothing |
| 8 | `ARCH.pl` | 6 parts, five directives to hoist, one of which is the `prolog_load_context` hazard | nothing |
| 9 | `emit_ts.pl` | LOWEST priority. The TypeScript door is paused (user 2026-08-21) and takes no new work; the split adds no feature but does need the emitted-bytes receipt of section 3 | do it only if the user wants it; the door being paused is an argument for leaving the file alone entirely |
| 10 | `lower.pl` | 35 parts, the biggest surgery, and the file most likely to be edited under you | Chris's main tree has uncommitted `lower.pl` work; this PR opens only after that commits |

**The temporal-v2 lane holds none of these files.** The codex lane at
`/private/tmp/sprefa-temporal-v2` (branch `feature/temporal-relations-v2`) is
dirty on `0_compiler_relations.pl`, `0_generic_expand.pl`,
`0_generic_expand/0_expand.pl`, `0_generic_expand/2_compiler_plane.pl`,
`0_generic_expand/5_type_freeze.pl`, `0_unsupported_messages.pl`,
`compile/test/4_braced_nested_relations.test.pl`,
`compile/test/plunit_tests.pl` and `typegen_golden.sh`. Intersection with the
ten targets: EMPTY. No target is sequenced after temporal-v2.

The one file the two plans do touch in common is
`compile/test/plunit_tests.pl`, and only in section 8's advisory sense: any
split of it waits for temporal-v2 to merge. Every split PR does have to keep
`plunit` at 1115/0, and a temporal-v2 merge that moves that number moves the
receipt for whichever PR is open at the time. Re-measure the base number after
each merge rather than quoting this document.

Chris's main tree also has uncommitted `0_generic_expand.pl` work. That file is
not a target, it is the precedent, so nothing here waits on it.

---

## 7. Per-file cuts

Every part table in this section is generated, not typed. The generated form,
including the cross-part call edges and the per-part ownership sentences, lives
at `plans/2026-08-24-prolog-split/reports/<name>.md`, concatenated into
`receipts.md`. The include block for each head lives at `heads.md`.

### 7.1 print_dl.pl

Head keeps lines 1..47: `:- module(print_dl, [...])` at `print_dl.pl:21-28`,
six `use_module` lines at `:30-40`, three ops at `:42-44`. Zero directives to
hoist, zero stray clauses.

| part | lines | span | owns |
|---|---:|---|---|
| `0_entry.pl` | 124 | 48-171 | the entry points and the block join that assembles a printed program |
| `1_decl_order.pl` | 91 | 172-262 | EDB decl synthesis for the text door and the declaration ordering it feeds |
| `2_decl_line.pl` | 129 | 263-391 | the rel decl line: arrow columns, modifiers, type applications and template columns |
| `3_column_types.pl` | 145 | 392-536 | printing a column type, annotations, decl columns and enum, product and sum fields |
| `4_rule_and_query.pl` | 75 | 537-611 | rule lines, query lines with their order tails, and match arms |
| `5_body.pl` | 113 | 612-724 | the body, one goal per indented line, surface wrappers and host input interleaving |
| `6_term.pl` | 129 | 725-853 | the general term printer: vars, ints, atoms, dot chains, lists and json |
| `7_braces_and_quoting.pl` | 53 | 854-906 | brace pairs and the always-explicit quoting |

Cuts land on the file's own banners at `:46`, `:74`, `:256`, `:535`, `:610`,
`:721`, `:887`. No predicate straddles. 10 directed part pairs.

### 7.2 0_dot_expand.pl

Head keeps lines 1..76: module at `0_dot_expand.pl:51-62`, five `use_module` at
`:64-71`, three ops at `:73-75`. Zero hoists, zero straddles.

| part | lines | span | owns |
|---|---:|---|---|
| `0_qualified_types.pl` | 147 | 77-223 | the entry point, qualified type path resolution, minted type decls and enum arm refs |
| `1_rel_paths.pl` | 140 | 224-363 | rel path rewriting, the decl scope tree and the path collision check |
| `2_nested_captures.pl` | 150 | 364-513 | nested parent refs, capture shapes, parent column insertion and the capture rule, arrow and head forms |
| `3_capture_body.pl` | 74 | 514-587 | parent atoms inside a body and the captured body rewrite |
| `4_dot_rules.pl` | 168 | 588-755 | desugaring a dot rule, rewriting head and goals, replacing dot gets and checking the receiver |
| `5_body_vars.pl` | 81 | 756-836 | bound body variables, binding positions and conjunction/goal-list conversion |

This file carries no section banners, so the cuts are derived from the
predicate order alone. 6 directed part pairs, the loosest coupling of the ten.

### 7.3 0_type_plane.pl

Head keeps lines 1..66: module at `0_type_plane.pl:30-57`, four `use_module` at
`:59-62`. No ops, no `discontiguous`, no hoists, no straddles.

| part | lines | span | owns |
|---|---:|---|---|
| `0_definitions.pl` | 129 | 67-195 | type definitions, declared type names, column storage, list element types and the wrapper unwrapping |
| `1_relation_shape.pl` | 122 | 196-317 | ref columns, relation columns and their types, and a relation value as a term or an object |
| `2_type_order.pl` | 99 | 318-416 | the topological order over declared types, the cycle witness, and type and field shape errors |
| `3_canonicalize.pl` | 157 | 417-573 | world row canonicalization, reference target normalization, and canonical struct and field values |
| `4_row_violations.pl` | 181 | 574-754 | row shape violations, position column names, the wide integer witness and column value shape errors |
| `5_type_json.pl` | 284 | 755-1038 | ref column names, type field values and the canonical json renderer with its js float formatting |

### 7.4 compile.pl

Head keeps lines 1..79: module at `compile.pl:8-28`, a `thread_local` at `:35`,
`:- set_prolog_flag(encoding, utf8)` at `:39`, thirty `use_module` at `:41-70`,
three ops at `:72-74`, one `meta_predicate` at `:76`.

| part | lines | span | owns |
|---|---:|---|---|
| `0_fixtures.pl` | 58 | 80-137 | reading one fixture term out of a fixture file, and finding a fixture by name |
| `1_program_plan.pl` | 219 | 138-356 | `program_plan/3`, the one term `lower.pl` and `emit_ts.pl` both read, plus the compiler-type-rule partition, reference-target materialization and the plan debug dumps |
| `2_reserved_namespace.pl` | 47 | 357-403 | the compiler-owned `__` namespace: which names are reserved and what a violation reads as |
| `3_fixture_entry.pl` | 39 | 404-442 | the `compile_fixture` entry points, world shape checks and the single-arity-per-name check |
| `4_storage_names.pl` | 203 | 443-645 | shape identity and storage naming: shape digests, declaring-module stems, ascii folding and unique suffix allocation |
| `5_dl6_door.pl` | 124 | 646-769 | the `.dl6` text door: emitter and schedule options, arrival terms, seeded forms and the fact partition |
| `6_program_phases.pl` | 99 | 770-868 | `compile_program` and the phase pipeline that runs parse, lower, boot and emit, and writes the compiled output |
| `7_phase_trace.pl` | 130 | 869-998 | phase measurement, the per-phase debug hooks and the compile trace file |

Hoist: `compile.pl:317` `:- meta_predicate check_step(+,0).` moves into the
head. A `meta_predicate` declaration must precede the clauses it governs, and
`check_step/2` is defined at `:321-323` inside `1_program_plan.pl`.

**Folder-name collision, the one deviation.** `v6/prolog/compile/` already
exists and holds `parse_dl_dcg.pl`, `registry.pl`, `test/`, `out/` and more, so
`compile.pl` cannot own a folder named `compile`. The tables above use
`v6/prolog/compile_pl/`. See section 9.

### 7.5 0_program_check.pl

Head keeps lines 1..38: module at `0_program_check.pl:9-17`, five `use_module`
at `:19-28`, two ops at `:30-31`, and `:- discontiguous program_violation/3.`
at `:33`.

| part | lines | span | owns |
|---|---:|---|---|
| `0_lookups.pl` | 62 | 39-100 | `first_violation/3` and the small decl readers the violation clauses all call |
| `1_violations_decls.pl` | 312 | 101-412 | the violation clauses about declarations and patterns, with the cst regexp and ast capture helpers only they use |
| `2_violations_rules.pl` | 359 | 413-771 | the violation clauses about rules, reserved carriers and column type conflicts |
| `3_aggregates_and_types.pl` | 104 | 772-875 | numeric aggregate operands, the implemented aggregate roster, declared column type uses and the rule atom readers |
| `4_column_variables.pl` | 111 | 876-986 | the declared column table, head and body column variables, storage assignability and relation argument violations |

`program_violation/3` has 38 clauses spread over `:101-770` with helper
predicates interleaved at `:230`, `:275-333`, `:365-374` and `:413-420`. That
is why `:- discontiguous program_violation/3.` is already at `:33`. The cut
puts clauses 1..26 in `1_violations_decls.pl` and clauses 27..38 in
`2_violations_rules.pl`, in file order, so `first_violation/3` at `:39-42`
still finds the same first solution.

Alternative if the user prefers one predicate in one file: a single part
spanning `:101-771`, 671 lines. Under the 700 cap, no discontiguity introduced,
and outside the 200-500 target band. The plan takes the two-part cut; the
one-part cut is a one-line edit to `cuts/0_program_check.cuts.json`.

### 7.6 analyze.pl

Head keeps lines 1..52: module at `analyze.pl:7-25`, ten `use_module` at
`:27-46`, three ops at `:48-50`.

| part | lines | span | owns |
|---|---:|---|---|
| `0_rel_and_rule_shape.pl` | 58 | 53-110 | rel kind, key and keep readers, edge-vs-level rule shape, and the headed/derived ref lists |
| `1_body_walk.pl` | 79 | 111-189 | walking a rule body for the refs it uses, the coalesce output resolution and the event use rows |
| `2_guard_goals.pl` | 99 | 190-288 | guard and bind goal classification, tick goals, and whether a program mentions tick or the catalog |
| `3_ref_inventory.pl` | 43 | 289-331 | the program-wide ref inventory: seeded, declared, arrival-target and all program refs |
| `4_column_names.pl` | 133 | 332-464 | column naming from surface variable identity, ref occurrence args and snake-case folding |
| `5_literal_types.pl` | 94 | 465-558 | column type read off a declaration, and the type a concrete literal witnesses |
| `6_program_types.pl` | 109 | 559-667 | the driver for program-wide column typing and the seed-row contributions it starts from |
| `7_type_fixpoint.pl` | 168 | 668-835 | the contribution fixpoint over rule heads, and the body type environment a rule's goals bind |
| `8_expression_types.pl` | 176 | 836-1011 | typing an expression, arithmetic result types, and merging two contributions into one column type |
| `9_edge_shape.pl` | 175 | 1012-1186 | edge trigger shape: sampled goals, departure goals and the goals an edge body cannot carry |
| `10_edge_head_types.pl` | 59 | 1187-1245 | edge head column-type consistency across the rules writing one rel |
| `11_subset_gate.pl` | 201 | 1246-1446 | the subset gate: every construct the compiler has not built yet, with the reason term each throws |
| `12_rule_observers.pl` | 138 | 1447-1584 | which rules read which rel, self-read one-pass closure, and the edge rule shape check |
| `13_shape_checks.pl` | 308 | 1585-1892 | reserved constructs in a body, head conflict risk, compound patterns on arrival rels, and the level and aggregate rule shape checks |

Hoist: `analyze.pl:109` `:- table body_ref_uses/2.` moves into the head. A
`table` directive must precede the tabled predicate's clauses, which are at
`:113-120` inside `1_body_walk.pl`.

### 7.7 compile/parse_dl_dcg.pl

Head keeps lines 1..46: module at `parse_dl_dcg.pl:1-11`,
`:- set_prolog_flag(back_quotes, codes).` at `:14`, three `use_module` at
`:17-19`, four ops at `:22-28`, `thread_local` at `:30-33`, and both
`discontiguous` directives at `:42-43`.

The flag and the ops are the reason the head order matters more here than
anywhere else. Every part after `2_lexer.pl` uses `` ~`use` ``,
`` @`?` ``, `` #`(` `` and backquoted code lists; none of it parses if the
head's `:14` and `:22-28` do not run before the first include.

| part | lines | span | owns |
|---|---:|---|---|
| `0_cst_shapes.pl` | 62 | 47-108 | the editor CST shape and origin tables, and the thread-local recorders the passes write into |
| `1_entry.pl` | 271 | 109-379 | the four entry points, the two-pass driver, parse marks, line/column reporting for a reason, statement source refs, and host path flattening |
| `2_lexer.pl` | 138 | 380-517 | whitespace and comments, the `@ ~ #` sigil operators, identifiers, int/float/atom/string literals, escape decoding, and variable holes |
| `3_use_and_router.pl` | 56 | 518-573 | use/import items and `statement//5`, the router that picks rel, query, match or rule |
| `4_rel_decl.pl` | 447 | 574-1020 | the whole rel declaration grammar: nested rels, arrival tails, generic parameters, interfaces, type expressions, enums, keep/key clauses and the decl-b column tail |
| `5_name_resolution.pl` | 115 | 1021-1135 | the post-parse name passes: module path collisions, reserved names, minted names, relation-value decl normalization |
| `6_host_and_template.pl` | 42 | 1136-1177 | the removed `sh`/`bind` statements, host output column specs, and template literals |
| `7_query_and_match.pl` | 63 | 1178-1240 | the `?` query statement with its order tail, and match statements with their arms |
| `8_rule_and_args.pl` | 153 | 1241-1393 | rule statements, head atoms, and named/positional argument resolution including keyword puns |
| `9_body.pl` | 200 | 1394-1593 | rule bodies: body items, cst query items, balanced-bracket scanning, rel atom terms and infix items |
| `10_expr.pl` | 184 | 1594-1777 | the arithmetic tier expression grammar, json literals, dotted and slash paths, brace terms and list terms |

`lex_token/2` splits across `2_lexer.pl` (clauses at `:475`, `:476`, the quoted
and atom literal patterns) and `6_host_and_template.pl` (the clause at `:1164`,
the template literal pattern). The comment at `:41` says the rows sit beside
their decoders on purpose, and `:- discontiguous lex_token/2.` at `:42` is
already in the head.

`type_base/3` and `type_argument/3` interleave at `:853-886` and both land
inside `4_rel_decl.pl`, so no new discontiguity there;
`:- discontiguous type_base/3.` at `:43` stays regardless.

### 7.8 ARCH.pl

`ARCH.pl` declares no module. Head keeps lines 1..149, which is the file's
prose header plus the `use_module` at `:145`. It is data, not code:
`task/3` with 265 rows, `covers/2` with 115, `construct/3` with 38 and
`fork/5` with 16. The brief's `task/5` spelling is stale; the rows are
`task(Name, Status, Deps)`, sampled at `ARCH.pl:684-690`.

| part | lines | span | owns |
|---|---:|---|---|
| `0_species.pl` | 218 | 150-367 | the graph, refines, species, algorithm, prior_art, capability, tech and technique rows |
| `1_constructs.pl` | 94 | 368-461 | the construct roster with its status and tier vocabularies |
| `2_covers.pl` | 156 | 462-617 | which construct each endpoint covers, and the endpoint existence check |
| `3_forks.pl` | 66 | 618-683 | the open design fork rows |
| `4_tasks.pl` | 288 | 684-971 | the task rows |
| `5_gate.pl` | 56 | 972-1027 | roadmap, topsort, the check rows and `go/0` |

Splitting by arc family instead of by relation was considered and rejected. The
arcs are named in `task/3`'s first argument and in `covers/2`, and a
family-based cut would put `task` rows and their matching `covers` rows in one
part, which reads well until `topsort/3` (`:979-983`) walks the whole task
graph and `check/2` (`:989-1002`) quantifies over every row. The relation-based
cut keeps each table whole, which is what the gate's own predicates assume.

Five directives to hoist, and one of them is the sharp one:

| line | directive | why it must move |
|---|---|---|
| 315 | `:- use_module('src/kernel.pl')` | imports must be in the head |
| 366 | `:- use_module('conformance/rulings.pl')` | same |
| 602 | `:- dynamic arch_dir/1.` | must precede `asserta/1` on it |
| 603 | `:- prolog_load_context(directory, Dir), asserta(arch_dir(Dir)).` | inside an include it reports `.../v6/prolog/ARCH`, not `.../v6/prolog`, and `covers_endpoint_exists/1` at `:605-612` builds fixture paths off it; measured, section 2 |
| 1007 | `:- use_module('src/grader', [run/1])` | imports must be in the head |

The `:145`, `:315`, `:366` and `:1007` `use_module` lines all collapse into one
block at the top of the head, in that order.

### 7.9 emit_ts.pl

LOWEST priority. The TypeScript door is paused (user 2026-08-21: no new
features, no new tests, no sweep runs, output byte-identical for unchanged
programs). The split adds nothing the door needs. Planned because the brief
asks for it; the recommendation is to leave the file alone until the door
un-pauses.

Head keeps lines 1..37: module at `emit_ts.pl:4-8`, `use_module` at `:10-30`,
two ops at `:32-33`. **Two stray clauses sit in the head region**:
`bind_executor/2` at `:27-28`, wedged between the `use_module` at `:24` and the
one at `:29`. They stay in the head file, in place. Moving them into a part
would change nothing semantically, and leaving them costs two lines.

| part | lines | span | owns |
|---|---:|---|---|
| `0_text_helpers.pl` | 141 | 38-178 | the IR version and the js template, string, identifier and case text helpers |
| `1_header_and_imports.pl` | 113 | 179-291 | the emitted file's header comment and every import line |
| `2_value_plane.pl` | 190 | 292-481 | the declared value plane: struct and enum type plans, ref column maps, identity tables and the normalize lines |
| `3_local_types.pl` | 181 | 482-662 | local helper types, the world plan and the host plan json |
| `4_bind_and_query.pl` | 91 | 663-753 | bind config literals and the query plan json |
| `5_arrival_gate.pl` | 152 | 754-905 | the bind args helper, the arrival value type gate and the trigger occurrence helper |
| `6_catalog.pl` | 188 | 906-1093 | ddl entries, rel columns, physical names, raw and declared column types, and the catalog rows |
| `7_snapshot.pl` | 127 | 1094-1220 | boot entries, the snapshot type and its two readers, and `final_select` |
| `8_arrivals.pl` | 50 | 1221-1270 | arrival statements and the function that runs them |
| `9_incremental_plans.pl` | 482 | 1271-1752 | incremental relation plans: edge and level statements, retention, refCount and dred sql, and the fixpoint IR text |
| `10_ordered_loop.pl` | 350 | 1753-2102 | the ordered pre-occurrence loop with its carry, arm and departure lines |
| `11_level_recompute.pl` | 114 | 2103-2216 | level recompute and the row-count sql it reads |
| `12_deltas_and_tick.pl` | 171 | 2217-2387 | `build_deltas`, snapshot retention, the ordered tick function and the incremental mode lines |
| `13_prune.pl` | 248 | 2388-2635 | subscribe-cone pruning, plan export, `advance_tick` and the incremental tick dispatch |
| `14_top_level.pl` | 152 | 2636-2787 | `emit_program/5` and the two statement classifiers |

Zero straddles, zero directives to hoist, 34 directed part pairs. Extra
receipt: the emitted-TypeScript content manifest of section 3.

### 7.10 lower.pl

LAST. Blocked until Chris's uncommitted `lower.pl` work in the main tree
commits.

Head keeps lines 1..215: module at `lower.pl:116-176`, seventeen `use_module`
at `:178-201`, three ops at `:203-205`, and `:- thread_local` at `:214`.

| part | lines | span | owns |
|---|---:|---|---|
| `0_storage_context.pl` | 204 | 216-419 | the thread-local storage context, frontier mode, shared frontier ids and DDL, every table name and the sql quoting |
| `1_pattern_args.pl` | 94 | 420-513 | the pattern-argument compiler for level rule bodies |
| `2_positive_uses.pl` | 236 | 514-749 | positive body-atom compilation: joins, coalesced uses, FROM parts, old-state reads and seeded pre uses |
| `3_negative_uses.pl` | 88 | 750-837 | NOT EXISTS compilation and the coalesce recount markers |
| `4_head_expr.pl` | 415 | 838-1252 | head expression compilation, shared by both rule kinds, and the expression-lift guard and bind goals |
| `5_catalog_ddl.pl` | 182 | 1253-1434 | the catalog DDL contract, set-rel tables and keys, option-some tables, acyclic guards and the rel and rule hashes |
| `6_catalog_rows.pl` | 77 | 1435-1511 | the catalog row entry points and the type row families |
| `7_catalog_planes.pl` | 347 | 1512-1858 | the plane rows: rel, departure, pre, view, dict, level and port planes, plus the storage rows |
| `8_catalog_decls.pl` | 72 | 1859-1930 | decl rows and type metadata rows |
| `9_semantic_ids.pl` | 275 | 1931-2205 | semantic type ids and every metadata row family they annotate |
| `10_module_rels.pl` | 104 | 2206-2309 | catalog rel plans and the per-module rel column view |
| `11_module_map.pl` | 82 | 2310-2391 | spliced module rows, the rel-to-module map and module edge rows |
| `12_catalog_lists.pl` | 117 | 2392-2508 | list type rows, the list and rel id maps and the rel rows |
| `13_catalog_paths.pl` | 158 | 2509-2666 | rel scope, the path tree and room rows, column rows and the catalog text sql |
| `14_guards_and_comparisons.pl` | 99 | 2667-2765 | one guard goal, regexp goals, and comparison sql with its no-coercions type check |
| `15_head_select.pl` | 123 | 2766-2888 | the head select list and the intern write statements it splits out |
| `16_interning.pl` | 210 | 2889-3098 | intern mode, text constants in the id space, the decode view and the ingest door's intern plan |
| `17_ddl.pl` | 124 | 3099-3222 | the relation DDL |
| `18_dictionaries.pl` | 367 | 3223-3589 | dictionary tables, relation reference projection and `decode/2` as a dictionary join |
| `19_relation_values.pl` | 371 | 3590-3960 | relation-value terms lowered as dictionary joins |
| `20_arrivals.pl` | 111 | 3961-4071 | the arrival statement templates |
| `21_edge_rules.pl` | 353 | 4072-4424 | edge rule lowering |
| `22_level_rules.pl` | 85 | 4425-4509 | level rule lowering and its statement groups |
| `23_avg_accumulator.pl` | 319 | 4510-4828 | the incremental avg accumulator: its seed, body and delta rows, scoped inserts and deletes |
| `24_aggregate_scope.pl` | 189 | 4829-5017 | the aggregate scope table: its DDL, seed sql, scoped insert and delete, and the accumulator columns |
| `25_ref_counts.pl` | 189 | 5018-5206 | refCount sql, the refCount plan, frontier staging and the counted and recursive seeds |
| `26_expand.pl` | 66 | 5207-5272 | the level expand plan with its seed, hop and absorb sql |
| `27_dred.pl` | 266 | 5273-5538 | in-place recursive-head maintenance |
| `28_fixpoint_ir.pl` | 573 | 5539-6111 | the backend-neutral fixpoint IR |
| `29_json_decode.pl` | 258 | 6112-6369 | `decode/2` over a json column, lowered to json1 sql |
| `30_aggregate_heads.pl` | 519 | 6370-6888 | aggregate heads |
| `31_deltas_and_order.pl` | 462 | 6889-7350 | the per-rel delta statements and the `?` order tails |
| `32_boot.pl` | 111 | 7351-7461 | boot seeding |
| `33_top_level.pl` | 174 | 7462-7635 | `lower_program/2` and the plan term it returns |
| `34_write_verbs.pl` | 161 | 7636-7796 | the six write verbs |

Twenty-nine of the thirty-five cuts land on the file's own section banners
(`lower.pl:207`, `:353`, `:408`, `:511`, `:748`, `:803`, `:1221`, `:1250`,
`:2884`, `:2921`, `:3000`, `:3051`, `:3090`, `:3213`, `:3458`, `:3519`,
`:3959`, `:4041`, `:4403`, `:4477`, `:5270`, `:5535`, `:6058`, `:6344`,
`:6874`, `:6922`, `:7339`, `:7460`, `:7627`). Two banner sections exceed 700
lines on their own and are sub-cut at predicate boundaries:

| banner section | lines | sub-cut into |
|---|---:|---|
| `:1250` the program catalog scaffold | 1634 | `5_catalog_ddl` through `15_head_select`, eleven parts, 72-347 lines |
| `:4477` group-scoped aggregate maintenance | 793 | `23_avg_accumulator` + `24_aggregate_scope`, 319 + 189, plus `25_ref_counts` and `26_expand` which the banner also covered |

Four directives to hoist, all in the `0_storage_context.pl` span:

| line | directive |
|---|---|
| 230 | `:- thread_local frontier_mode_option/1.` |
| 231 | `:- thread_local shared_frontier_relation_id_fact/2.` |
| 233 | `:- meta_predicate with_frontier_mode(+,0).` |
| 234 | `:- meta_predicate with_shared_frontier_ids(+,0).` |

Zero predicates straddle. 154 directed part pairs, the densest of the ten,
which is the expected shape for a 7795-line file with one shared sql-text
vocabulary; `reports/lower.md` names every edge.

---

## 8. Everything else over 600 lines

`find v6/prolog -name '*.pl' -not -path './compile/out/*' | xargs wc -l` on the
base sha, rows above 600 that are not one of the ten targets.

| file | lines | verdict |
|---|---:|---|
| `compile/test/plunit_tests.pl` | 11504 | SPLIT IT, and it is the biggest win in the tree, but not on this plan's terms: it is a plunit file, so the unit is `:- begin_tests/end_tests`, not a predicate family, and a `.test.pl` sibling set already exists in `compile/test/`. Blocked on temporal-v2, which holds it dirty. Its own PR, its own plan. |
| `conformance/rulings.pl` | 879 | LEAVE. It is the decision log and CLAUDE.md points every agent at it as one readable file; splitting it costs more than it saves. |
| `compile/test/type_relation_ir.test.pl` | 802 | LEAVE for now, same reasoning as `plunit_tests.pl`: test files split by `begin_tests` block, and that is a different plan. |
| `compile/8_emit_rust_types.pl` | 769 | SPLIT, second wave. The Rust door is the live one, so it will keep growing; four to five parts at 150-200 lines. |
| `conformance/engine.pl` | 757 | SPLIT, second wave. Same shape as the ten. |
| `compile/6_isolated_compiler_dd.pl` | 752 | SPLIT, second wave. |
| `compile/registry.pl` | 717 | LEAVE. It is a roster of primitives, one row each, and a reader wants the whole roster in one buffer. |
| `emit_rust.pl` | 709 | SPLIT, second wave, and it should be first of the second wave: the Rust door takes all new work, so this file is the one most likely to cross 1000. |
| `compile/test/compiler_relations.test.pl` | 707 | LEAVE, test file. |
| `compile/test/3_clock_check.test.pl` | 698 | LEAVE, test file. |
| `conformance/fixtures/6_relation_depth.pl` | 674 | LEAVE. Fixtures are read whole by `go.pl`. |
| `0_generic_expand/1_annotations.pl` | 649 | LEAVE. It is already a part, and it is the largest one the precedent produced. If a lane wants a 700 ceiling everywhere, this is the one file that argues for 500 instead. |
| `conformance/fixtures/timeless_rail.pl` | 632 | LEAVE, fixture. |
| `conformance/body.pl` | 620 | BORDERLINE. Three parts would fit; low value while it sits at 620. |
| `1_host_expand.pl` | 620 | BORDERLINE, same. |

Second wave, if the user wants one: `emit_rust.pl`, `compile/8_emit_rust_types.pl`,
`conformance/engine.pl`, `compile/6_isolated_compiler_dd.pl`. Four files, same
mechanism, same receipts, ~2987 lines total.

---

## 9. Open questions for the user

1. **`compile.pl`'s folder name.** `v6/prolog/compile/` is taken by a real
   directory, so `compile.pl` is the one file that cannot follow the
   "folder named after the file" rule. Candidates: `v6/prolog/compile_pl/`
   (what this plan uses), `v6/prolog/compile/parts/` (nests under the existing
   folder, reads oddly), or rename `compile.pl` itself. Pick one before PR 4.
2. **`emit_ts.pl`: split it at all?** The door is paused. The plan is written
   and gradeable; the recommendation is to skip it.
3. **`0_program_check.pl`: two parts or one?** The two-part cut lands 312 and
   359 lines and makes `program_violation/3` discontiguous across parts, which
   the existing `:33` directive already permits. The one-part cut is 671 lines
   and keeps the predicate whole. Section 7.5 has both.
4. **The part-size ceiling.** This plan capped at 700 and targeted 200-500.
   `lower/28_fixpoint_ir.pl` at 573 and `lower/30_aggregate_heads.pl` at 519
   are the only two over 500. Tightening to 500 costs four more cuts in
   `lower.pl` and one in `emit_ts.pl`.
5. **`.github/CI-KNOWN-RED.md:117`** records `prolog-lint` at `findings=14`;
   it measures 18 today. Out of this lane's scope, flagged.
