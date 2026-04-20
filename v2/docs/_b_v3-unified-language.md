# v3 Unified Language: A Teaching Document

A standalone read for learning sprf v3's language design. The reader is
assumed to know Rust basics, have seen a parser generator, and have
written at least a small DSL. Familiarity with lambda calculus in its
three-production form is helpful but optional.

This document reflects the design as locked during the 2026-04-19 and
2026-04-20 sessions. Where a decision has not yet been made the text
says so explicitly.

---

## Reading order

Read Parts 1, 2, and 3 in sequence. Parts 4 through 7 can be read in
any order once the first three are absorbed. Appendices A, B, and C
are references; visit them when the chapters point there.

```
Part 1  The shape of the language
Part 2  Grammar
Part 3  Execution model
Part 4  Cross-rule joins
Part 5  Sub-grammars and injection
Part 6  Tooling lanes
Part 7  Future hooks
Appendix A  Complete grammar reference
Appendix B  Example programs
Appendix C  Glossary
```

---

# Part 1. The shape of the language

## Chapter 1. What sprf v3 is

sprf v3 is a query language for code. A program written in it is a
`.sprf` file. Running the program extracts rows of data from one or
more git repositories, stores those rows in SQLite tables, and
optionally runs SQL-driven assertions over those tables.

The language has four core behaviors:

1. Pattern-match source code using sub-grammars (ast-grep for rust,
   typescript, python; json; markdown; regex; shell).
2. Compose patterns into pipelines that produce streams of cursors.
3. Persist each pipeline's output as a SQLite table.
4. Express cross-pipeline joins and assertions in either inline form
   or explicit SQL.

The language is expression-based with no statements. Every line of a
`.sprf` file is a value-producing expression. Binding a name to an
expression creates a named rule that other expressions can reference.

## Chapter 2. The three-tier model

Information in a running sprf program lives in one of three tiers.

**Stream tier.** Cursors flow through operators. A cursor carries a
file coordinate (fs/repo/rev), a byte range, captured bindings
gathered so far, and slots for op-specific typed payload. Ops
transform cursor streams. This tier is the runtime evaluation model.

**Name tier.** Identifiers bind to values in a single environment.
Names resolve to rules, ops, captures, or scalars. The environment
is lexically scoped with lookup falling back through parent scopes.

**Value tier.** Scalar literals at the source level. Strings, atoms,
numbers, booleans, and null. These appear in op argument positions
and as capture values once a pipeline produces them.

These three tiers compose: values flow as fields inside cursors,
names reference streams of cursors, and operations transform those
streams into new streams. The language design goal is to have one
uniform rule for each tier so an author only has to learn three
kinds of thing.

## Chapter 3. Cursor as the unit of flow

A cursor is a struct carrying the following fields:

```
fs         path to a file
repo       repository identifier
rev        revision (branch, tag, commit, worktree marker)
content    bytes of the file (shared via Arc)
byte_range range within content that this cursor "sees"
slots      typed payload channel (SlotKey<T> -> T)
captures   name -> value bindings accumulated so far
parent     link to the cursor this one was narrowed from
```

Every op receives a stream of cursors, performs some work, and emits
a stream of cursors. Stream in, stream out. The cursor structure is
closed; ops communicate state through `slots` and `captures`, never
by inventing new flow types.

## Chapter 4. Values, names, and ops

Three categories of named thing exist in the environment:

**Ops** are functions over cursor streams. Built in (`ast`, `json`,
`re`, `sh`, `fs`, `repo`) or user-defined via abstraction. Each op
has a fixed call signature: zero or more slots in square brackets,
parentheses, or braces, each slot potentially parsed by a distinct
sub-grammar.

**Rules** are named pipelines. A rule is a composition of ops with
optional capture decls. Binding a name to an expression creates a
rule. The rule's run produces a SQLite table; the rule's captures
become the columns of that table.

**Captures** are typed projections from a cursor stream. When an op
inside a rule declares a capture name at a pattern hole, each
emitted cursor carries a binding for that name. Captures are
UPPERCASE by convention.

Values of all three kinds live in one environment. Lookup dispatches
on the resolved entity kind, not on syntactic prefix.

