# expressions: the tier-0 expression layer

Lab: `v6/prolog/labs/expressions.pl`.
Run: `swipl -q -l v6/prolog/labs/expressions.pl -g go -g halt` (22 PASS, exit 0).
Receipts: `swipl -q -l v6/prolog/labs/expressions.pl -g report -g halt`.

## Why this lab

`plans/2026-07-27-lab-consolidation.md` records the audit verdict: the candidate
surface has no expression syntax at all, while arithmetic or comparison appears in
166 of 173 v5 corpus files and string interpolation in 69. `merge_family.md` ends by
proposing head-position expressions (`counter(name, total + 1)`) and parking them,
because a language with terms in columns can read `total + 1` two ways. This is that
lab.

The lab carries two things: an HM checker grown from `books/v6/enum_match.pl` with a
second judgment (expressions have types, goals do not), and a level-rule reference
interpreter that evaluates expressions per binding row after the joins bind their
variables.

## Verdict

Tier 0 holds the whole layer with one exception, stated at the bottom. Every design
choice below is graded in both directions.

| question | ruling | graded by |
|---|---|---|
| head expression: evaluate or store? | evaluate; `quote(...)` opts into storing | `arithmetic_head_evaluates_by_default`, `quote_stores_structure_with_substituted_leaves` |
| the ambiguous case | rejected by ordinary typing, both polarities | `bare_arithmetic_into_term_column_rejected`, `quoted_term_into_int_column_rejected` |
| binding form | `name := expr`, a distinct operator, not `=` | `bind_form_binds_for_later_goals` |
| comparison | a goal, never a value; no Bool type exists | `comparison_is_not_a_value` |
| `+` | Int only; v5's text overload is dropped | `plus_is_int_only` |
| interpolating an Int | auto-converts | `interpolation_auto_converts_int` |
| interpolating a term | compile error naming the type | `interpolation_of_term_rejected` |
| function purity | pure term to term; a rel name in an expression is an error | `rel_read_in_expression_rejected` |
| head arithmetic in recursion | rejected | `head_expression_in_recursive_rule_rejected` |

## Surface grammar additions

Six additions. Nothing else changes, and the keyword count stays at four
(`enum`, `struct`, `rel`, `bind`); every addition below is an operator or a
function-shaped form.

```
expr    ::= name | literal | expr binop expr | fn "(" expr,* ")"
          | "quote" "(" expr ")" | interpolated_string
binop   ::= "+" | "-" | "*" | "/" | "%"
cmp     ::= "<" | "<=" | ">" | ">=" | "==" | "!="
goal    ::= atom | name ":=" expr | expr cmp expr
head    ::= rel "(" expr,* ")"
```

1. **Body comparisons.** `total > cap`, `t - at < 60`. Both sides are expressions;
   the goal produces no value. `< <= > >=` need Int on both sides, `== !=` need equal
   types.
2. **Body bindings.** `sum := a + b`. A new name, visible to every goal after it and
   to the head. Rebinding an existing name is an error.
3. **Head-position expressions.** `counter(name, total + 1)`, `union_size(a, b, ua +
   ub - sh)`. Evaluated once per binding row, after the body.
4. **Pure function application.** `lower(str)`, `split(path, ".", -1)`,
   `strip_prefix(file, prefix)`. Signature table below.
5. **`quote(...)`.** One new function-shaped form. Builds a term instead of applying
   the operator.
6. **String interpolation.** `"banned word ${word} at ${path}"`. A `${name}` hole in
   any string literal.

Expressions are allowed in comparison sides, in `:=` right-hand sides, and in head
columns. They are NOT allowed as arguments of a body atom, which is v5's rule
("never in a binding atom", `docs/reference/syntax.md` `arith` row). The checker
throws `expression_in_atom_argument`. An atom argument is a variable, a literal, or
a wildcard; anything computed goes through `:=`.

### Why `:=` and not `=`

v5 spells the bind as an equation: `s = strip_prefix(f, p), s != f`
(`examples/arch-conformance.dl:33`). That makes `=` mode-polymorphic: it binds when
the left side is free and compares when it is bound, so the meaning of a line depends
on which goals precede it. Reading a rule then requires tracking binding order across
the whole body, and the mode lab is trying to make modes explicit, not implicit.

`:=` binds, `==` compares, and range restriction becomes syntactic: a name is bound by
an atom argument or by a `:=`, full stop. That gives the "unbound variable in an
expression" error a precise home, graded twice
(`unbound_variable_in_expression_rejected`, `unbound_variable_inside_quote_rejected`).

