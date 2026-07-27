# astgrep_patterns: verdict on the quoted-DSL pipeline

Lab: `v6/prolog/labs/astgrep_patterns.pl` + `v6/prolog/labs/node_types_fixture.json`.
Run: `swipl -q -l v6/prolog/labs/astgrep_patterns.pl -g go -g halt`. 36 PASS, exit 0,
empty stderr.

## What got proven

| Pass | Mechanism | Graded by |
| --- | --- | --- |
| import | `library(http/json)` reads node-types.json, asserts `node_kind/1`, `node_field/3`, `node_field_multiple/3`, `node_children/2`, `node_children_multiple/2`; anonymous entries dropped | `grammar_kinds`, `grammar_drops_anonymous`, `grammar_fields`, `grammar_children`, `grammar_closed` |
| parse | 40-line DCG over pattern text; `$UPPER`, `$$$`, `$$$NAME`, calls, member access, string/number literals | `parse_call`, `parse_member_ellipsis`, `parse_named_ellipsis`, `parse_literals`, `parse_nonlinear` |
| surface | SWI quasiquotation `{|sg||foo($A, $$$REST)|}` produces the identical term at read time | `quasiquote_same_term` |
| check | pattern term x grammar facts -> `ok(Annotated)` or `bad(Reason)`, total | `check_accepts_call`, `check_resolves_property_kind`, `check_accepts_named_ellipsis`, three refusals |
| lower 1 | unification match over `node(Kind, Fields, Children)`, metavariable holes, sibling-list holes, non-linearity by term identity | 8 match checks |
| lower 2 | tree-sitter query s-expression with `@capture`, `#eq?` predicates and `.` anchors | 4 exact-string checks + 2 blocked checks |
| derived | term surgery on a checked pattern, re-checked, then lowered both ways | 2 checks |

The refusals are driven by the imported facts, not hardcoded. Probe: retract
`node_field(call_expression, function, _)` and assert a version that includes
`string`, and `"hello"($A)` flips from refused to accepted with
`ann(string, pstring(hello))` in callee position.

## 1. Surface embedding: quasiquotation wins, but not for the reason it looks like

SWI quasiquotation was tried, not assessed in prose. It works and it is six lines:

```prolog
:- quasi_quotation_syntax(sg).
sg(Content, _SyntaxArgs, _VariableNames, Pattern) :-
    with_quasi_quotation_input(Content, Stream, read_string(Stream, _, Text)),
    parse_pattern(Text, Pattern).
```

`{|sg||foo($A, $$$REST)|}` in a clause body reads as the pattern term. The
graded check compares it against `parse_pattern/2` on the same text and they
are `==`.

The interesting property is not the syntax, it is WHEN the parse runs.
Quasiquotation expands at read time, so:

- the pattern is a compile-time constant, and `parse_pattern` never runs at tick time;
- a syntax error in the pattern is a compile error, at the character offset;
- the grammar check can run in the same phase, so an impossible pattern is a
  compile error too.

A body operator spelled `cst ~ sg { foo($A) }` needs exactly the same thing
from the outer lexer that quasiquotation needs: a raw-text region the outer
lexer can find and skip without understanding the inner language. That is the
whole cost, and it is identical for both spellings. Where they differ is that
the `~` operator reads as a runtime relation between a CST value and a pattern
value, which invites an implementation that parses and checks at tick time.
That is the wrong default: it turns a static error into a per-row error, and it
re-parses a constant on every tick.

Verdict:

1. Primary form is a compile-time quoted region with an explicit closing
   delimiter, `{|sg|| ... |}` shape rather than `sg{ ... }`. Brace balancing
   fails on patterns that legitimately contain unbalanced braces (a JS pattern
   like `function $F() {` is a real ast-grep pattern), and the outer lexer must
   not need a nested parser to find the end.
2. The body operator stays, but as a relation between an already-checked
   pattern VALUE and a CST value: `match(cst, Pattern)` where `Pattern` came
   from a quoted region or from a rule. Not as the quoting construct itself.
3. `parse_pattern` remains available at runtime for exactly one caller: derived
   patterns built from text held in rows. Those cannot be checked at compile
   time, so they get the `ok/bad` result and a refusal row, which is the right
   failure mode for data-driven patterns.

Two entry points, two failure modes, and the failure mode is visible from the
spelling. That is the part worth keeping.