---

# Part 2. Grammar

## Chapter 5. Casing as syntax

The first character of an identifier decides its syntactic category.
This rule holds across every position in the grammar.

| first character   | category                     |
|-------------------|------------------------------|
| UPPERCASE letter  | term (capture decl or ref)   |
| lowercase letter  | op or rule                   |
| punctuation       | sigil op (`$`, `&`, ...)     |
| digit             | number literal               |

Examples that lean on this rule:

```
CLASSES    UPPERCASE, term
my_rule    lowercase, rule or op
$          single-char, sigil op
42         digit, number literal
```

Inside the `${...}` carveout (Chapter 7), the first token follows the
same rule. This gives a single dispatch mechanism without a
position-specific table.

## Chapter 6. Rules, references, application

The grammar has one core production: op application. Rules and
captures fall out of specific op uses. No dedicated binding syntax
exists because `rule` is an op.

**Rule declaration** is an application of the `rule` op. Paren
takes an atom name; brace takes a chain body:

```
rule(name) {
  > op > op > op
}
```

Top-level `rule(...)` calls register absolute entries in the global
path namespace. Nested `rule(...)` inside a fork arm or another
rule body registers a relative entry under that parent's path.
The same op, the same syntax, position in the program decides
scope.

**Reference** reads a name. The form depends on what the name
references:

```
my_rule            bare name in expression position (function value)
$NAME              capture ref in walker body
${rule_name.$VAR}  cross-rule ref inside a carveout
&.$X               cursor rebase on a captured path
&.fs               cursor rebase on a field
```

Resolution happens at lower time using a single env built from
every rule-op application in the program plus built-in op names.

**Application** invokes an op with arguments:

```
op_name args
```

where `args` is zero or more slots: `[...]`, `(...)`, `{...}`.
Each slot's body is parsed by a grammar that the op declares at
registration time.

**Capture declaration** happens inside a walker body. A capture is
any `$NAME` token the walker encounters at a pattern hole. No
separate "declare" form exists; the walker's lower-time pass
scans its body for `$NAME` occurrences and registers them.

## Chapter 7. Carveouts: `${...}` and `&{...}`

A carveout is a syntactic device for embedding host expressions
inside sub-grammar bodies. Two carveouts exist:

```
${ expr }      host expression carveout
&{ addr }      address expression carveout
```

Inside a sub-grammar body (the body of an `ast[rust]{...}` block for
example), the sub-grammar parses its own language. Carveouts are
recognized before sub-grammar parsing by scanning for balanced
braces after `$` or `&`. The bytes inside the braces become a host
range; the bytes outside go to the sub-grammar.

The host range's content is parsed by the host expression grammar
for `${...}` and by the address grammar for `&{...}`. Nesting works
to arbitrary depth because the scan is iterative.

## Chapter 8. Field access and projection

A rule value carries a set of capture fields from its binding ops.
Access a capture using dotted notation:

```
my_rule.N          reads the N field from my_rule's cursor stream
```

Field access requires the field name to be UPPERCASE by the casing
rule in Chapter 5. Lowercase after a dot is a path continuation
rather than a capture access.

Rename a projected field using `>`:

```
my_rule.N > M      projects N, rebinds locally as M
```

Chains of field access and rename compose left-to-right:

```
my_rule.N > LOCAL > count()
```

## Chapter 9. Literals and the value tier

Five scalar literal kinds exist at the source level.

| kind    | syntax                                       |
|---------|----------------------------------------------|
| string  | `"foo"`, `'foo'`, `"""triple-quoted"""`      |
| raw     | `r"raw"`, `r#"raw with "quotes""#`           |
| number  | `42`, `3.14`, `0xff`                         |
| boolean | `true`, `false`                              |
| atom    | `:foo`                                       |
| null    | `none`                                       |

These cover the set of value types that appear in op argument
positions. Regex and glob are op calls taking string arguments,
spelled `re("...")` and `glob("...")`. Structured literals (arrays,
objects) come from sub-grammars such as json parsed by the `json`
op.

## Chapter 10. Quoting strategies

Strings have five forms, each solving a specific failure mode.