Cost: two extra characters at the 11 `strip_prefix` sites and equivalents in the
corpus. `let name = expr` is the same design with a fifth keyword, which is why it
lost.

## THE COLLISION

`counter(name, total + 1)` in a language with terms in columns:

- reading A: evaluate to an Int and store `6`;
- reading B: store the structure `+(5, 1)` in a term-valued column.

Both are legitimate. Three disambiguation rules were on the table.

### Rejected: type-directed (an Int column evaluates, a Term column stores)

This is the cheapest to write and the worst to read and to implement.

- The same text means two different things in two rules that look identical, and the
  difference lives in a `rel` declaration elsewhere in the file.
- It makes elaboration depend on inference and inference depend on elaboration. To
  know whether `total + 1` is an application or a construction you must first know
  the column type; to type the expression you must first know which of the two it is.
  The cycle is real, not hypothetical: HM types are holes during inference, and a
  column whose type is still a variable at that moment has no answer.

### Rejected: an explicit eval marker (`eval(total + 1)`)

Taxes the case that appears in 166 of 173 corpus files to serve a case that appears
in none of them yet. Wrong side of the ratio.

### Chosen: evaluation is the default, `quote(...)` opts into storage

```
counter(name, total + 1)            evaluates    -> counter("clicks", 6)
counter_patch(name, quote(total + 1))  stores    -> counter_patch("clicks", +(5, 1))
```

Properties that earn it:

1. **Elaboration is syntactic and local.** No declaration is consulted, no inference
   runs first. `quote` is visible in the text at the point where the meaning changes.
2. **The ambiguous case is REJECTED, by ordinary typing.** `quote(E)` has type `Term`
   for every `E`, and `Term` is opaque: nothing unifies with it but itself. So a bare
   arithmetic expression aimed at a `Term` column is a clash, and a quoted term aimed
   at an `Int` column is the mirror clash. Both are graded with the exact error term:

```
collision_bare_into_term
  REJECTED type_clash(term,int,in(column(patch),total+delta))
collision_quote_into_int
  REJECTED type_clash(int,term,in(column(total),quote(total+delta)))
```

   The disambiguation rule is syntactic; the safety net is the type checker. Neither
   one needs the other's answer first.
3. **`quote` freezes the OPERATOR, not the variables.** `quote(total + 1)` with
   `total = 5` stores `+(5, 1)`, not `+(total, 1)`. Substituting the leaves is the
   useful reading (you build data out of row values) and it keeps range restriction
   working underneath the quote, graded by
   `unbound_variable_inside_quote_rejected`. A symbol is a string literal; there is
   no need for a second meaning of a bare name.

The spelling `'{total + 1}'` was considered and dropped: it needs a raw-text region
in the lexer, which `surface_dcg` already owes for extraction, and there is no reason
to spend that token on a form that reads fine as a function call.

## Typing

Types: `Int`, `Str`, `Term`, `List(T)`. No `Bool`, because predicates are goals and a
boolean value never needs to exist. Column types are required (LANG.md:17) and that
requirement is what does the work in three separate places: overload resolution,
range restriction, and the collision.

Rules the checker enforces, each with its graded receipt:

| rule | receipt |
|---|---|
| arithmetic is Int on both sides, result Int | `plus_is_int_only` |
| `+` does NOT concatenate text | `plus_is_int_only` |
| a comparison in a value position is an error | `comparison_is_not_a_value` |
| function arguments must match a declared signature | `function_argument_type_rejected` |
| an undeclared function is an error, not a rel guess | `unknown_function_rejected` |
| interpolation parts must be Int or Str | `interpolation_of_term_rejected` |
| a rel name in an expression is an error | `rel_read_in_expression_rejected` |
| every well-typed program checks clean | `well_typed_programs_check` |

**`+` is Int only.** v5 overloads it (`int + int` adds, `text + text` concatenates)
and pays for it with a special case documented in `docs/reference/syntax.md`: "mixed
int/text is a typecheck error (interpolate or int(..))". Interpolation already covers
concatenation with better ergonomics (`"${a}${b}"`), so the overload buys nothing and
costs a rule. `str("a") + str("b")` is rejected here, deliberately, and that is a
deviation from v5 worth noticing during the port.

