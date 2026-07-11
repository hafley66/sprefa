# String ergonomics design: body binds and concatenation

## Context

Item 13 in `plans/2026-07-11-codex-feedback-queue.md` asks for grammar sign-off
before adding two S3/S4 conveniences:

```dl
resolved(callee) <- raw_edge(callee_q),
  callee = replace(callee_q, ".", "::").

endpoint("https://" + host) <- service(host).
```

The base is not a blank slate. `plans/2026-07-09-body-binds-text-concat.md` records
an earlier design, and base `07030b1` already contains its prototype behavior:

- `Term::Call` represents pure value functions and `Term::Arith(Add)` represents
  both numeric `+` and type-directed text concatenation (`src/ast.rs:197-217`).
- `BodyItem` has no bind variant; comparisons remain
  `BodyItem::Cmp(Constraint { lhs, op, rhs })` (`src/ast.rs:275-337`).
- `body_item()` parses a leading `ident(...)` as a relation/effect atom and
  otherwise parses a constraint (`src/parse.rs:506-545`). `constraint()` parses
  an expression, `CmpOp`, and expression. `expr()` already gives `* / %` higher
  precedence than `+ -`, and term-position calls parse recursively
  (`src/parse.rs:1185-1292`).
- `body_sql_ex()` first collects every positive-atom variable and its column
  type, then treats `Eq` with a fresh bare variable and a `Call`/`Arith` other
  side as a computed bind. It records the expression SQL in `canon`; all other
  equalities become filters (`src/lower.rs:239-333`).
- `term_sql()` lowers text/text `+` to SQLite `||`, keeps int/int `+`, and rejects
  known mixed pairs. String-consuming paths use `term_sql_text()`, which decodes
  `Type::Sym` through `_strings` (`src/lower.rs:43-89, 118-182`).

This proposal therefore ratifies and tightens an existing surface rather than
inventing a second one. The implementation follow-up should preserve the useful
behavior, make the grammar/ambiguity contract explicit, and pin the `sym` rules
that became important after string interning.

## Decisions

### Exact grammar

The recommended concrete grammar is:

```ebnf
body_item       ::= positive_atom
                  | "!" positive_atom
                  | comparison_or_bind
                  | source_op

comparison_or_bind
                ::= expr cmp_op expr

cmp_op          ::= "=" | "!=" | "<" | "<=" | ">" | ">=" | "=~" | "~~"

expr            ::= additive
additive        ::= multiplicative (("+" | "-") multiplicative)*
multiplicative  ::= primary (("*" | "/" | "%") primary)*
primary         ::= IDENT
                  | STRING
                  | INT
                  | interpolated_string
                  | path_literal
                  | call
                  | "(" expr ")"
call            ::= IDENT "(" (expr ("," expr)*)? ")"
```

There is deliberately **no separate parser production for bind**. After parsing
to `Cmp(Eq)`, typecheck/lowering classify the item as a computed bind iff:

1. the left side is a bare, non-wildcard variable;
2. that variable is not bound by any positive atom and was not introduced by an
   earlier computed bind; and
3. the right side is value-producing (`Call` or `Arith`, including a `+` tree).

Positive relation atoms are declarative and order-free, so a variable appearing
in any positive atom is bound even if the atom is textually later. Bind-to-bind
dependencies are ordered: a right-hand variable may come from any positive atom
or an earlier bind, never a later bind.

Only the conventional left-to-right spelling introduces a value:

```dl
x = replace(y, ".", "::")       # candidate bind
replace(y, ".", "::") = x       # equality filter; never a bind
```

The base prototype accepts the reverse spelling symmetrically. Deprecate that
unpublished convenience rather than make assignment visually bidirectional. A
short compatibility window may warn on reverse fresh-variable binds before they
become an unbound-variable error.

### Ambiguity with existing `CmpOp::Eq`

The token grammar cannot decide whether `=` means introduction or equality,
because boundness comes from relation schemas and the whole positive body:

```dl
x = replace(y, "a", "b")        # bind if x is fresh
pair(x, y), x = replace(y, "a", "b")  # filter because x is already bound
```

This is intentional contextual classification, not a shift/reduce ambiguity.
Both forms parse as the existing `BodyItem::Cmp`; no lexer token and no AST enum
case is added. The `has_computation`/freshness gate prevents these existing
equalities from changing meaning:

- `x = y` remains equality, never alias introduction;
- `x = "literal"` remains equality and errors if `x` is otherwise unbound;
- a bound `x = fn(...)` remains a filter;
- only `=` can bind; `!=`, ordering, regex, and glob operators never introduce;
- `_ = fn(...)` is an error, not a discard bind.