**Single-quoted** (`'...'`) strings accept no escapes. Every
character between the quotes appears literally. Useful for regexes
and shell fragments where backslash has its own meaning.

**Double-quoted** (`"..."`) strings support standard escapes: `\n`,
`\t`, `\"`, `\\`, `\u{1F4A9}`. Useful for general strings where
newlines or embedded quotes need representation.

**Triple-quoted** (`"""..."""`) strings span multiple lines and
accept embedded double quotes without escaping. Useful for long
prose or SQL bodies.

**Raw** (`r"..."`) strings accept no escapes and preserve every
character literally, similar to single-quoted. The leading `r` acts
as a marker for readers that the content is literal.

**Hashed raw** (`r#"..."#`, `r##"..."##`) strings accept embedded
double quotes by escalating the hash count. The close marker must
match the open marker's hash count. Useful for regex patterns that
contain quotes.

## Chapter 11. Atoms

An atom is an unquoted symbol that acts as a literal value. The
leading colon form `:name` is the explicit atom constructor.

```
:rust              the atom "rust"
:error             the atom "error"
```

Atoms appear in two positions:

**Explicit atom literals** via the `:name` form in any expression
position. Useful when a name would otherwise be interpreted as a
reference to a binding.

**Implicit atoms** in op slot positions where the slot grammar
expects an atom. `ast[rust]` passes the atom `rust` to `ast` without
the leading colon, because the bracket grammar for `ast` declares
its argument to be an atom.

Atoms interned at load time are cheap to compare.

---

# Part 3. Execution model

## Chapter 12. Phase ordering: parse, lower, run

Every `.sprf` program executes in three phases.

**Parse.** The source text becomes a syntax tree. tree-sitter-sprefa
produces the outer tree. Each op's slot bodies are parsed by the
sub-grammar the op declares. Carveouts inside sub-grammar bodies
are identified by balanced-brace scan and recursively re-entered.

**Lower.** The parsed tree walks into an environment-building phase.
Each abstraction registers its name. Each reference resolves to an
entity. Each application checks arg count and type. Diagnostics for
missing names, duplicate names, or type mismatches emit here.

**Run.** The resolved plan executes. Cursors flow through ops. Each
rule's cursor stream persists to its SQLite table. Check ops
evaluate their SQL and persist violation rows. Mutation effects
queue to the mutation handler for deferred apply.

The three phases match the three productions in Chapter 6. Parse
recognizes them syntactically. Lower resolves references.
Run applies functions.

## Chapter 13. The unified environment

The environment is a tree of scopes. The root scope contains every
built-in op and every top-level rule binding. Child scopes open at
each rule body's internal bindings. Lookup walks from the current
scope up the parent chain.

Entity kinds the environment stores:

```
EntityRef {
    Op(Arc<dyn Operator>),
    Rule(BindingId),
    Capture(SlotKey),
    Scalar(Value),
}
```

A name's resolved kind determines how the runtime treats its use
site. An `Op` resolves as a function to apply. A `Rule` resolves as
a stream to subscribe to. A `Capture` resolves as a field to read
from an enclosing cursor. A `Scalar` resolves as a value to pass
through.

## Chapter 14. The resolver

The resolver walks the parsed tree post-parse. Responsibilities:

1. Build scopes from abstractions.
2. Replace each name reference with an `EntityRef` pointing at the
   binding site.
3. Check application arg counts and types against op signatures.
4. Emit diagnostics for unresolvable names, ambiguous references,
   or type mismatches.

Resolver output is the input to the run phase. Downstream code sees
resolved trees, free of name strings.

The resolver runs on tolerant-parse output for LSP. Unresolvable
names surface as diagnostics; the tree otherwise remains usable for
hover, completion, and goto-definition queries.

## Chapter 15. Cursor narrowing at carveouts

When a carveout appears at tree position P inside a sub-grammar
body, the inner expression receives a cursor narrowed to P. Field
change table:

| field       | behavior                                     |
|-------------|----------------------------------------------|
| fs/repo/rev | inherited unchanged                          |
| content     | inherited unchanged                          |
| byte_range  | narrowed to P's node range                   |
| slots       | inherited with walker path appended          |
| captures    | inherited (visible and readable)             |
| parent      | set to the outer cursor                      |