**Interpolating an Int auto-converts.** The corpus writes `"unwrap in a changed file
over budget (${n} > 10)"` and `"${path}:${line}"`; requiring `to_str(n)` there taxes
69 files for zero information. `to_str` stays in the stdlib for the cases outside a
string.

**Interpolating a Term, enum, or struct is a compile error.** No canonical text form
exists that the language may pick without inventing one, and inventing one silently is
how `[object Object]` happens. The error names the type; the fixes are projecting the
field you meant or calling `digest`.

**Interpolation desugars before typing.** `"banned word ${word} at ${path}"` becomes
`concat([str("banned word "), word, str(" at "), path])`, graded exactly by
`interpolation_desugars_to_concat`. The desugaring never inserts a `to_str` around a
hole, because deciding whether to insert one would require the type, and that is the
type-directed elaboration the collision rule rejects. Instead `concat` accepts any
displayable part and converts at run time.

## Evaluation semantics

Expressions evaluate per binding row, after the joins that bind their variables. Body
goals run left to right: an atom binds, a `:=` computes, a comparison filters. The
head evaluates last.

Receipts, from `examples/graph-measure.dl:64-71` transcribed into the lab:

```
union_size(a, b, ua + ub - sh) <-
    callee_set_size(a, ua), callee_set_size(b, ub), shared_count(a, b, sh);
jaccard(a, b, sh * 100 / u) <-
    shared_count(a, b, sh), union_size(a, b, u), u > 0, sh * 100 / u >= 40;
```

```
union_size("db.rs","cli.rs",12)      10 + 8 - 6
union_size("db.rs","ui.rs",13)       10 + 4 - 1
jaccard("db.rs","cli.rs",50)         6*100/12 = 50, kept
                                     1*100/13 =  7, filtered by >= 40
```

The range join, from `.dl/no-new-eprintln.dl:47-51`:

```
eprintln_waived(path, line_number) <-
    eprintln_waiver_line(path, waiver_line), eprintln_hit(path, line_number),
    waiver_line >= line_number - 1, waiver_line <= line_number;
```

```
eprintln_waived("src/db.rs",40)      waiver 39: 39 >= 39 and 39 <= 40
                                     hit 88 dropped: 39 >= 87 is false
waiver_note("src/db.rs",40,"eprintln at src/db.rs:40 is waived")
```

The bind, from `examples/arch-conformance.dl:33`:

```
file_stem(f, s, t) <- module_edge(f, _), layer(p, t),
                      s := strip_prefix(f, p), s != f;
```

```
file_stem("src/db/conn.rs","conn.rs","storage")
```

`layer("src/cli/","ui")` produces `s == f`, so the `!=` drops it. That single rule
carries all three of the new goal forms: a wildcard atom argument, a `:=` whose result
feeds both a later goal and the head, and a comparison over the bound name.

Range restriction is a compile error, not a runtime one:

```
unbound_bare      REJECTED unbound_variable(missing)
unbound_quoted    REJECTED unbound_variable(missing)
```

## The stdlib

13 names, 14 rows, all pure. Every entry has a corpus receipt or is the target of a
desugaring. Occurrence counts are `grep -rhoE 'name\s*\(' --include=*.dl .` over the
repo, so they include comments and are a floor, not a census.

| function | signature | why it is here |
|---|---|---|
| `len` | `Str -> Int` | message building, budget checks |
| `len` | `List(A) -> Int` | second row, same name; see ambiguity 3 |
| `lower` | `Str -> Str` | case-blind compare (1 occurrence) |
| `trim` | `Str -> Str` | 13 occurrences |
| `split` | `(Str, Str, Int) -> Str` | 37 occurrences, the most-used function in the corpus; negative index counts from the end |
| `join` | `(List(Str), Str) -> Str` | 10 occurrences |
| `replace` | `(Str, Str, Str) -> Str` | 19 occurrences |
| `strip_prefix` | `(Str, Str) -> Str` | 11 occurrences; the arch-conformance tier rule |
| `strip_suffix` | `(Str, Str) -> Str` | symmetry; v5 ships both |
| `concat` | `List(Display) -> Str` | the desugaring target of interpolation; not a table row in the lab because `Display` is a class, not a type (ambiguity 4) |
| `to_str` | `Int -> Str` | the explicit conversion, for use outside a string |
| `abs` | `Int -> Int` | arithmetic completeness |
| `digest` | `A -> Str` | the input-digest salt (consolidation item 4) and the transform law; the one escape for a value interpolation refuses |
| `apply` | `(Term, A) -> A` | optimistic update; see below |

