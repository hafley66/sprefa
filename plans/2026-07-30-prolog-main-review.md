# Prolog main review, 2026-07-30

Reviewer lane: `lane/prolog-review`, worktree `/Users/chrishafley/projects/sprefa-lane-plreview`,
base `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd`. Read-only. Nothing in `v6/prolog/` was edited.

Scope: `v6/prolog/` in full, 15,532 lines across 36 `.pl` files plus the 193-fixture corpus.

## What was run, hermetically, before any claim below

All under `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, scratch dbs under `$TMPDIR`,
no daemon touched, `~/.local/state` untouched.

| gate | result |
|---|---|
| `conformance` (`conformance/go.pl`) | 193 PASS / 0 fail |
| `plunit` (`compile/test/plunit_tests.pl`) | 222 tests, exit 0 (1 choicepoint warning at `plunit_tests.pl:2784`) |
| `roundtrip` (`compile/scripts/roundtrip.sh`) | ALL GRADES PASS, G2 no parse errors |
| `text-door` (`compile/scripts/text_door_receipt.sh`) | compiled=133 byte_identical=133 failures=0 |
| `prolog-lint` (`tools/prolog-lint.sh`) | findings=1 baseline=1 OK, 91 files cross-referenced |
| `0_profile_compile_curve.sh` | ran, output analysed in F13/F14 |
| sweep | NOT run: `v6/tsv2/node_modules` is absent in this worktree. Bucket counts below are read from the checked-in `compile/out/manifest.json` + `run-results.json`. |

Every finding below has a file:line and a case I ran. Findings are split into PROVED (I observed the
behaviour) and SUSPECT (the mechanism is in the source but I could not execute the last hop).

Probe harness used throughout: a two-door driver that puts one `prog/2` through
`engine:run_program/5` and through `compile:program_plan/2` + `lower:lower_program/2`, printing both
answers. It lives only in scratch; nothing was added to the tree.

---

# PROVED

## F1. The oracle cannot solve `combine/variadic` or `next/1`. Both are registry `live`.

Severity: **HIGH**. Two shipped constructs answer zero rows on the reference door and real rows on
the emitted door.

`conformance/body.pl:154-168` is the whole of `solve/2`. It has clauses for `true`, `,`, `not`,
`latest`, `pre`, `now`, `finalize`, `:=`, `is`, `decode`, `json_each`, comparisons, and then the
catch-all at `body.pl:168`:

```prolog
solve(Atom, Ctx) :- Ctx = ctx(Visible, _, _), member(Atom, Visible).
```

There is no `combine` clause and no `next` clause. A term-form `combine(a(X), b(Y))` is therefore
looked up as a relation literally named `combine/2`, finds nothing, and the rule derives nothing.

Receipt, both doors, same program:

```
prog([], [ (out(X,Y) <- combine(a(X), b(Y))) ]), initial [a(1), b(2)]
  ORACLE   ok final=[a(1),b(2)]          <- no out/2 at all
  COMPILER ok (lowered)                  <- emits the cross join

prog([], [ (out(X) <- next(a(X))) ]), initial [a(1)]
  ORACLE   ok final=[a(1)]               <- no out/1
  COMPILER ok (lowered)