The inner expression evaluates against the narrowed cursor. Any
further carveout inside the inner expression narrows again.

This rule makes cursor flow compositional. An expression at any
depth operates on the cursor shaped by its enclosing context, with
the byte_range always pointing to the tree position where the
carveout appears.

## Chapter 16. Lookup rules and scope

Name lookup walks the scope chain from innermost to outermost. The
first binding with a matching name wins. Shadowing is allowed; an
inner rule named `re` shadows the built-in regex op within its
scope.

Lexical scope means the scope a reference sees is the scope the
reference was written into, regardless of the dynamic call chain.
Rule bodies close over their lexical environment.

Cross-rule field access uses dotted notation:

```
my_rule.CAPTURE_NAME
```

The left of `.` resolves to a rule or op value. The right of `.` is
a capture projection. Ambiguous names (same name as both a capture
and a rule) emit a diagnostic at lower time.

---

# Part 4. Cross-rule joins

## Chapter 17. Inline xref semantics

A cross-rule join appears inline inside a pattern hole. The
syntactic form:

```
${ rule_name.$CAPTURE > $LOCAL_NAME }
```

Semantically this is:

1. Resolve `rule_name` to a rule value via the env.
2. Look up `CAPTURE` as a field of that rule's cursor stream.
3. For each row in the rule's SQLite table, use that capture value
   as the required match value at this pattern hole.
4. When the hole matches, bind the matched value locally as
   `LOCAL_NAME` so downstream ops can reference it.

The effect is a semijoin: the current op's output filters to only
those matches whose hole value equals some value from the
referenced rule. Row counts at each stage appear in the per-rule
tables, making selectivity observable.

The `$` prefix on the segment name (`.$CAPTURE`) distinguishes a
capture projection from a path continuation. A bare `.name` would
continue the path namespace; `.$CAPTURE` enters the capture
lookup side of the rule's binding table. The rename target
`> $LOCAL_NAME` follows the same convention.

## Chapter 18. Tap-before-filter and evidence

Every pipeline step automatically taps its input stream into an
`_evidence` table before filtering. The convention:

```
pipeline stage 0   ->   table rule_name
pipeline stage N   ->   table rule_name_evidence_N
```

Evidence captures cursors that reached the step. The step's output
is the subset that survived whatever filter the step applied.

For a rule with an inline xref, evidence shows every match the
pattern found; the output shows only matches the xref accepted.
Difference between the two gives drift, orphan references, or
coverage holes.

Evidence tables are append-only. No downstream op writes to an
evidence table.

## Chapter 19. Capture tier and assertion tier

Two persistence tiers coexist in SQLite.

**Capture tier.** Every rule writes its output and evidence to
append-only tables. Authorship is one-directional: rules write
their own tables and evidence tables, downstream ops read those
tables but never modify them.

**Assertion tier.** Check ops read from capture-tier tables and
write violation rows to their own violation tables. A violation
table has the shape `violations_<check_name>`. Check ops cannot
modify capture-tier rows.

This separation means assertions fail loudly without destroying
extracted data. A failed check adds a violation row; the capture
tables keep their extracted content for further inspection.

## Chapter 20. check, assert, witness

Three assertion ops share the same machinery. Each takes a name
and a SQL body. Each writes violation rows to a per-check table.
The three forms differ in what counts as failure.

**check**(name) runs the SQL and writes whatever rows the SQL
returns to the violations table. The violations table contents are
the rule's result. Useful when the check produces structured
information about drift, missing joins, or mismatched keys.

**assert**(name) wraps the SQL expecting zero rows back. Any rows
returned are violations. Useful for hard invariants where returning
any row at all indicates failure.

**witness**(name) wraps the SQL expecting at least one row back.
An empty result set is a violation. Useful for coverage checks
where the absence of a row means something expected is missing.

All three persist their full input rows so debugging a failed check
reads the violation table directly. UDFs available inside the SQL
include `meet()`, `is_bottom()`, `spans_overlap()`, `same_rev()`.

---

# Part 5. Sub-grammars and injection