## 2. What the pattern type system needs from the grammar facts

Concretely, per fact:

| Fact | What the checker cannot do without it |
| --- | --- |
| `node_kind/1` | close the world; give `kind_not_allowed` a printable domain; reject a made-up kind |
| `node_field(Kind, Field, AllowedKinds)` | resolve every pattern position to a concrete node kind. This IS the typing environment; the check is set membership against the field's declared type list |
| `node_field_multiple/3`, `node_children_multiple/2` | decide whether `$$$` is legal at a position. Without them, ellipsis placement is unchecked and `$$$.method()` parses to a member access whose object is a list |
| `node_children(Kind, AllowedKinds)` | type the unlabelled positional children that `$$$` fans over. Fields and children are different slots and the pattern language reaches both |

Two findings that are more than bookkeeping.

**The check returns an annotation, and the native lowering needs it.** The
check is not a predicate that says yes or no. It returns the pattern with a
concrete node kind attached at every position, and that annotation is what the
emitter prints as the query node name. The same pattern token `method` resolves
to `identifier` in callee position and `property_identifier` in property
position, because the two slots declare different type lists. Graded by
`check_accepts_call` and `check_resolves_property_kind`. Consequence for
dependency order: the reference lowering runs on unannotated pattern terms and
needs no grammar at all, while the native lowering cannot emit a single node
name without the check having run. The grammar import gates one backend and not
the other.

**node-types.json cannot say what a bare pattern name denotes.** A pattern
token like `foo` could be an `identifier`, a `property_identifier`, a
`statement_identifier`, or a keyword, and the JSON has no field that says so.
This lab carries a two-row bridge table, `pattern_leaf_kind/1`, and it is the
only hand-written grammar knowledge in the file. Real ast-grep does not need
it, because it parses the pattern TEXT with the target grammar, so every token
comes back already kinded. That is the honest resolution and it changes what
`ts_grammar_import` has to deliver: importing node-types.json gives the
checker its type environment, but a general pattern front end needs the target
parser itself, not just its schema. A schema-only import works per language
with a small hand-written bridge, and does not generalise to an arbitrary
grammar dropped in at link time.

`"required"` from node-types.json is deliberately not imported. Patterns are
partial by construction and never owe a required field, so the flag has no
checker to feed.

## 3. Patterns as terms: derived patterns, worked

Because a pattern is a ground term, a rule can build one. Worked example, a
data-driven rename codemod:

A rel holds the renames, one row each, `rel rename(old: Key(Str), new: Str)`,
seeded with `rename("method", "invoke")`. A level rule derives two pattern
terms per row:

- match term: `pcall(pmember(pmeta('OBJ'), pident(Old)), [pellipsis('ARGS')])`
- rewrite term: `pcall(pmember(pmeta('OBJ'), pident(New)), [pellipsis('ARGS')])`

Both are ordinary ground terms in a column, which is what clocked_terms PART B
already licenses. Both go through `check_top` against the imported grammar, so
a rename to a name that cannot sit in the property slot is refused before it
touches a file, and the refusal is a row with a reason rather than a crash. The
match term goes to the reference lowering, which finds the call sites and binds
`OBJ` and `ARGS`. The rewrite term is instantiated with those same bindings and
printed back to source. Nothing in the pipeline was written per rename. Adding
a row adds a codemod, and deleting the row retracts the codemod along with its
consequences, because the match results are a level view.

The lab ships the miniature of this and grades it.
`rename_callee/3` is three lines of term surgery; the derived term type-checks,
reference-matches, and emits a query with `"g"` in place of `"f"`, all without
re-parsing any text. `literalize_callee/2` is equally valid term surgery that
produces an illegal pattern, and the re-check catches it with
`kind_not_allowed(string, [identifier, member_expression])` instead of the
matcher silently finding nothing. That distinction is the argument for
patterns-as-terms over patterns-as-strings: a string codemod pipeline can only
report zero matches, and zero matches is indistinguishable from a broken
pattern.

One caveat that the emission lowering makes concrete. The rewrite step needs
`ARGS`, a named sibling-list binding, and named `$$$` has no tree-sitter query
form. So the codemod route works on the reference backend and does not work on
the native one for any pattern that carries a sibling list into its rewrite.

## 4. The two lowerings are not equivalent, and the lab measures the gap