Deliberately absent:

- **`min` / `max` as scalars.** The corpus has 21 `max(` and 15 `min(` occurrences and
  they are AGGREGATES (`resp_latest(ep, max(b))`, `examples/gh-cache.dl:99-102`), a
  different tier with different semantics. Two-argument scalar `min`/`max` would share
  a spelling with them. That collision is ambiguity 1.
- **`contains` and regex.** A predicate over strings is a goal, not a value, and
  adding it would drag in a `Bool` type for one function. v5 spells this `=~ /re/`,
  a comparison-shaped body form; it belongs with the extraction lab, which owns
  regex literals anyway.
- **`round` / floats.** 7 occurrences of `round(` in the corpus, all inside percentage
  arithmetic that `/` already truncates. There are no floats in tier 0.

### `apply` and the optimistic-update shape

`quote` needs one worked receipt to earn its place, and optimistic update is it. Inside
a quote the name `it` is reserved and stands for the value `apply` will supply later:

```
pending(id, quote(it + delta))      <- queued(id, delta);
optimistic(id, apply(patch, value)) <- pending(id, patch), base_value(id, value);
```

```
pending("cart",t(+,[it,3]))
optimistic("cart",13)
```

The patch is stored as data in a term column, moves around like any value, and is
applied against whatever base exists at use time. Graded by
`stored_patch_applies_to_a_later_base`.

## Numbered ambiguities found

1. **Scalar `min`/`max` collide with the aggregate `min`/`max`.** The corpus uses the
   aggregate form 36 times and the scalar form zero times, so this lab omits the
   scalar. Before the aggregate lab picks a spelling, decide whether `min(a, b)` and
   `min(x)` may share a name (arity-directed dispatch, which reintroduces a mild form
   of the type-directed problem) or whether one of them is renamed.

2. **`apply` is the one function whose result type is unchecked.** Its signature is
   `(Term, A) -> A`, and `Term` is opaque, so the checker cannot verify that the frozen
   tree actually produces an `A`. `apply(quote("x"), 1)` types as `Int` and evaluates
   to a string. Every other expression form in tier 0 is statically sound. Options:
   accept a run-time check at the apply site, parameterize the term type
   (`Term(Int)`), which reintroduces inference through `quote`, or drop `apply` and let
   patches be consumed only by rules that pattern-match them.

3. **Overloading `len` is safe only because column types are required.** Both `len`
   signatures are considered and exactly one survives, because the argument's type is
   always known by the time the call is typed (graded by
   `len_overload_resolves_by_declared_column_type`). If any future column type can be a
   type variable (a generic rel), overloaded stdlib entries stop having a principal
   type. Either state that column types are always ground, or forbid overloading.

4. **`Display` is a class, not a type, and it is the only ad-hoc polymorphism in tier
   0.** `concat` accepts `Int` or `Str` parts and converts. That is a one-off rule in
   the checker with no general mechanism behind it. Either name it as a class the
   language has (which is a type-system feature well past tier 0) or restate `concat`
   as strictly `List(Str)` and make the desugaring insert `to_str`, which requires
   types at desugaring time and therefore contradicts the collision rule. This is the
   one place where the syntactic-elaboration discipline pays a cost.

5. **No `Bool` type means predicate-shaped functions have no home.** `contains`,
   `matches`, `starts_with` are all goals in this design. That is coherent, and it
   means a program cannot store the result of a test in a column. v5 works around this
   with 0/1 Int columns. Decide whether a computed flag column is wanted, because
   adding `Bool` later changes the comparison rules.

6. **`==` on a `Term` column is structural equality on a value the checker cannot see
   inside.** The lab allows it (both sides are `Term`, so the types match). Whether
   two quotes built from different rules can be meaningfully compared depends on a
   canonical term encoding that does not exist yet (the same gap as `digest`), which
   in turn waits on the `struct` specification the audit files as 18a.

7. **Division is truncating and there are no floats.** `sh * 100 / u` in
   `examples/graph-measure.dl:70` relies on that. Nothing in the spec says so. If a
   float type ever arrives, every existing percentage rule changes meaning silently.
   State the Int-only arithmetic law now.

8. **The `_` wildcard in an atom argument has no stated relationship to a fresh
   variable.** The lab treats it as "match anything, bind nothing", which is what v5
   does. It matters here for the first time because an expression referring to a
   wildcard is now writable and must be an error (the lab throws
   `wildcard_in_expression`); nothing in LANG.md mentions `_` at all.