## Chapter 21. Per-op grammars

Each op registers up to three slot grammars:

```
bracket_grammar()     parses [...] body
paren_grammar()       parses (...) body
brace_mode()          parses {...} body
```

A grammar is a reference to a `&'static LanguageFn` from the
tree-sitter ecosystem. Ops reuse published grammars (tree-sitter-
rust, tree-sitter-json, tree-sitter-markdown) for their domain
languages.

Some ops provide sprf-specific grammars for specialized needs:

```
tree-sitter-sprefa-walker    walker DSL for structural matching
tree-sitter-sprefa-term      host expression grammar for ${...}
tree-sitter-sprefa-addr      address grammar for &{...}
```

Each grammar sits in its own crate, compiled independently.

## Chapter 22. The injection driver

The injection driver is the framework component that walks an
op-call tree and reparses each slot's byte range with the op's
declared grammar. One driver handles every op; op authors never
implement injection themselves.

Driver algorithm:

```
queue = [root host parse]
while queue has entries:
    site = pop queue
    parser.set_language(site.lang)
    parser.set_included_ranges([site.range])
    tree = parser.parse(source)
    for each injection match in tree:
        queue.push(child site with child's lang and range)
```

The site metadata includes language key, byte range, depth, and
parent pointer. The driver output is a collection of sub-trees
linked by their parent relationships.

## Chapter 23. Interpolation holes

When a carveout appears inside a sub-grammar body, the sub-grammar
would normally choke on the carveout's bytes because the carveout
uses host syntax. The driver handles this by subtracting carveout
ranges from the sub-grammar's included ranges:

```
body_range      whole body the sub-grammar should parse
carveout_ranges ranges inside body that belong to the host
sub_ranges      body_range minus carveout_ranges, as a multi-range
```

The sub-grammar parses `sub_ranges` and sees the body as if the
carveouts were absent. Positions inside the sub-grammar tree remain
correct with respect to the original source because tree-sitter
preserves byte offsets across range splits.

Carveout ranges come from the host parser's pre-pass scan. A scan
for balanced `${...}` and `&{...}` is a simple state machine that
runs at lex time.

## Chapter 24. Shell host and double-brace escape

The `sh` op's body can contain both sprf carveouts and bash's own
`${...}` syntax. The escape mechanism:

```
$var        bash variable, passed to shell unchanged
${expr}     sprf carveout, host resolves
${{var}}    literal ${var} passed to shell, sprf treats as escape
```

Doubled braces peel one layer of sprf claim and deliver the
remaining bytes to the shell. Triple braces pass `${{...}}` through.
The rule only applies to `$`-led sequences; other double braces
remain untouched.

For other sub-grammars (json, markdown, rust), the `${...}` form
does not appear in their native syntax, so no escape is required.

---

# Part 6. Tooling lanes

## Chapter 25. Parsing impact

The host grammar contains these productions:

```
program      := stmt*
stmt         := binding | expr
binding      := "(" IDENT ")" "="? expr
expr         := name_ref
              | application
              | field_access
              | carveout
              | literal
name_ref     := IDENT
application  := IDENT slots
field_access := expr "." UPPER
carveout     := "${" expr "}" | "&{" addr "}"
slots        := ("[" body "]")? ("(" body ")")? ("{" body "}")?
literal      := STRING | NUMBER | ATOM | "true" | "false" | "none"
```

The parser distinguishes `UPPER_IDENT` from `lower_ident` at the
lexer level. Productions that require one or the other use the
appropriate token.

Parsing is context-free. No symbol-table lookup happens during
parse. Name resolution runs post-parse in the resolver phase.

## Chapter 26. LSP impact

Four LSP touchpoints handle every name-oriented operation.

**Completion at a name position** queries the environment for
in-scope names, filters by case context (UPPER cursor position
filters to captures and terms; lower cursor position filters to ops
and rules), and returns one ranked list.

**Hover on a name** resolves the name, dispatches on entity kind,
and renders the appropriate markdown. Ops show their docstring and
slot signatures. Rules show their capture set and source chain.
Captures show declared-at source and inferred type.