The babel two-path claim is that one term drives both backends. It does, but
the backends do not agree, and three divergences showed up while making the
exact-string checks pass:

1. **Named `$$$` is inexpressible natively.** Tree-sitter queries capture
   nodes, never sibling lists. Emission returns `blocked(named_ellipsis('REST'))`
   rather than dropping the binding. Graded.
2. **Child matching is a subsequence by default.** A tree-sitter node pattern
   matches its children in order but not exhaustively, so a naive
   `(arguments (_) @A)` also matches `f(a, b)`, while `foo($A)` in the
   reference matcher means exactly one argument. The fix is the `.` anchor
   operator, interleaved at the list head, the list tail, and between any two
   non-ellipsis neighbours. The lab emits
   `(arguments . (_) @SAME_1 . (_) @SAME_2 .)` and the anchors are in the
   graded strings. An arg list of exactly `[]` stays inexpressible, because a
   query cannot say "no children at all", so `foo()` returns
   `blocked(empty_child_list_inexpressible)`. Graded.
3. **Non-linearity means two different things.** The reference lowering binds a
   metavariable to a subtree and re-binding demands term identity, so `$A`
   twice means the same subtree. The native form is `(#eq? @A_1 @A_2)`, and
   `#eq?` compares source TEXT. For identifier captures the two agree. For
   captures over expressions they diverge on whitespace and comments.

None of these is fatal and all three are cheap to state. What they say about
the design is that a two-path lowering owes a written equivalence law and a
refusal channel, not just two emit functions. The refusal channel is the part
that is easy to skip and expensive to skip: an emitter that silently drops a
named `$$$` produces a query that runs, returns matches, and is wrong.

## 5. Ambiguities found in LANG.md

1. **A quoted DSL has no syntactic home.** The surface is "Keywords: `enum`,
   `struct`, `rel`, `bind`. Nothing else." A quoted region is none of those and
   is not a rule arrow. Is a quoted DSL a value expression, a body atom, or a
   third syntactic category? The lab needs it to be a value, because a rule
   derives one.
2. **A pattern's type is parameterised by an imported artifact.** Column types
   are required, so a column holding a pattern needs a type. The honest type is
   something like `Pattern(JsGrammar, call_expression)`: the grammar it was
   checked against, and the root kind it resolved to. LANG.md's type surface
   has `Key(Type)` as a type-position constructor but no notion of a type
   parameterised by a link-time import. Same slot, different question.
3. **The grammar import is neither a fact nor an effect.** node-types.json is
   an external file. As an effect it would fill at tick time, but the pattern
   check wants to run at compile time. LANG.md says an effect is "a lazy rel
   whose oracle is the world"; that makes a compile-time check depend on a
   world fill. Either grammar import is a third thing (link-time, like `bind`),
   or the check must be allowed to run at tick time and report rows.
4. **`bind` is the closest existing construct and is reserved for effects.**
   "Exactly one bind per effect" and "Program text never names a transport".
   Grammar import has the same shape: link-time, one per language, program text
   should not name the .json path. If `bind` generalises to link-time imports,
   say so; if not, a second link-time keyword arrives and the four-keyword
   claim goes to five.
5. **List-valued bindings have no relational shape.** A `$$$NAME` binding is an
   ORDERED sibling list. LANG.md's relational model has flat rows, and
   surface-boil's storage note says every type is a table with a dense
   surrogate. An ordered list is either a term in a column (licensed by
   clocked_terms PART B) or a junction table with an order column. The two
   choices give different answers for equality, indexing, and IVM deltas on a
   partial list change. Unaddressed.