This choice keeps `Cmp` consumers (source evaluators, pin detection, frontend
rewrites, diagnostics) structurally compatible. Typecheck and lower must share one
classification helper or an equivalently exhaustive test matrix so they cannot
disagree on whether a row introduces a variable.

### Concatenation spelling: ranked options

1. **Recommended — overload `+` by operand type.** `int + int` remains numeric;
   two text-base operands concatenate; a known mixed int/text pair is an error.
   The grammar and precedence already exist, chained expressions read naturally,
   and the base prototype proves the lowering seam.
2. **`concat(a, b, ...)` pure function.** Unambiguous and variadic, but verbose,
   nests poorly with other functions, and creates a second spelling beside
   interpolation. It remains a possible future variadic convenience, not the
   foundational operator.
3. **Expose SQL-style `||`.** No type overload, but leaks the storage backend,
   requires a new lexer/operator surface, and is unfamiliar outside SQL/shell.
   Reject for the language surface; `||` remains an implementation detail.

Choose option 1. Exact typing is:

| Left | Right | Result | Lowering |
|---|---|---|---|
| `int` | `int` | `int` | SQL `+` / integer evaluator |
| any text-base type | any text-base type | unbranded `text` | decode `sym` operands, SQL `||` / string evaluator |
| `int` | text-base, either order | error `plus-mismatch` | no SQL |
| unknown | known side | adopt known side for checking | lower consistently with inferred type |
| unknown | unknown | legacy `int` default | SQL `+` |

Text-base means `text`, `path`, `file`, `dir`, `repo`, `rev`, and `sym` at this
surface. Concatenation drops brands/path-ness and produces plain `text`.
Interpolation remains the clearest spelling when coercing an integer into text;
there is no implicit int-to-text conversion through `+`.

### Lowering shape

Keep the current expression-in-canon design. A computed bind does not materialize
a relation, CTE, or new SQL column by itself. It inserts `target -> expression SQL`
and `target -> inferred Type` into the same per-rule maps used for atom variables.
Later binds, comparisons, negations, and the head see the target through those
maps. The expression may inline at each use; pure builtins make re-evaluation
semantically safe.

Lowering order is:

1. collect all positive atoms, canonical SQL cells, and declared column types;
2. walk comparisons in source order, installing computed binds or emitting
   filters; then
3. lower negations after binds so a bind used inside `NOT EXISTS` correlates to
   the outer expression instead of becoming a new local variable.

Representative result:

```dl
resolved(caller, callee) <- raw_edge(caller, callee_q),
  callee = replace(callee_q, ".", "::").
```

```sql
INSERT OR IGNORE INTO rel_resolved ("caller", "callee")
SELECT r0."caller", replace(r0."callee_q", '.', '::')
FROM rel_raw_edge r0
```

No “assignment order” is imposed on positive atoms. Only computed-bind chains use
textual order because a later expression may consume an earlier computed value.

### `sym`-column interaction rules

`Type::Sym` is an interned `StringId` stored as INTEGER. The language surface is
text-like, but producing arbitrary text is not the same as interning it. Apply
these rules consistently:

1. A `sym` used as an argument to a string function, interpolation, or text `+`
   is decoded through `_strings` at the text-consumption boundary.
2. A function or concatenation result is plain `text`, never `sym`, even when all
   inputs were `sym`.
3. Comparing a computed text result to a `sym` column hashes the text side with
   `sprf_sym(...)`; the `sym` cell remains an integer and does not decode.
4. Joining `sym` to `sym` remains an integer equality. Comparing a string literal
   to `sym` uses its compile-time `StringId`.
5. Ordering, regex, glob, functions, interpolation, and concatenation consume
   lexical text and therefore decode `sym`.
6. A computed bind cannot fill a `sym` head column. Hashing alone would create an
   ID with no guaranteed `_strings` row, breaking later decode. The error must say
   to make the destination `text` or route an already interned `sym` variable.
7. Concatenating branded/path-like values produces unbranded `text`; it cannot
   silently regain the original brand in a head column.

These rules preserve the performance shape in `eq_cond()` and `head_term_sql()`:
decode only for text consumption, retain integer joins/filters wherever possible.

### What stays an error

- Function calls and arithmetic/concatenation in relation-atom binding slots:
  write a separate computed bind or put the expression in the head.
- A bind whose right-hand variable is absent from all positive atoms and earlier
  binds (`unbound-bind`, naming both variables and the ordering fix).