**Goto-definition** finds the abstraction that bound this name.
Walks the environment scope chain, locates the binding node, and
returns its source range.

**Find-references** scans the tree for nodes whose resolved
`EntityRef` matches the target. Node kinds to scan include
`name_ref`, `application`, and `field_access`.

Diagnostic rendering is uniform. Unresolvable names emit one
diagnostic kind. Shadowed names emit one warning kind. Type
mismatches emit per-op diagnostics owned by the op.

## Chapter 27. Op authoring

An op author implements the `Operator` trait. The trait slots:

```
fn name(&self) -> &'static str;
fn bracket_grammar(&self) -> Option<&'static LanguageFn>;
fn paren_grammar(&self) -> Option<&'static LanguageFn>;
fn brace_grammar(&self) -> Option<&'static LanguageFn>;
fn brace_mode(&self) -> BraceMode;

fn parse(&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
    -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>;

fn pipe(&self, ctx: OpCtx, stream: BoxStream<Arc<[Cursor]>>)
    -> BoxStream<Arc<[Cursor]>>;

fn hover_op(&self) -> Option<Markdown>;
fn completions_in_args(&self, cursor: Pos) -> Vec<Completion>;
fn schema(&self) -> Option<RowSchema>;
```

Minimum viable op fills four slots: `name`, `parse`, `pipe`, and
one `CaptureKind` impl for the emitted cursors. Remaining slots
default to no-op, returning `None` or an empty collection.

Higher-order ops take op-value arguments:

```
fn paren_grammar(&self) -> Option<&'static LanguageFn> {
    Some(&tree_sitter_sprefa_term::LANGUAGE)
}
```

The paren grammar parses a host expression, which at lower time
resolves to an `EntityRef`. The op receives the resolved entity
and uses it during `pipe`.

## Chapter 28. Core authoring

The framework components that handle the Lispy unified model:

**Env** structure holding `HashMap<Name, EntityRef>` with a
parent-pointer chain. Scopes open at each rule body and inner
abstraction.

**Resolver** pass that walks a parsed tree, builds the Env,
replaces each name reference with an EntityRef, and emits
diagnostics for unresolvable or ambiguous names.

**Abstraction handler** in the parser that recognizes `(NAME) expr`
and the `=` variant. Dispatches by casing of NAME to either a
capture decl (UPPER) or a rule decl (lower).

**Casing dispatch** at the lexer level, tagging every ident as
`UPPER_IDENT` or `lower_ident`. Parser uses the appropriate token
in productions that care.

**Carveout scanner** as a pre-pass over sub-grammar body bytes.
Produces a list of carveout ranges. The injection driver subtracts
these from the sub-grammar's included ranges.

Removed from the framework compared to earlier drafts:

- Separate rule-name lookup path.
- The `rule()` op as a distinct registrar.
- Central enum switches over op kind for rule-ref vs op-ref
  disambiguation.
- Per-position name resolver variants.

---

# Part 7. Future hooks

## Chapter 29. Higher-order ops

Ops take op-values as arguments. Immediate applications:

```
(retried) retry(sh { deploy.sh }, 3)
(timed)   time(ast[rust] { fn $N })
(batched) batch(sem[rust] { class::$C }, 100)
```

The wrapper op receives an `EntityRef::Op` from its argument grammar
and calls it inside its own pipe implementation. No new grammar
concept, no new runtime concept. Lands whenever an author writes
the first wrapper op.

## Chapter 30. Meet lattice for assertions

SQL UDFs `meet()` and `is_bottom()` give check ops a principled
merge rule. Meet of two values returns the greatest lower bound
satisfying both. Incompatible values return bottom.

Meet rules per type:

| meet              | rule                                         |
|-------------------|----------------------------------------------|
| string x string   | equal keeps; different bottoms               |
| atom x atom       | equal keeps; different bottoms               |
| number x number   | equal keeps; different bottoms               |
| bool x bool       | equal keeps; different bottoms               |
| span x span       | overlap intersects; disjoint bottoms         |
| null x anything   | anything                                     |
| cross-type        | bottom                                       |

A check like "same port across environments" becomes:

```sql
check(ports_agree) {
    SELECT meet(a.V, b.V) as r
    FROM A JOIN B ON a.rev = b.rev
    WHERE is_bottom(meet(a.V, b.V))
}
```

UDF implementation ships with the framework. The table above is
the complete spec.

## Chapter 31. sprf-paths and branching

The top-level namespace is a path table. Rule names are short names
for top-level entries. Fork arms inside pipelines become positional
entries. Both address uniformly:

```
my_rule                  top-level entry
my_rule.arm_2            fork arm inside my_rule
my_rule.arm_2.sub_3      nested fork arm
```

Future work pins the exact path syntax for fork arms. The namespace
is reserved and unified with rule names so the addition lands
without grammar churn.

## Chapter 32. The #lang tower

The first line of a `.sprf` file can select a grammar version:

```
#lang sprf.v3
```

tree-sitter-sprefa dispatches to the declared version. Older files
keep parsing under their original grammar. Newer files opt in to
features at their own pace.

The tower is Racket-style: a `#lang` marker at file head, a language
registry that maps markers to grammars, and independent grammar
evolution under separate crates. v3 ships with `sprf.v3` as the
default and no other dialects; future versions add entries.

---

# Appendix A. Complete grammar reference

## Tokens

```
UPPER_IDENT    [A-Z][A-Za-z0-9_]*
lower_ident    [a-z][a-z0-9_]*
NUMBER         digit+ ("." digit+)? | "0x" hex_digit+
STRING         "..."  '...'  """..."""  r"..."  r#"..."#
ATOM           ":" lower_ident
PUNCT_OP       "$" | "&" | "%" | "@"
```

## Productions

```
program         := stmt*

stmt            := binding | expr | ";"

binding         := "(" IDENT ")" ("=" expr | expr)

expr            := chain

chain           := expr ">" expr
                | expr "|" expr
                | application
                | name_ref
                | field_access
                | carveout
                | literal
                | "(" expr ")"

application     := op_name slots

op_name         := UPPER_IDENT | lower_ident | PUNCT_OP

slots           := slot+
slot            := "[" body "]"
                | "(" body ")"
                | "{" body "}"

body            := sub_grammar_body
                | host_expr_list

host_expr_list  := expr ("," expr)*

name_ref        := IDENT

field_access    := expr "." UPPER_IDENT

carveout        := "${" expr "}"
                | "&{" addr_expr "}"

addr_expr       := "." field_name
                | "." UPPER_IDENT
                | addr_expr "." addr_segment

literal         := STRING | NUMBER | ATOM | "true" | "false" | "none"

comment         := ":-" (anything up to EOL)
                | ":-" (anything up to) "-:"
```

## Reserved punctuation

```
${ }        host carveout open/close
&{ }        address carveout open/close
${{ }}      bash escape inside sh body
> TARGET    projection rename
.           field access, path segment
:           atom prefix
:-          line comment or scoped comment open
-:          scoped comment close
=           explicit bind operator
;           statement separator (optional)
```

---

# Appendix B. Example programs

## B.1 Classes and calls

```sprf
(classes) ast[rust] { class ${N} }

(calls)   ast[rust] { new ${classes.N > TARGET}() }

assert(no_orphan_calls) {
    SELECT path, TARGET as caller_target
    FROM   calls_evidence
    WHERE  TARGET NOT IN (SELECT N FROM classes)
}
```

Extracts every class declaration and every `new X()` call. The
`calls` rule filters calls to targets that appear as classes in
the same run. Evidence captures unfiltered calls. The assert
surfaces calls whose target has no matching class.

## B.2 Schema drift across repos

```sprf
(truth) repo(:acme_lib)   > fs("schema.sql") > sha256(${H})

(svc_a) repo(:acme_svc_a) > fs("schema.sql") > sha256(${truth.H > OK_A})

(svc_b) repo(:acme_svc_b) > fs("schema.sql") > sha256(${truth.H > OK_B})

assert(schemas_synced) {
    SELECT 'svc-a' as where_, H as actual FROM svc_a_evidence
    WHERE  H NOT IN (SELECT H FROM truth)
    UNION ALL
    SELECT 'svc-b' as where_, H as actual FROM svc_b_evidence
    WHERE  H NOT IN (SELECT H FROM truth)
}
```