```

This is not a registry oversight. The oracle's OTHER walkers do handle the term form:
`conformance/level_eval.pl:127-131` dispatches on `surface_for_term(Goal, _, _, splice_bare, _, _)`
and splices the arguments for stratification, and `0_body_walk.pl:121` documents the same splice for
the trigger walk. So the oracle stratifies a construct its solver cannot execute.

Why nobody noticed: `compile/parse_dl.pl:1092`

```prolog
build_surface_item(_, splice_bare, live, Args, Item) :- !, combine_body(Args, Item).
```

splices at PARSE time, so the text door never produces the term. Consequences, all verified:

- Zero fixtures in the 193-fixture term corpus use `combine(` or `next(` in a rule body
  (`grep` over `conformance/fixtures/*.pl` returns only the rel name `queue_next` and `pre(...)`).
- The only coverage is `v6/dl/fixtures/golden-flex.dl6:208,217`, which enters through the parser and
  is therefore already spliced before either door sees it.
- `compile/print_dl.pl` cannot print them. Parsing `golden-flex.dl6` and printing it back yields
  `combine ERASED` / `next ERASED` (ran it). The roundtrip gate compares TERMS, not text, so it is
  structurally blind to this.
- `compile/scripts/golden_coverage.pl:56-58` knows the term vanishes and falls back to grepping the
  source text for `combine(`. That gate proves the spelling is present in one file; it cannot prove
  the construct works.

Judgment: `combine` and `next` are write-only surface. They have a lowering, no printer, no oracle
solver, and no graded fixture in the form the oracle actually consumes.

## F2. Five reserved registry words silently derive nothing on the oracle door.

Severity: **HIGH**. Same mechanism as F1, different words. The compiler refuses each by name; the
oracle runs the program and answers an empty relation.

Registry rows: `compile/registry.pl:48` (`zip/2`), `:50-53` (`unsubscribe/1`, `complete/1`,
`subscribe/1`, `error/1`).

```
out(X,Y) <- zip(src(X), other(Y))       ORACLE ok, out/2 absent | COMPILER unsupported_construct(zip)
out(X) <- src(X), subscribe(other(X))   ORACLE ok, out/1 absent | COMPILER unsupported_construct(lifecycle_arm(subscribe))
out(X) <- src(X), complete(other(X))    ORACLE ok, out/1 absent | COMPILER unsupported_construct(lifecycle_arm(complete))
out(X) <- src(X), unsubscribe(other(X)) ORACLE ok, out/1 absent | COMPILER unsupported_construct(lifecycle_arm(unsubscribe))
out(X) <- src(X), error(other(X))       ORACLE ok, out/1 absent | COMPILER unsupported_construct(lifecycle_arm(error))
```

`0_program_check.pl:341-343` already names this class in prose, while scoping the `call/N` refusal
narrowly: "the wider class (any reserved registry word silently deriving nothing on the ORACLE door,
which `zip/2` also does today) is a separate and larger question". This is the receipt for the whole
family, and F1 shows the family is bigger than "reserved": it includes two `live` rows.

The shape of a fix is the same shape `dynamic_relation_name` already took: the oracle needs a
reserved-word gate at load, driven by `surface/5`, so an unimplemented or reserved functor in a body
is a refusal instead of an empty relation lookup.

## F3. A level-head expression whose type contradicts the declared column type is checked by nobody.

Severity: **HIGH**. Same class as the ref-column burr B1 closed today, one column type over.

```
prog([col_type(i/1,v,int), col_type(o/1,v,int)], [ (o(concat([A,x])) <- i(A)) ]), initial [i(3)]
  ORACLE   ok final=[i(3), o('3x')]
  COMPILER ok (lowered)
```

The emitted statement is

```sql
INSERT OR IGNORE INTO "o" ("v") SELECT (b0."v" || 'x') FROM "i" b0
```

and `compile/lower.pl:749` declares the destination

```prolog
column_def(QuotedColumn, int, Def) :- !, format(atom(Def), '~w INTEGER NOT NULL', [QuotedColumn]).
```

Real sqlite receipt (tables are `PRIMARY KEY (...) WITHOUT ROWID` per `lower.pl:738`, and composite
keys behave the same):

```
CREATE TABLE b ("v" INTEGER NOT NULL, PRIMARY KEY("v")) WITHOUT ROWID;
INSERT INTO b SELECT (3 || 'x');
SELECT "v", typeof("v") FROM b;   ->   3x|text
```

Two more of the same shape, both accepted on both doors:

- `col_type(o/1,v,int)` with `o(A/2.0)` stores the float `1.5` in an INTEGER column.
- `col_type(o/1,v,text)` with `o(A+1)` stores the integer `4` in a TEXT column.

The EDGE twin is equally unguarded: `analyze.pl:1034`'s `edge_head_column_type_mismatch` compares a
BODY column type to a HEAD column type for a variable passing through, so a head EXPRESSION bypasses
it. The edge version of the concat program compiles clean too.

Note the internal inconsistency inside one predicate. `lower.pl:749-763`:

| declared type | emitted constraint |
|---|---|
| `int` | `INTEGER NOT NULL`, no CHECK |
| `bool` | `INTEGER NOT NULL CHECK (col IN (0,1))` |
| `float` | `REAL NOT NULL CHECK (typeof(col) = 'real' AND col BETWEEN ...)` |
| `ref(_)` | `INTEGER NOT NULL`, no CHECK |
| `text` | `TEXT NOT NULL`, no CHECK |

`bool` and `float` are defended at the storage layer; `int`, `text` and `ref` are not. The
`decl_type_conflicts_witness` refusal covers a declaration contradicting a LITERAL witness, never an
EXPRESSION result.

## F4. The B1 ref-column fix closes only when both columns are declared. The burr survives one decl away.

Severity: **HIGH**.

`0_program_check.pl:252-262` (`relation_column_type_conflict`) requires the offending variable to sit
at a ref-typed column AND at some OTHER DECLARED column of a different type. Its own scope comment
(`:248-251`) says so honestly. The problem is that the incident it was written for is reachable
without the second declaration, and an undeclared source rel is not exotic: it is what
`edb_definition` blesses and what most of the corpus writes.

```
WITH the source declared        (col_type(src/1, p, text)):
  ORACLE   threw(relation_column_type_conflict(span/2,at,fpath,src/1,p,text))
  COMPILER threw(unsupported_construct(relation_column_type_conflict(...)))   <- the fix works

DROP that one declaration, same rule:
  prog([type_decl(fpath,[col(name,text)]), col_type(span/2,at,fpath), col_type(span/2,s,int)],
       [ (span(P,1) <- src(P)) ]),  initial [src('a.rs')]
  ORACLE   ok final=[src('a.rs'), span('a.rs',1)]
  COMPILER ok (lowered)
```

The plan the compiler builds for that program:

```
relplan(span/2, set, [at,s], none, [ref(fpath), int])
```

so `at` is `INTEGER NOT NULL` (`lower.pl:762`) and the emitted level insert selects `src."col1"`, a
text, straight into it. That is exactly the B1 incident text
(`plans/2026-07-30-relpattern-adversarial-review.md`): text into an integer-affinity ref column, read
back by the boundary render as a dictionary id.

`analyze.pl:program_column_types/7` already infers a type for `src`'s column from its literal
witness. `0_program_check.pl` deliberately does not see that inference (it takes `prog/2` only), so
the information needed to close the hole exists and is not wired to the check.

## F5. A compound nested two deep in an untyped column renders differently at the two doors.

Severity: **HIGH**. Both doors compile; the tick logs disagree byte for byte; zero fixtures cover it.

`compile/lower.pl:455-462` compiles any unrecognised compound expression into the json1 tagged form,
recursing through `compile_term_sub_expr/3`, so `foo(bar(1))` stores

```json
{"fn":"foo","args":[{"fn":"bar","args":[1]}]}
```

`compile/lower.pl:2486-2491` (`canonical_column_expr(Column, text, Expr)`) undoes exactly ONE level:

```sql
CASE WHEN json_valid(c) AND json_type(c) = 'object'
     THEN json_extract(c,'$.fn') || '(' || (SELECT group_concat(value,',') FROM json_each(c,'$.args')) || ')'
     ELSE c END
```

Receipt, real sqlite vs the real oracle encoder:

```
sqlite3   ->  foo({"fn":"bar","args":[1]})
ticklog:value_json(foo(bar(1)), J)  ->  "foo(bar(1))"
```

`conformance/ticklog.pl:170-174` (`term_text/2`) recurses; the SQL does not. The lower.pl comment at
`:2465-2471` discusses arity ("any number of arguments in original order") and never depth.

Both doors accept the program that produces it:

```
prog([], [ (out(foo(bar(X))) <- src(X)) ]), initial [src(1)]
  ORACLE   ok final=[out(foo(bar(1))), src(1)]
  COMPILER ok (lowered)
```

Width is fine and I checked it: `foo('a,b')` renders `foo(a,b)` on both sides, because
`ticklog:term_text/2` uses `~w` with no quoting either. Only depth diverges.

## F6. `min`/`max` over a text column crashes the oracle with a raw SWI arithmetic error.

Severity: **MEDIUM**. The compiler names it; the oracle has no operand check at all.

```
prog([], [ (m(min(N)) <- src(N)) ]), initial [src(alpha), src(beta)]
  ORACLE   threw(error(type_error(evaluable, alpha/0), context(lists:min_list/3, _)))
  COMPILER threw(unsupported_construct(aggregate_operand_not_number(min, _, text)))
```

`conformance/level_eval.pl:240-241` calls `min_list/2` and `max_list/2` directly with no type gate;
`compile/lower.pl:2193` has the named refusal. A cold author gets a prolog library internal on the
door that is supposed to be the specification. The message has no relation name, no column, no rule.

## F7. `keep(count(N))` with a negative N makes the oracle FAIL, silently, with no message.

Severity: **MEDIUM**.

`conformance/engine.pl:390-395`:

```prolog
prune_rel(Ref-count(Limit), Store0, Store) :-
    log_stamps(Store0, Ref, Stamped),
    length(Stamped, Total),
    Drop is max(0, Total - Limit),
    length(Dropped, Drop), append(Dropped, _, Stamped),
    ...
```

With `Limit = -3` and one stored row, `Drop = 4` and `append(Dropped, _, Stamped)` cannot succeed, so
`prune_rel/3` fails, so `tick/7` fails, so `run_program/5` fails.

```
prog([kind(l/1,log), keep(l/1, count(-3))], []), schedule [[+l(a)]]
  ORACLE   failed          <- no exception, no message, no diagnostic
  COMPILER ok (lowered)
```

`keep(count(0))` is accepted by both and stores nothing, which is at least defensible. There is no
validation of the retention bound anywhere on either door. A bare `failed` out of `run_program/5` is
worse than a throw: `engine:fixture_expectations_hold/2` cannot distinguish it from a wrong answer,
and `0_refusal_messages.pl` has nothing to print.

## F8. Unsafe negation: the oracle's answer depends on whether the negated relation happens to be empty.

Severity: **MEDIUM**.

```
prog([], [ (out(X) <- src(_), not(other(X))) ]), initial [src(a), other(b)]
  ORACLE   ok final=[other(b), src(a)]     <- out/1 silently empty
  COMPILER threw(unsupported_construct(unbound_head_var(_)))
```

`body.pl:156` is `solve(not(Goal), Ctx) :- \+ solve(Goal, Ctx)`. With `X` unbound, `\+ other(X)`
succeeds only when `other/1` has NO rows, and then `eval_head/2` reaches an unbound argument and
throws `unbound_in_expression` (`body.pl:25`). So one program has two oracle behaviours selected by
the data: zero rows when the negated rel is populated, a throw when it is empty. The compiler is
right to refuse; the oracle has no range-restriction check.

## F9. `edge_body_needs_json_destructure` refuses cases its own stated reason does not cover, and cites a slot that was ruled.

Severity: **MEDIUM**. Nine of the sixty unsupported fixtures sit behind it.

`compile/analyze.pl:943-945` refuses by FUNCTOR:

```prolog
edge_goal_refusal(Goal, Body, 8, edge_body_needs_json_destructure(Body)) :-
    nonvar(Goal), ( Goal = decode(_, _) ; Goal = json_each(_, _) ).
```

The reason written above it (`analyze.pl:925-935`) is about UNTYPED compound arrivals: "a compound
value that ARRIVES is stored as canonical term text ... the two encodings do not meet". True, and it
does not apply to a column with a DECLARED struct type, which has no term-text encoding at all and
whose lowering already exists.

```
decode over a struct-typed column, LEVEL body   -> COMPILER ok (lowered)     [dictionary join]
the identical decode,               EDGE body   -> COMPILER threw(unsupported_construct(
                                                     edge_body_needs_json_destructure(...)))
```

Separately, `compile/lower.pl:931-934` states the reason for the same refusal as
"that encoding question is SLOT-TERM-STRUCT's, not this one's". `conformance/rulings.pl:367` ruled
`compound_storage = struct_as_rows` on 2026-07-29, which is that slot. So the two files give two
different reasons for one refusal and one of them cites a dead slot. `ARCH.pl:788`
(`json_edge_body_unblock`) already carries the stale-reason half; the over-wide-refusal half is new.

## F10. `relax_strata/4` is 22 byte-identical lines in two files, and `strat.pl` hand-rolls a topological sort the new graph module ships.

Severity: **MEDIUM**. This is the answer to "what grew back or was missed".

`diff` of `conformance/level_eval.pl:151-172` against `compile/strat.pl:80-101` is EMPTY. Both are
`relax_strata/4`, character for character. `strat.pl:10-11` admits it: "same relax_strata gap
algorithm, reimplemented here since that predicate is not exported".

This is the single most dangerous duplication in the tree, because the two copies decide the stratum
NUMBERS on the two doors. They agree today only because they are identical text; nothing tests that
they stay identical, and a divergence would silently reorder the emitted SQL relative to the oracle's
fixpoint.

One layer out, the same algorithm is written twice at a coarser grain:
`level_eval.pl:89-108` (`stratify_level_rules/2`) and `strat.pl:42-65` (`stratum_groups/2`) compute
the same grouping with different helper spellings. The org review's rank R9 shared the DECLARATION
queries (`relation_kind/3`, `declared_key/3`) and left the stratifier alone.

And `strat.pl:130-138`:

```prolog
kahn_order([], _, Acc, Order) :- reverse(Acc, Order).
kahn_order(Remaining, Edges, Acc, Order) :- ...
```

is a hand-rolled topological sort, per-step `select/3` over the remaining list with a `member/2` scan
of the edge list inside a negation. `0_graph.pl:219-220` already exports
`graph_topological_order/2` over `library(ugraphs)`' `top_sort/2`, and `0_graph.pl:224-225` exports
`graph_has_cycle/1`, which is exactly what `topo_order_group/2`'s length comparison at `strat.pl:120`
is emulating. `0_graph.pl` has exactly two consumers today: `compile/3_clock_check.pl:27` and
`labs/rel_definition_hash/`. `strat.pl` is the third caller it was built for and does not use it.

## F11. Duplicate and partial `col_type` declarations are silently discarded, names and types both.

Severity: **MEDIUM**.

`compile/analyze.pl:293-301` gates on `length(TypedColumns, Arity)`, so ANY count of declared columns
other than exactly the arity falls to the inference path. Receipts, reading the built plan:

| declarations | resulting relplan |
|---|---|
| `col_type(r/1,v,int)` | `relplan(r/1,set,[v],none,[int])` |
| `col_type(r/1,v,int), col_type(r/1,v,text)` | `relplan(r/1,set,[col1],none,[int])` |
| `col_type(r/1,v,int), col_type(r/1,w,text)` | `relplan(r/1,set,[col1],none,[int])` |
| `col_type(r/2,a,int)` (arity 2, one decl) | `relplan(r/2,set,[col1,col2],none,[text,text])` |

Read the table: declaring a column type twice silently loses the column NAME (`v` becomes `col1`,
changing the emitted SQL identifier). Declaring types for SOME columns of a rel silently loses the
names AND the types, so the declared `int` becomes an inferred `text`. Both doors accept all four.
The `decl_type_conflicts_witness` refusal never fires because it compares a declaration to a literal
witness, not a declaration to another declaration.

Reachability: the row-count mismatch cases are term-door shapes (fixtures, labs, hand-built plans),
not `.dl6` text, where the parser emits one `col_type` per declared column. That caps the severity,
but the term door is the corpus's door.

## F12. Two fixtures in the 193 are unconditional passes.

Severity: **MEDIUM** for a corpus whose green count is a headline number.

`conformance/engine.pl:565-566`:

```prolog
forall(member(Expectation, Expectations),
       expectation_holds(Expectation, FinalAll, DeltaTicks))
```

`forall/2` over an empty list succeeds. Two fixtures have `[]`:

- `conformance/fixtures/expressions.pl:266-271` `typed_int_without_literal_witness`: no rules, no
  initial rows, no schedule, no expectations. It cannot fail. It asserts nothing.
- `conformance/fixtures/expressions.pl:275-280` `typed_int_contradicts_text_witness`: one seed row,
  no expectations. Its comment claims "A declaration type that disagrees with a concrete witness is a
  compiler refusal", which this fixture never checks; that claim is graded only through the sweep.

The wider corpus picture, measured:

| shape | count |
|---|---|
| fixtures | 193 (193 distinct names) |
| empty expectation list | 2 |
| ticks-only expectations | 0 |
| no initial AND no schedule | 16 (14 are legitimate `throws/1` refusal fixtures) |
| `throws/1` fixtures | 33 |
| `final(_, [])` assertions | 30 |
| fixtures asserting only empty results | 7 |
| zero-rule programs | 29 |

The 7 "assert only empty" fixtures are not vacuous (`decode_missing_key_fails_quietly`,
`text_one_and_numeric_one_never_join` and friends pin a real negative), and 33 `throws` fixtures pin
real refusals. Only the 2 above are hollow. Also: the corpus is 193 while the SWEEP covers 133
compiled + 60 unsupported, so 60 fixtures are graded by the oracle alone and never replayed.

## F13. The compile-profiling instrument's headline is measured on a shape the compiler never sees.

Severity: **MEDIUM**. The instrument is good; its conclusion is drawn from the wrong fixture.

`compile/scripts/0_profile_compile_curve.sh` generates its own program:

```awk
for (i = 0; i < relations; i++)  printf "rel r%d(x: text).\n", i
for (i = 1; i <= rules; i++)     printf "r%d(X) <- r%d(X).\n", i, i - 1
```

One body atom, one column, one variable, no guards, no joins, no types, no hosts, no aggregates. On
that shape it prints `dominant_phase: emit` and
`controlled_shape: plan scales as rules^0.964`.

On the four largest real `.dl6` programs in the tree, `plan` dominates, not `emit`:

| program | parse_ms | plan_ms | lower_ms | emit_ms | plan inferences | emit inferences |
|---|---|---|---|---|---|---|
| `v5-parity.dl6` | 13 | **58** | 25 | 37 | 1,225,654 | 357,648 |
| `flagship-flow.dl6` | 12 | **65** | 21 | 38 | 1,365,721 | 355,902 |
| `golden-flex.dl6` | 8 | **17** | 7 | 14 | 322,021 | 125,847 |
| `flagship-callgraph.dl6` | 6 | 5 | 4 | 9 | 91,387 | 94,841 |

And the `rules^0.964` claim is an artefact of the interval it is computed over. The awk block reads
only the LAST pair of rows (N=100 to N=117), where `plan_ms` moves 13 to 15 against a `wall_ms`
resolution of 10ms and phase clocks of 0.5ms. Running the same generator further out:

| n (rels) | plan_ms | plan inferences |
|---|---|---|
| 25 | 5 | 82,731 |
| 50 | 13 | 257,807 |
| 100 | 37 | 881,270 |
| 200 | 171 | 3,796,837 |

8x the program, 46x the inferences: `rules^1.84`, heading to quadratic. The script's own guard
("an exponent back above 2 means a per-path or per-pair search returned") is therefore armed at
sizes where the quadratic term has not yet shown up. It would not have caught this.

`emit` deserves its share on the synthetic and only there. Its cost is text assembly:
`emit_ts.pl:81-92` (`js_template_codes/2`) rewrites every SQL statement code by code with two full
`atom_codes`/`string_codes` conversions per statement, and the exec profile at N=60 attributes
33.3% to `format/3` (4,003 calls) and 16.7% each to `js_template_codes/2` and `string_codes/2` (645
calls each). That is O(emitted bytes) with a constant factor, linear, and not currently a problem.
Caveat on that number: the exec profile's total is 0.026 s at 1000 Hz, which is 26 samples. The
percentages in that table are noise.

## F14. Plan's real hot spot: the whole rule set is re-walked once per column.

Severity: **MEDIUM**, and it is the thing to fix if plan is ever a problem.

SWI wall profiler, 20 `program_plan/2` runs over `flagship-flow.dl6` (181 lines):

```
lists:member_/3                 391,380 calls   16.3% self
body_walk:walk_node/6           326,300 calls   11.4% self + 29.6% children
lists:reverse/4                 889,260 calls    9.4% self
registry:surface/5            1,461,380 calls    4.3% self
analyze:ref_occurrence_args/3    11,520 calls    2.8% self + 60.9% CHILDREN
```

Per single plan of a 181-line program: 16,315 `walk_node/6` calls and 73,069 `registry:surface/5`
resolutions.

The driver is `compile/analyze.pl:310-313`:

```prolog
ref_occurrence_args(Rules, Ref, Args) :-
    member(Rule, Rules),
    ( rule_head(Rule, Head), rel_ref(Head, Ref), Head =.. [_ | Args]
    ; rule_body(Rule, Body), body_ref_uses(Body, Uses), member(use(Ref, Args, _, _), Uses) ).
```

called from `analyze.pl:315-321` (`column_name_at/5`) inside `maplist` over every position of every
ref (`analyze.pl:288-290`). So the whole rule set, including a full `body_ref_uses/2` walk of every
body, is traversed once per (Ref, Position). The if-then commits on the first hit, which is why the
trivial chain fixture looks near-linear; the cost lands on positions with NO named variable, which
must EXHAUST the scan before falling back to `col<N>`, and on refs that occur late in rule order.

The comment at `analyze.pl:280-286` explains correctly why `findall/3` cannot be used here (it would
`copy_term` away the variable identity the whole naming scheme depends on). That constraint rules out
collecting solutions; it does not rule out computing all column names for all refs in ONE pass over
the rules, which is the shape that removes the quadratic.

## F15. Documentation counts are stale by a full arc, again.

Severity: **LOW** individually, **MEDIUM** as a recurrence. Every one of these is a number a reader
would trust.

| claim | file:line | actual |
|---|---|---|
| "fixtures swept 155 / compiled 94 / IDENTICAL 92 / UNSUPPORTED 61" | `compile/SCOREBOARD.md:31-38` | 193 / 133 / 131 / 60 (from the checked-in `out/manifest.json` + `out/run-results.json`) |
| "expect: 156 PASS, nothing else" | `v6/justfile` conformance recipe | 193 PASS |
| "expect: 137/137 and growing" | `v6/justfile` plunit recipe | 222 |
| "expect: 95/95/0" | `v6/justfile` text-door recipe | 133/133/0 |
| "expect: RUN total=95 identical=93 wrong=0" | `v6/justfile` sweep recipe | 133 compiled / 131 identical / 2 run_error |
| "the 135-fixture baseline plus the keep refusal fixture" | `v6/justfile` header | 193 |

Two banned-word uses, flagged not edited, per the standing law:
`v6/prolog/ARCH.pl:690` uses "provenance"; `v6/prolog/compile/lower.pl:987` uses "load-bearing".
(`v6/prolog/LANG.md:89-90` is the law statement itself and is exempt.)

---

# SUSPECT

## S1. `avg` over integer columns probably diverges: the oracle can return an integer, SQLite's `avg` cannot.

`conformance/level_eval.pl:236-239`:

```prolog
agg_compute(avg, Contributions, Average) :-
    sum_list(Contributions, Sum), length(Contributions, Count), Average is Sum / Count.
```

SWI's `/` returns an integer when the division is exact. Verified oracle-side:

```
prog([], [ (m(avg(N)) <- src(N)) ]), initial [src(4), src(2)]
  ORACLE   ok final=[m(3), src(2), src(4)]     <- integer 3
  COMPILER ok (lowered)   emits: SELECT avg(b0."col1") FROM "src" b0 HAVING count(*) > 0
```

SQLite's `avg()` always returns REAL, so the emitted side holds 3.0. Whether the two tick logs
differ then depends on the TS-side canonicaliser and on `ticklog.pl:126-138`
(`normalize_float_json_atom/2`), which strips a trailing `.0`. I could not run the emitted module:
`v6/tsv2/node_modules` is absent in this worktree.

Why it is ungraded: the only `avg` fixture is `conformance/fixtures/5_value_plane.pl:47-57`
(`float_avg_is_grouped`), whose columns are all declared `float`. There is no int-column `avg`
fixture. Worth one.

## S2. `group_concat` with no ORDER BY is relied on for argument order.

`compile/lower.pl:2490` renders a compound's arguments with
`(SELECT group_concat(value, ',') FROM json_each(c, '$.args'))` and the comment at `:2471` claims
"renders any number of arguments in original order". SQLite does not guarantee `group_concat`
ordering without an `ORDER BY` inside the aggregate. It follows scan order in practice today. I never
observed it wrong, and I did not construct a case that would flip it, so this is a claim stronger
than its SQL rather than an observed defect.

## S3. `aggregate_in_edge_head` inspects top-level head arguments only.

`0_program_check.pl:92-99`:

```prolog
aggregate_head_ref(Head, Ref) :-
    compound(Head), Head =.. [_ | Args], Args \== [],
    member(Arg, Args), aggregate_argument(Arg), !, head_ref(Head, Ref).
```

`member(Arg, Args)` walks one level. An aggregate nested inside a head expression, `h(K, count(X)+1)`,
would not be recognised, so the `aggregate_in_edge_head` refusal (and `level_eval.pl:27-31`'s own
`aggregate_head/3`, which has the same one-level shape) would miss it. I did not run it to ground
because the head-arithmetic path has its own refusals that may or may not catch it first. Worth ten
minutes.

## S4. Eight lab directories are alive on main, at least four of them past their landing.

`v6/prolog/labs/` contains `generic_scan_instantiation`, `ghcacher_tick_golden`, `json_interop`,
`json_syntax`, `openapi_codegen`, `rel_as_stream`, `rel_definition_hash`, `rel_value_unification`.

The standing lab protocol says a lab file surviving its landing commit is a defect, and that
`v6/prolog/labs/` "was deleted 2026-07-27 and stays deleted". Four of these have landed receipts:

- `json_syntax`: `ARCH.pl:783` reads "LANDED 2026-07-30 (merge 62f9ce84 ...)".
- `rel_definition_hash`: `0_graph.pl:8-10` cites it as the second SCC copy that was consolidated
  away, i.e. history.
- `rel_as_stream`: `rulings.pl:453` (`scan_surface`) is its verdict, ruled 2026-07-30.
- `openapi_codegen`: `rulings.pl:459-466` are its rulings, ruled 2026-07-30.

SUSPECT rather than PROVED because the prompt says other lanes are live and some of these may belong
to arcs still in flight. The four above are the ones whose verdicts are already recorded in
permanent homes.

## S5. `v6/prolog/src/` is dead: four files, 374 lines, zero live callers.

`prolog-lint` reports every export of `src/checks.pl`, `src/emit_ts.pl`, `src/grader.pl` and
`src/kernel.pl` as an unused-export candidate. The only references in the tree are `ARCH.pl:291`
(`use_module('src/kernel.pl')`) and `labs/rel_value_unification/*` (`use_module('../../src/grader.pl')`),
which is the lab tree from S4. `ARCH.pl:662` records `src/emit_ts.pl` as "superseded by
tsv2_pipeline"; `ARCH.pl:171` calls it an "engine-v1 seam experiment".

The module-name collision the org review found IS fixed: `src/emit_ts.pl:27` declares module
`emit_ts_engine_v1` and `compile/emit_ts.pl:34` declares `emit_ts`, and loading both in one process
succeeds (ran it). SUSPECT only on whether `src/kernel.pl` is still wanted by `ARCH.pl`.

## S6. `relation_kind/3` carries a clause for a word the surface no longer has.

`0_program_check.pl:76`:

```prolog
relation_kind(Decls, Ref, set) :- declared_kind(Decls, Ref, set), !.
```

Ruling `no_policy_suffix_words` (`rulings.pl:288`) removed `set` from the surface, and
`registry.pl:113` marks `set/0` `decl(refuse(removed_word))`. `kind(_, set)` appears in ZERO of the
193 fixtures (`kind(_, log)` appears 168 times). The clause is unreachable through either door
unless a term-form program writes it by hand, in which case it silently succeeds where the parser
would refuse. Small; listed for completeness because it is a live acceptance path for a refused word.

---

# What is genuinely good, and should be protected

These are the parts I tried to break and could not, or that made this review possible at all.

**The two-door split, with the sharing line drawn explicitly.** `0_program_check.pl:16-33` states
what it deliberately does NOT own (exception vocabulary, check ORDER, compiler capability refusals)
and why each is fixture data. That paragraph is what let me reason about door disagreements at all
instead of guessing. The classes it DOES share are genuinely shared: `first_violation/3` walks
whatever order it is handed, and `engine.pl:137-162` and the compiler each declare their own order
over one implementation.

**Refusals are named, and the inventory is derived rather than listed.**
`0_refusal_messages.pl:64-76` builds the refusal inventory by walking the loaded CLAUSE BODIES of ten
named modules for `unsupported_construct(Reason)` subterms. A new refusal enters the inventory from
its defining clause. There is no second list to forget. That is the right shape and it is rare.

**The comments state limits instead of claiming completeness.** `analyze.pl:925-935` explains exactly
which encodings do not meet; `0_program_check.pl:264-293` names a capability decision as a capability
decision and argues why refusing on both doors beats a door disagreement; `lower.pl:918-924` explains
why `decode/2` is a rule rewrite rather than a compiler stage, so no statement family can be the one
where the destructure is silently absent. Several of my findings are things these comments already
half-say. That is a good failure mode for a codebase to have.

**`0_graph.pl`.** The buy-before-build verdict is IN THE HEADER with numbers (27,082 ms Warshall vs
27 ms direct on a 1000-node chain; 255,333 ms of 255,490 ms on the old simple-path enumeration), the
`transitive_closure/2` strictness was "confirmed by measurement, not by reading" with the three
worked cases inline, and the obviously-correct-but-cubic composition is RETAINED as a differential
oracle in `test/0_graph.test.pl`. Keeping the slow correct implementation as the test oracle for the
fast one is the pattern to repeat.

**`golden_coverage.pl`'s symmetry.** It fails when a live construct is unexercised AND when a
declared absence stops being a refusal (`golden_coverage.pl:22-27`). A refusal quietly becoming
acceptance breaks the gate. Most coverage gates only check one direction.

**The registry as one inventory.** `surface/5` and `expression/5` drive analyze dispatch, the parser,
the printer, the generated SYNTAX table, the tmLanguage grammar, the CLI verb list, the oracle's
comparison-goal recognizer (`body.pl:100-104`) and the oracle's aggregate classifier
(`level_eval.pl:33-51`). `level_eval.pl:38-44` even explains why the aggregate classifier
deliberately does NOT filter on `Status`, and what would silently break if it did. The exceptions to
this discipline are exactly where my findings are, which is itself evidence the discipline works.

**Everything green, first run, hermetic.** 193/193 conformance, 222 plunit, 133/133 text door,
roundtrip all grades, lint at baseline, on a cold worktree with a nonexistent config and no daemon,
with no setup beyond `swipl`. A review that can run the whole thing in ten minutes is a review that
can find things; that property was paid for and should not be spent.

---

# Ranked, one line each

| # | finding | severity | proved? |
|---|---|---|---|
| F1 | oracle cannot solve `combine`/`next`, both registry `live`; term form derives zero rows, compiler emits a join | HIGH | proved |
| F2 | `zip`/`subscribe`/`complete`/`unsubscribe`/`error` silently derive nothing on the oracle door, refused by name on the compiler | HIGH | proved |
| F3 | level-head expression type vs declared column type is unchecked; concat writes text into `INTEGER NOT NULL` | HIGH | proved |
| F4 | the B1 ref-column fix needs BOTH columns declared; drop one decl and text flows into a ref column again | HIGH | proved |
| F5 | depth-2 compound in an untyped column: `foo(bar(1))` vs `foo({"fn":"bar",...})`, both doors compile, no fixture | HIGH | proved |
| F9 | `edge_body_needs_json_destructure` refuses struct-typed decode its own reason does not cover; `lower.pl:931` cites a ruled slot | MEDIUM | proved |
| F10 | `relax_strata/4` byte-identical in two files; `strat.pl` hand-rolls a topo sort `0_graph.pl` ships | MEDIUM | proved |
| F6 | `min`/`max` on text crashes the oracle with a raw SWI `type_error` | MEDIUM | proved |
| F8 | unsafe negation: oracle answers zero rows or throws depending on the data | MEDIUM | proved |
| F7 | `keep(count(-3))` makes `run_program/5` fail silently, no message | MEDIUM | proved |
| F11 | duplicate/partial `col_type` silently drops column names and types | MEDIUM | proved |
| F13 | profile curve's `dominant_phase: emit` and `rules^0.964` are artefacts of a one-atom synthetic; real programs are plan-dominant at `rules^1.84` | MEDIUM | proved |
| F14 | plan re-walks all rules once per column (`ref_occurrence_args/3`), 73k registry hits per 181-line program | MEDIUM | proved |
| F12 | two fixtures have empty expectation lists and pass unconditionally | MEDIUM | proved |
| F15 | SCOREBOARD.md and five justfile expect-comments stale by a full arc | LOW | proved |
| S1 | `avg` over int columns: oracle integer vs SQLite REAL, no int fixture | MEDIUM | suspected |
| S3 | `aggregate_in_edge_head` only inspects top-level head args | MEDIUM | suspected |
| S4 | eight labs alive on main, four with landed verdicts | MEDIUM | suspected |
| S5 | `v6/prolog/src/` is 374 lines of dead code | LOW | suspected |
| S2 | `group_concat` without ORDER BY relied on for argument order | LOW | suspected |
| S6 | `relation_kind/3` accepts `kind(_, set)`, a removed word | LOW | suspected |

15 proved, 6 suspected.

## The one thing to fix first

**F1 + F2 together: give the oracle a `surface/5`-driven body-goal gate at load time.**

Not because they are the largest, but because they are the cheapest fix that removes a whole class.
Today `conformance/body.pl:168`'s catch-all turns every unimplemented, reserved, misspelled or
not-yet-written construct into "look it up as a relation, find nothing, derive nothing". That is one
clause standing between the reference implementation and a silent wrong answer for seven functors I
found and every functor anyone adds next.

The gate already has a home and a precedent: `engine_check_order/1` at `conformance/engine.pl:137`,
and `dynamic_relation_name` at `0_program_check.pl:344`, which took exactly this shape for `call/N`
and whose own comment (`:341-343`) says the wider class is open. Widening it to "a body goal whose
functor has a `surface/5` row that is not `live`, or has a `live` row the solver has no clause for,
is a refusal" closes F2 outright and turns F1 from a silent wrong answer into a loud gap.

F3 and F4 are the ones with the worse consequences, and both are the same missing check on the same
plane (declared column type versus what actually gets written). They should be one follow-up arc, not
two, and it should include a graded fixture for each of the four cells I ran.