- A bind depending on a later bind.
- Body-level computed binds in source/extractor rules (`scan`, `match`, `ast`,
  `sg`, `json`, `cmd`, and peers). Their row evaluator may compute expressions in
  the head, but this proposal does not add a second binding environment. The
  diagnostic must name the derived-rule alternative.
- Unknown pure functions, wrong arity, wildcard call arguments, and non-pure
  effect calls used as values.
- Text with `-`, `*`, `/`, or `%`; only `+` is polymorphic.
- Mixed int/text `+`; use interpolation for text or `int(...)` for numeric intent.
- A computed result in a `sym` head column.
- `x = y` as an attempted alias bind and `x = "literal"` as an attempted constant
  bind. Both remain comparisons; introduce constants in the head or a value
  function if a future explicit constant-bind feature is approved.
- SQL `||` and an implicit variadic `concat()`; neither is introduced in this arc.

### Alternatives considered for binding syntax

1. **Recommended — contextual `x = computed_expr` over existing `Cmp(Eq)`.** It
   matches the requested surface and current prototype with no AST churn. Its
   contextual rule must be documented and shared by typecheck/lower.
2. **`let x = expr`.** Syntactically explicit and could allow literal/variable
   binds, but adds a keyword/token/AST path and a second statement family for a
   narrow need. It is the fallback if Chris rejects contextual `=`.
3. **Relational `bind(expr, x)` / `let(expr, x)` op.** Uniform atom shape but
   reverses normal reading order, looks effectful, and obscures the fact that the
   expression is pure and row-local.

Choose option 1, with left-hand introduction only.

## Four-layer planning protocol

### 1. Type signatures

```rust
// AST stays structurally unchanged.
struct Constraint { lhs: Term, op: CmpOp, rhs: Term }
enum BodyItem { /* ... */ Cmp(Constraint), /* ... */ }

enum CmpClass<'a> {
    Bind { target: &'a str, expr: &'a Term },
    Filter,
}

fn classify_cmp<'a>(cmp: &'a Constraint,
                    atom_vars: &HashSet<String>,
                    prior_binds: &HashSet<String>) -> CmpClass<'a>;
fn term_vars(term: &Term, out: &mut Vec<String>);
fn term_ty(term: &Term, tys: &HashMap<String, Type>) -> Result<Option<Type>>;
fn term_sql(term: &Term, canon: &HashMap<String, String>,
            tys: &HashMap<String, Type>) -> Result<String>;
fn term_sql_text(term: &Term, canon: &HashMap<String, String>,
                 tys: &HashMap<String, Type>) -> Result<String>;
fn body_sql_ex(body: &[BodyItem], rels: &Rels,
               overrides: &HashMap<usize, String>)
    -> Result<(HashMap<String, String>, HashMap<String, Type>,
               Vec<String>, Vec<String>)>;
```

The classifier should be the conceptual single source of truth. If crate
boundaries prevent literal reuse by typecheck and lower, pin identical table tests
against both call sites.

### 2. Pseudo-code

```text
parse every `expr op expr` body item as BodyItem::Cmp

atom_vars = every non-wild variable in every positive atom
canon, tys = SQL cell and declared type for those variables
prior_binds = empty

for body item in source order:
  if item is Cmp:
    class = classify(item, atom_vars, prior_binds)
    if Bind(target, expr):
      require every expr variable in atom_vars or prior_binds
      expr_type = infer expression type
      expr_sql = lower expression
      canon[target] = expr_sql
      tys[target] = expr_type
      prior_binds += target
    else:
      lower as comparison filter using sym-aware equality/text rules

for each negated atom:
  correlate variables through final canon, including computed binds

lower head:
  text destination + sym source -> decode
  sym destination + sym var -> integer pass-through
  sym destination + string literal -> compile-time StringId
  sym destination + computed text -> error
```

For `Term::Arith(Add)`:

```text
infer lhs/rhs
Int + Int          -> Int, SQL `+`
TextBase + TextBase -> Text, decode sym operands, SQL `||`
Int + TextBase     -> plus-mismatch error
unknown side       -> adopt known side; both unknown keep legacy Int
```

### 3. Instance lifetimes

- Parsed `Term`, `Constraint`, and `BodyItem` values live with one loaded
  `Program`, including across daemon ticks/hot reload until that program is
  replaced.
- `atom_vars`, `prior_binds`, typecheck `seen`, lowering `canon`, and lowering
  `tys` are per-rule scratch state and are dropped after checking/lowering.