Hashes the schema file in three repos. The check surfaces any repo
whose copy diverges from the source-of-truth repo.

## B.3 Cross-revision API drift

```sprf
(api_main) rev(main)    > ast[ts] { export function ${N} }

(api_feat) rev(feature) > ast[ts] { export function ${api_main.N > MATCH} }

witness(all_main_apis_present_on_feature) {
    SELECT N FROM api_main
    WHERE  N NOT IN (SELECT MATCH FROM api_feat)
}
```

Identifies exported functions present on main but absent on a
feature branch. The witness fires when any main-only function
appears.

## B.4 Imports audit with regex

```sprf
(imports) ast[rust] { use ${PATH} }

(suspicious) imports > filter(${re(r"^crate::internal")})

assert(no_internal_crosses) {
    SELECT path, PATH FROM suspicious
}
```

Pattern-matches import statements, filters to those starting with
`crate::internal`, and flags any crossing rule as a violation.

## B.5 Higher-order op

```sprf
(with_retry) retry(sh { deploy.sh }, 3)

(timed_scan) time(ast[rust] { fn ${N} })
```

Wrapper ops receive op-values as arguments. `retry` invokes the sh
op up to 3 times on failure. `time` measures wall time of the
wrapped ast scan.

---

# Appendix C. Glossary

**Abstraction.** A binding form. `(NAME) expr` binds NAME to expr
in the current scope.

**Application.** Invoking a function value with arguments. Spelled
`op_name slots` in source.

**Atom.** A lightweight symbol value. Spelled `:name`. Interned.

**Carveout.** A syntactic device for embedding host expressions in
sub-grammar bodies. Spelled `${expr}` for values, `&{addr}` for
addresses.

**Capture.** A typed projection from a cursor stream, binding a
value at each emitted cursor. UPPERCASE by convention.

**Casing dispatch.** The rule that first character of an ident
determines its syntactic category.

**Check op.** Generic name for `check`, `assert`, `witness`
operators that run SQL and produce violation rows.

**Cursor.** The unit of flow. A struct carrying file coord, byte
range, content, slots, captures, and parent link.

**Entity.** The resolved form of a name reference. Kinds: Op, Rule,
Capture, Scalar.

**Env.** The name-to-entity map built at lower time. Scoped by
abstraction nesting.

**Evidence.** A tap-before-filter persisted stream. Each pipeline
stage writes an evidence table holding the input cursors.

**Field access.** Projection of a capture from a rule value.
Spelled `rule.CAPTURE`.

**Higher-order op.** An op whose signature accepts another op as
an argument.

**Host grammar.** tree-sitter-sprefa. Parses the outer `.sprf`
file.

**Injection driver.** The framework component that reparses op slot
bodies with sub-grammars. Walks an op tree, calls
`set_included_ranges`, queues nested injections.

**Lower.** The phase between parse and run. Builds the Env,
resolves references, checks types.

**Meet.** Greatest lower bound on the value lattice. The merge rule
for unifying two values.

**Op.** A function over cursor streams. Built in or user-defined
via abstraction.

**Reference.** A use of a name. Resolves to an Entity at lower
time.

**Rule.** A named pipeline. Produces a SQLite table as its
materialized output.

**Scope.** An env subregion. Opens at abstraction nesting and
closes at scope end.

**Semijoin.** A relational operation that filters the left relation
to rows matching the right. Inline xref is a semijoin.

**Slot.** An argument position on an op. Bracket `[...]`, paren
`(...)`, or brace `{...}`. Parsed by the op's declared grammar.

**Sub-grammar.** The tree-sitter Language used to parse an op's
slot body. Distinct from the host grammar.

**Stream tier.** The cursor-flow runtime evaluation.

**Tap.** An observation of a stream that persists its contents
without affecting flow.

**Term.** An UPPERCASE identifier. Represents a capture decl or
ref.

**UDF.** A SQLite user-defined function registered by the
framework for check op SQL bodies.

**Violation table.** A per-check persistence target. Holds rows
that a check's SQL returned.

---

# End of document