## What this unblocks

- **Ratchet comparisons.** `.dl/no-new-eprintln.dl` and `.dl/rails.dl` are built on
  `count(...) > baseline` and range joins over line numbers. The range join runs here
  today.
- **Message columns.** The `diag` sink (55 corpus files) is a rel whose message column
  is an interpolated string. Interpolation plus a `Str` column is the whole mechanism;
  the sink itself is a separate question.
- **Derived measures.** `examples/graph-measure.dl`'s jaccard, degree, and union
  computations are head arithmetic and nothing else. They run here today.
- **Optimistic update.** A term column plus `quote` plus `apply` stores an intended
  change as data and applies it later, which is what an optimistic UI write needs and
  what a plain value column cannot express.
- **Path and identifier construction.** `"${repo}#PR-${num}"` in
  `.dl/git-graph.dl` and `"${path}:${line}"` across the rails are interpolation.
- **The digest salt.** `digest` is the function the change-recurrence salt (item 4 of
  the consolidation) needs; it had no home in the surface before this.

## Tier placement

Tier 0, with one exception and two dependencies.

**In tier 0 without qualification:** arithmetic, comparison, `:=` bindings,
head-position expressions, `quote`, pure function application, interpolation, and the
whole type layer above. None of them touches time, the tick, keys, effects, or the
world. The lab's interpreter is level rules over a set of facts and nothing else.

**The exception: head expressions inside a recursive stratum are rejected.** Datalog
terminates because heads only move existing values around; an arithmetic head can mint
new values forever. `std/entry.dl` caps its reachability recursion at depth 64 by hand
and documents that a longer real path silently disappears, which is the workaround for
exactly this. `AUDIT.md` 18f files it as a design gap. The ruling here is the
conservative one, graded:

```
recursive_head    REJECTED head_expression_in_recursive_rule(depth/2)
```

`examples/graph-measure.dl`'s arithmetic heads are all in non-recursive strata, so the
ban costs the corpus nothing today. `pre` breaks the stratum, so `merge_family.md`'s
`counter(name, total + 1) <+ increment(name, _), pre(counter(name, total))` is
unaffected: it reads T-1, not itself. Direct self-reference only in this lab; the
mutual case needs the stratum SCC, which belongs to the stratification check.

**Two dependencies on work outside tier 0:**

- `digest(A) -> Str` needs a canonical term encoding, which needs `struct` specified
  (`AUDIT.md` 18a). It types fine here; its runtime is a placeholder hash.
- `apply(Term, A) -> A` is statically unsound as noted in ambiguity 2. It can ship in
  tier 0 with a run-time check, or wait.

**Explicitly not in this lab, and not tier 0 blockers for it:** aggregates
(`count`/`max`/`sum`, 76 corpus files), negation, extraction ops, `?` queries, and the
`diag`/`gen` sinks. Expressions compose with all of them and constrain none of them,
with the single exception of the scalar-versus-aggregate naming collision in
ambiguity 1.

## Deviations from the LANG.md snapshot

- **`!=` is written `\=` in the lab.** Prolog's `!` is a solo character, so `!=` does
  not tokenize. The surface spelling stays `!=`.
- **Interpolated strings are marked `istr("...")`.** Prolog's reader has no
  interpolation. The surface spelling is a plain double-quoted string with `${name}`
  holes; the lab parses the same text with its own scanner and the desugaring is
  graded.
- **A bare lowercase atom in an argument or expression is a variable; literals are
  `lit(5)` and `str("x")`.** The opposite of prolog's convention, chosen so the rule
  bodies in the lab read like the surface. Facts carry raw values.
- **`<=` is declared as an operator** because prolog spells its own `=<`.
- **Only level rules.** The tick, edge rules, keys, and `pre` are `merge_family.pl`'s
  subject and are not modelled here. The one place the two labs meet is the head
  expression `counter(name, total + 1)`, and the recursion ruling above says what
  happens to it under `pre`.
- **`rel` declarations are `rel_decl/2` facts, not parsed syntax.** Column types are
  present and required, since three separate rulings lean on them.
- **`struct` and `enum` columns are not modelled.** The lab's `Term` type stands in
  for every column that holds structure. Where the ruling would differ for a nominal
  struct type it is called out (ambiguities 2 and 6).