- Computed values have SQL row lifetime only. They are expressions, not stored
  cells or engine-side mutable variables.
- `_strings` and interned `sym` rows keep their existing database lifetime. A
  computed text result does not extend or mutate that intern table.
- Pure function implementations/UDF registrations remain process/connection
  scoped as today; this proposal adds no per-bind cache.

### 4. Storage, reads, and writes

Parsing/typechecking reads program text and relation metadata only. Lowering reads
column types and, at query execution, string functions/concat may point-read
`_strings` to decode a consumed `sym`. Equality and literal filters retain hashed
integer paths when possible.

The design creates no schema, table, relation, CTE, or durable bind storage.
Computed binds inline into generated SQL and write only whatever derived relation
the enclosing rule already writes. Concatenation changes emitted SQL from numeric
`+` to `||` for typed text operands; it does not persist intermediate strings.
Diagnostics (`unbound-bind`, `plus-mismatch`, sym-destination errors) are the only
new checker outputs.

## Migration and compatibility

- Existing numeric arithmetic keeps its grammar, precedence, AST, and SQL.
- Existing `x = y`, literal equality, and bound-variable function comparisons
  remain filters.
- The base prototype's reverse computed bind is the only proposed tightening.
  Search the corpus before implementation; if any user-facing example relies on
  it, warn for one release or retain symmetry explicitly rather than silently
  changing results.
- Current docs already advertise computed binds and text `+`; implementation
  should update them to the signed-off left-hand-only, sym-aware contract rather
  than introduce another spelling.
- Source-rule users continue to compute in the head or split extraction and
  transformation into two rules. No implicit evaluator parity is promised.
- `concat()` can be added later as a variadic pure function without changing `+`;
  it is not required for this ergonomic fix.

## Verification

Parser/AST tests:

- `x = replace(y, ".", "::")` and `x = a + b` parse as `Cmp(Eq)` with a bare
  left `Var` and computed right `Term`;
- `x = y` stays Var/Var equality; precedence pins `a + b * c` and parentheses;
- a leading `replace(...)` body item retains relation-atom parsing unless it is
  on a comparison side; and
- `||` is rejected.

Typecheck/lowering tests:

- fresh left variable binds; bound left variable filters; reverse spelling is
  filter/error according to the signed-off migration choice;
- atom variables are order-free, bind chains are order-sensitive, and an unbound
  RHS names the fix;
- a bind used in a later negation correlates to the outer expression;
- int/int `+` emits `+`, text/text emits `||`, path/text emits `||` and yields
  text, mixed int/text reports `plus-mismatch`;
- `sym + text`, `replace(sym, ...)`, and interpolation decode through `_strings`;
- sym/sym joins and sym/literal filters remain integer comparisons;
- computed text compared to sym hashes the text expression; computed text into a
  sym head errors; and
- source-rule body bind refusal names head computation / two-rule derivation.

End-to-end fixtures should cover a chained normalization, concatenated URL, and a
sym-backed callee transformation. Pin rows and representative SQL, not just parse
success. Run focused parser/typecheck/lower tests, then at most two full integration
suites under the queue's hermetic-run rule.

## Staffing

- Implementation: one high-reasoning agent in a dedicated worktree. Although the
  diff should be small, parser/typecheck/lower/source-evaluator parity and `sym`
  performance make this a single-owner semantic change.
- Base: `07030b174f68372289e793b23d73de1f32d83ed1` or a rebased descendant after
  sign-off. Reconcile against the already-shipped prototype; do not reimplement
  it blindly.
- Current arc: proposal only; no source changes and no runtime-suite budget used.
- Implementation suite budget: focused tests freely; full integration suite at
  most twice.

## Sign-off needed

- [ ] Chris approves contextual `x = computed_expr` over the existing `Cmp(Eq)`
      AST rather than a new `let` keyword or `BodyItem::Bind`.
- [ ] Chris chooses left-hand-only introduction (recommended) versus retaining
      the base prototype's symmetric `computed_expr = x` bind.
- [ ] Chris confirms that `x = y` and `x = "literal"` remain comparisons, not
      alias/constant binds.
- [ ] Chris approves overloaded `+` for text/text and int/int, with mixed
      int/text a hard error; `concat()` and `||` are deferred.
- [ ] Chris approves atom-order-free inputs but source-ordered bind chains.
- [ ] Chris approves derived-rule-only body binds; source rules continue to
      compute in heads or a second derived rule.
- [ ] Chris approves the `sym` contract: decode on text consumption, hash for
      equality, and reject computed text into a `sym` head column.