6. **No stated law that two lowerings of one term must agree.** The two-path
   design is written down in surface-boil ("native driver query || unification
   reference semantics, babel two-path") with no conformance obligation. This
   lab found three divergences in the smallest possible instance. The
   obligation should be written as part of the design, along with the rule that
   a backend which cannot express a construct refuses rather than approximates.
7. **Match results are level, applied codemods are edge, and the boundary is
   unstated.** Pattern matches over a CST retract when the file changes, which
   is `<-`. A codemod actually applied to disk is an occurrence that cannot
   un-happen, which is `<+` plus a world effect. LANG.md gives both arrows but
   never works an example where the same rule chain crosses from one to the
   other, and the codemod route is exactly that crossing.
8. **`Key(Type)` versus arrow-as-functional-dependency, one more data point.**
   LANG.md lists this as open. The check pass here is a pure function from
   (grammar facts, pattern term) to result, so as a rel it is
   `rel checked(pattern: Key(Pattern), result: CheckResult)` and wants nothing
   from the `->` arrow. The `->` arrow earns its keep when the right side is
   filled by the world. A compile-time-decidable function does not need it.
   That is evidence for the two constructs being genuinely different rather
   than redundant, contrary to the "Redundant?" framing.

## 6. Tier-order implication

`ts_grammar_import` is listed `unbuilt, []` in ARCH.pl. The empty dependency
list is right and the lab confirms it: JSON import plus fact assertion is about
40 lines, needs `library(http/json)`, and touches neither `desugar_machinery`
nor `kernel_sql_lowering`. It can move to `labbed` on the strength of this lab.

What the lab adds is the edges on the other side.

```
                          (nothing)
                              |
                     ts_grammar_import          <- promote to labbed
                              |
                        pattern_check
                       /             \
   emit_ts_query (native backend)   derived_patterns (codemod route)
                                          |
                                    terms-in-columns storage
```

Read as dependencies:

- **Reference lowering depends on nothing.** It matches unannotated pattern
  terms against CST terms. It is available before any grammar work lands, which
  makes it the right first backend and the right oracle for the second.
- **Pattern check depends on ts_grammar_import** and nothing else.
- **Native query emission depends on pattern check**, transitively on
  ts_grammar_import, because it cannot print a node name without the resolved
  kind. Any plan that schedules a native backend before grammar import is
  scheduling it before it can be written.
- **The quoted surface depends on surface_dcg** (currently `unbuilt`,
  `[desugar_machinery]`) for one specific thing: a raw-text token region the
  outer lexer can skip. That requirement should be written into `surface_dcg`
  now, because retrofitting a raw-text mode into a finished lexer is the
  expensive version. It is one token class if planned and a re-lex if not.
- **The codemod route depends on terms-in-columns**, and on the list-storage
  question in ambiguity 5. That question belongs to whichever task owns the
  type-to-sqlite lowering, and it is currently owned by nobody.

Suggested ARCH.pl edits, for whoever owns that file:

```prolog
task(ts_grammar_import,   labbed,  []).                       % astgrep_patterns lab
task(pattern_check,       labbed,  [ts_grammar_import]).      % same lab
task(quoted_dsl_surface,  unbuilt, [surface_dcg, pattern_check]).
```

and a note on `surface_dcg` that it owes a raw-text region token.

## 7. Deviations from LANG.md's lab laws

1. LANG.md says reference semantics go in prolog and lowering is described in
   the .md "unless the lab is specifically about emission". This lab is
   specifically about emission, so lowering 2 is code and is graded on exact
   strings. Declared per the exemption.
2. The CST term is `node(Kind, Fields, Children)` rather than sexp_cst.pl's
   `node(Kind, Span, Children)` with `field/2` items mixed into the child list.
   Two reasons. Fields and children are separate slots in node-types.json, so
   separating them in the term makes the check and the match read the same
   shape the grammar declares. And the span is resolved to source text on
   leaves and then DROPPED, so that structural equality of two node terms means
   "same subtree", which is what makes a non-linear pattern work by unification
   with nothing else. Graded by `cst_span_free`: the two `x` nodes in
   `log(x, x)` are `==` after normalizing, and would not be with spans present.
   A real implementation keeps the span in a position excluded from equality.
3. The fixture declares 12 named kinds rather than the 8 sketched, so that the
   grammar is CLOSED: every kind named in any field or children type list is
   itself declared. `grammar_closed` grades it. An unclosed subset makes the
   checker refuse good patterns for the wrong reason, and the refusal names a
   missing kind that looks like a user error.
4. The fixture keeps six anonymous entries (`(`, `)`, `,`, `.`, `;`,
   `function`) because a real node-types.json has them and dropping them from
   the fixture would hide the filter the importer has to have.
   `grammar_drops_anonymous` grades that they never become node kinds.
5. `pattern_leaf_kind/1` is hand-written, two rows, and is the only grammar
   knowledge in the file that the JSON did not supply. Section 2 explains why
   it is unavoidable for a hand-rolled pattern parser.
