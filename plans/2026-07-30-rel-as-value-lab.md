# rel-as-value lab

Lane `lane/rel-as-value-lab`, base `609066ee0f5f9b5e64837371680092134c11c20f`.
Design record only. No production file changed, no syntax proposed for landing.

---

## 0. The short version

Two different features got the same name.

**What landed** (commits `4b0bc279` + `472320f4`, merged `2e2b983b`) lets you
write a *row* of one rel directly inside a column of another:

```
file(repo(Name), fpath(Path)) <- raw(Name, Path, _, _).
```

That is first-order. `repo(Name)` is a value. Nothing is being passed a
relation.

**What was asked for** is that the *relation itself* becomes the argument, so
one rule body works over whichever relation it is handed:

> "i meant rels as values as i can pass a literal functor into the arg slot,
> prolog cant do this with functors without higher order help"

The user is right that these are different, and right about the Prolog part.
Section 3 shows the requested spelling failing on both doors today, silently.

The good news, and the thing nobody said out loud: **the requested feature is
already ruled and already prototyped.** Two locks and two labs in this tree
answer it. Section 5 cites them. The answer is compile-time specialization
(the Souffle/Rust-generics shape), not a runtime relation value, and the
project committed to that before this lane existed.

---

## 1. What the landed feature buys

The ruling on this lane changed mid-flight: the feature stays. The user's own
words are "if its just destructuring in head positions that sounds like we
would eventually want it". So this section is not a keep-or-delete referendum.
It is: what does it buy, where are its edges, does it compose with the thing
actually wanted.

### 1.1 The worked example

Three rels, each naming the previous one in a column:

```
rel repo(name: text).
rel fpath(name: text).
rel file(repo: repo, at: fpath).
rel span(file: file, start: int, end: int).
```

Build two levels in one rule head, then read two levels back out in one body
atom:

```
span(file(repo(Name), fpath(Path)), Start, End) <- raw(Name, Path, Start, End).
coord(Path, Start, End)                         <- span(file(_, fpath(Path)), Start, End).
```

Before the commits, that program was wrong on **both** doors, in opposite
directions, with no error message. Verbatim from the fixture header
(`v6/prolog/conformance/fixtures/6_relation_depth.pl:20-73`, red receipts taken
at `68d1ca3f`):

- Reference engine: stored a raw Prolog compound, so the graded tick log printed
  `"repo(acme)"` where the compiled side printed `{"name":"acme"}`, and every
  `decode/2` over a rule-built value failed outright.
- Compiled side: emitted `json_extract(b1."repo", '$.fn')` against the INTEGER
  endpoint the previous statement had just written. `json_extract` of an integer
  is NULL, so `span` and `coord` were **permanently empty, with no refusal**.

So the honest grade of what it buys: **it closed two silent wrong-answer bugs at
depth two and deeper, and one never-graded byte divergence at depth one.** That
is real and it is the good kind of work. Depth one already worked on the
compiled side; depth two and beyond did not work anywhere.

### 1.2 Is it sugar? On the read side, yes. On the write side, no.

This distinction matters for section 5, so it gets its own receipt.

**Reading is sugar.** The nested pattern and the pre-existing `decode/2` chain
answer identical rows. Both spellings in one program, run on the reference
engine at HEAD:

```
via_pattern(Path) <- span(file(_, fpath(Path)), _, _).

via_decode(Path)  <- span(FileValue, _, _),
                     decode(FileValue, {at: AtValue}),
                     decode(AtValue, {name: Path}).
```

```
pattern=[via_pattern('src/a.rs')] decode=[via_decode('src/a.rs')]
A1 nested-pattern vs decode-chain EXPRESSIBLE
```

The corpus already pins this independently: fixture
`relation_depth3_chained_decode` carries `dfound` (decode chain) and `nfound`
(nested pattern) side by side and expects the same row from both.

**Writing is not sugar.** There is no other spelling that puts a *new* value in
a ref column. A rule can *forward* a ref it already has, with no relation term
anywhere:

```
copy(FileValue, Start, End) <- span(FileValue, Start, End).
```

```
copy/3 = [copy(obj([at-obj([name-'src/a.rs']),repo-obj([name-acme])]),10,20)]
A2 forward a ref variable (no term) EXPRESSIBLE
```

That is confirmed at the SQL level by an older lab check that still passes,
`v6/prolog/labs/rel_value_unification/11_ref_necessity.pl`
`typed_variable_forwards_opaque_identity_without_target_rejoin`: the emitted
statement is `SELECT b0."choice" FROM "selected" b0` with no rejoin.

But forwarding only moves an existing value. To *mint* one, the head term is the
only door, because `__id` is internal and no surface spelling reaches it. So the
landed feature is the sole constructor for ref columns. That is a genuine
capability, not sugar.

### 1.3 Orthogonal to kwargs, no overlap

Kwargs partial application (omitted named body columns become fresh wildcards)
operates *within* one atom's argument list. Relation patterns operate *across*
rels, on the contents of a single column. Kwargs can hand you the ref column as
an opaque value; it has no way to look inside it. They compose cleanly and
neither expresses the other.

### 1.4 The edges, and they are sharp

Four findings, all reproduced at HEAD on this lane.

**Edge 1: the refusal is syntactic, so a variable slips through.** The shared
check `relation_pattern_not_a_relation_value` (`v6/prolog/0_program_check.pl`)
runs `relation_argument_violation`, whose first line is `nonvar(Value)`. A
*literal* in a ref column is refused. A *variable* bound to a text leaf is not
inspected at all:

```
span(Path, Start, End) <- raw(_, Path, Start, End).   % Path is text, column is `file`
```

Reference engine at HEAD:

```
oracle ACCEPTED
  span/3 = [span('src/a.rs',10,20)]
```

A bare text string sits in a column the model says holds an integer endpoint.
The three refusal fixtures all use literals, so nothing in the corpus covers
this.

The compiled door accepts it too, and is worse. It emits:

```sql
INSERT OR IGNORE INTO "span" ("file", "start", "end")
SELECT b0."path_name", b0."start", b0."end" FROM "raw" b0
```

writing the TEXT `path_name` straight into `file`, whose own emitted DDL is:

```sql
CREATE TABLE "span" ("file" INTEGER NOT NULL, "start" INTEGER NOT NULL,
                     "end" INTEGER NOT NULL, ...,
                     PRIMARY KEY ("file", "start", "end")) WITHOUT ROWID
```

So a text string lands in an `INTEGER NOT NULL` column that is part of the
primary key. SQLite's type affinity accepts this on a non-STRICT table, and
`NOT NULL` is satisfied, so nothing fails at any layer. Both doors accept a
type-violating program and neither says a word. This is the same silent-wrong
class the arc set out to close, one level up: the arc fixed the cases where the
*shape* is written out, and left the case where a variable carries the wrong
type into the same slot.

**Edge 2: depth one got slower, and the receipt style cannot see it.** The
rewrite lowers *every* level uniformly to one `__ref_<type>` dictionary atom.
At depth one that re-derives an identity the scanned row already had.

Before (asserted by lab 11, written when it was true):

```sql
SELECT b0."__id" FROM "user" b0
```

At HEAD, same program:

```sql
INSERT OR IGNORE INTO "selected" ("choice")
SELECT b1."__id" FROM "user" b0, "__ref_user" b1
WHERE b1."id" = b0."id" AND b1."name" = b0."name"
```

And `__ref_user` is literally a view over `user`:

```sql
CREATE TEMP VIEW "__ref_user" AS
SELECT t."__id", "id", "name", json_object(...) AS "__rendered" FROM "user" t
```

So the statement self-joins `user` against a view of `user`, matching on every
value column, to recover `__id`, which was already in scope as `b0."__id"`.
Since `user` declares `UNIQUE("id")` and the join binds all columns, `b1."__id"`
is provably `b0."__id"`. The delta statement is worse, three tables:

```sql
SELECT DISTINCT b0."__id"
FROM "__frontier_user" d0, "user" r0, "__ref_user" b0
WHERE d0."_phase" >= 0 AND r0."id" = d0."id" AND r0."name" = d0."name"
  AND b0."id" = d0."id" AND b0."name" = d0."name"
```

Two lab checks that passed when written are now red at HEAD:
`target_scan_captures_dense_identity_without_ref` and
`incremental_target_frontier_rejoins_dense_identity_without_json`. Those are the
fail-first receipts for this regression and they were already sitting in the
tree.

The commit's own EXPLAIN receipts assert "every hop is SEARCH, never SCAN". A
SEARCH against a redundant extra table is still a SEARCH, so the receipt style
is structurally blind to this. That is worth fixing independently of the join:
**count the tables, not just the access method.** This is the same lesson as the
standing count-test law, applied to joins.

**Edge 3: edge rules lost a capability.** `relation_value_in_edge_rule` is new
in `472320f4` (`git log -S` confirms it appears in no earlier commit). It refuses
this on the compiled side:

```
post(user(Id, Name)) <+ user(Id, Name).
```

Lab 9 (`9_reference_construction_contexts.pl`) has a check named
`direct_target_edge_trigger_projects_joined_identity` asserting that exact
program lowered to `b0."__id"` with `FROM "user" b0` and no `json_object`. That
check is now red with `unsupported_construct(relation_value_in_edge_rule(...))`.
The reference engine still runs the program. So the arc traded a working
compiled edge construction for a named refusal, and the doors now disagree on
what is expressible.

This is the most consequential edge, because edge rules (`<+`) are where the
reactive and state-machine idioms live. Section 5.4 argues it also sits directly
in the path of the feature the user actually wants.

**Edge 4: one thing the arc genuinely improved, worth recording.** Lab 9's
`key_only_constructor_is_not_a_current_relation_term` asserted that
`post(user(Id)) <- source(Id)` (target named at the wrong arity) silently
compiled to a `json_object` blob. At HEAD it is
`relation_pattern_not_a_relation_value(post/1, author, user, user(_))`. A silent
miscompile became a named refusal. That is a clean win and it supersedes the lab
check.

### 1.5 Does it compose with what the user wants, or block it?

**It composes, with one exception that must be fixed first.**

The landed feature and a rel-parameterized rule live on different planes. The
landed feature is about a *column's contents*: which row of `repo` does this
`file` point at. A rel-parameterized rule is about a *rule's text*: which rel
does this body read. Nothing about the first constrains the second, and section
5.3 shows the coordinate systems are genuinely different (row identity versus
rel identity).

More than merely not colliding, head-position construction is the *natural
output side* of a specialized rule. When a generic rule is monomorphized against
a concrete rel, its head must write concrete rows, and if the target has ref
columns the head needs exactly this constructor. Without it, a specialized rule
could read generically but could never build a nested result. So the landed
feature is a prerequisite for the interesting half of the requested one, not a
detour around it.

The exception is Edge 3. A generic rule specialized into an **edge** rule, whose
head constructs a relation value, is refused on the compiled door today. Enum
state machines are already known to be the flagship edge idiom, and a
parameterized version of one is a natural first customer. So
`relation_value_in_edge_rule` should be treated as a blocker for the
higher-order arc rather than an accepted limit, and it is the first card in
section 8.

---

## 2. What the two flagged labs already settled

The coordinator flagged `rel_value_unification` (12 files) and
`rel_definition_hash` (1 file). Both were read. Neither is superseded by this
lab; one of them answers the crux outright.

### 2.1 `rel_value_unification` settles ROW identity, and does not touch the crux

Twelve files, all about how a *row* of one rel is referenced from another. Its
conclusions, still standing at HEAD:

- A scanned target captures a dense `__id`; typed variables forward it with no
  rejoin (`11_ref_necessity.pl`, still PASS).
- `decode/2` is the destructuring path, lowering to `__ref_<target>` joined on
  `__id` (still PASS).
- Graph cycles use an ordinary two-column edge rel; recursive inline reference
  stays the `type_cycle` refusal (still PASS).
- `ref` has no registered surface semantics; the spelling is the target rel name
  in column position (still PASS, and matches `locked(ref_current_verdict)`).

**It never asks whether a relation can be named as a value.** Every identity in
it is a row identity. So it does not answer the crux, and this lab does not
supersede it. What this lab adds on its territory is the four edges in section
1.4, two of which are its own checks going red.

### 2.2 `rel_definition_hash` answers the crux directly

This is the one that matters, and it is the least-read file in the pair. Eleven
receipts, all green at HEAD on this lane:

```
PASS variable alpha-equivalence
PASS systematic relation rename preserves content hashes
PASS declared column rename changes shape hash
PASS source rule order remains in exact-code hash
PASS conjunction order remains in exact-code hash
PASS match hashes after expansion equal handwritten rules
PASS generated host names normalize; template bytes invalidate
PASS recursive SCC hash survives rename and sees edge/body change
PASS layout, program semantics, and stable storage identity separate
PASS 6 calls -> 3 code templates, 6 state/storage instances
PASS renamed programs share abstract lowered SQL, raw SQL stays bound
11 PASS
```

Three of those are the crux, and section 5.3 unpacks them.

---

## 3. The requested feature, graded today

Six candidate spellings, run on the reference engine at HEAD. Full source in the
lane's scratch probes; every line below is copied from a real run.

| # | candidate | grade | receipt |
|---|---|---|---|
| B1 | hand-monomorphized: same body copied once per rel | **expressible** | `sym_python/1 = [sym_python(foo)]`, `sym_rust/1 = [sym_rust(bar)]` |
| B2 | discriminant column: N adapter rules, ONE generic body | **expressible, ugly** | `sym_total/2 = [sym_total(python,1),sym_total(rust,1)]` |
| B6 | rel name as a join key over a hand-built union | **expressible, ugly** | `selected/2 = [selected(def_python,foo)]` |
| B3 | rel name in a column, meta-called as a goal | **inexpressible, silent** | `picked/1 = []` |
| B4 | rel-valued ref column, dereferenced to rows | **inexpressible, silent** | `analysis/2` builds fine; `reached/1 = []` |
| B5 | generic write, head rel from a variable | **inexpressible, silent** | head is inert; no rows written |

### 3.1 What works: monomorphize by hand

```
sym_python(Symbol) <- def_python(_, Symbol).
sym_rust(Symbol)   <- def_rust(_, Symbol).
```

Character-identical bodies, twice. This is exactly what a compile-time generic
would *generate*. It works, it is correct, and it is the baseline every
candidate is measured against. Its cost is that the duplication is yours to
maintain.

### 3.2 What works but hurts: a discriminant column

```
def_any(python, Path, Symbol) <- def_python(Path, Symbol).
def_any(rust,   Path, Symbol) <- def_rust(Path, Symbol).

sym_total(Language, count(Symbol)) <- def_any(Language, _, Symbol).   % ONE generic body
```

The generic body is real: one rule, works over every language. But the
duplication did not disappear, it **moved into the adapters**. You still write
one rule per rel. You also pay a full copy of every row into `def_any`.

B6 is the same trick with the rel's own name as the tag, which additionally lets
a `enabled(RelName)` row select which sources participate at runtime. That is
genuinely useful and is the most the current language reaches toward the ask.
The name is a **join key**, never a goal.

### 3.3 What does not work, and fails in the worst possible way

The requested spelling:

```
picked(Symbol) <- which(RelName), RelName(_, Symbol).
```

That does not parse. Not in this language, in **Prolog itself**: a variable
cannot be a functor. This is precisely the boundary the user named. Spelled with
an explicit meta-call so it at least parses:

```
picked(Symbol) <- which(RelName), call(RelName, _, Symbol).
```

Result on the reference engine: `picked/1 = []`. Result on the compiler:
`compiler ACCEPTED SILENTLY`. No refusal on either door.

The reason, confirmed by dumping the plan:

```
rel call/3 kind=set cols=[col1,col2,col3]
rel def_python/2 kind=set cols=[path_name,symbol]
rel picked/1 kind=set cols=[symbol]
rel which/1 kind=set cols=[rel_name]
```

`call` has no registry row, so refusal-by-absence should have caught it. Instead
the `edb_definition` ruling (an undeclared rel is a legal input rel) claimed it
first: `call/3` became a phantom empty input relation with three anonymous
columns. **The higher-order spelling is silently reinterpreted as a first-order
relation named `call`, and the program returns nothing.**

That is the worst failure shape this project recognizes. A user reaching for the
feature gets an empty rel and no explanation. This is not a new defect class,
it is the known unpriced cost of `edb_definition`, but the requested feature
walks straight into it.

B5, the generic write, is the same story on the head side and equally inert.

---

## 4. Prior art

Sourced from the four verified local study files
(`theory/datalog-study/20260725.{1.datafun,2.flix,3.souffle,4.comparative}`),
with primary docs fetched where the skills were silent. URLs are the fetched
primary sources.

| system | what it buys | compile-time cost | runtime cost |
|---|---|---|---|
| **Souffle components** (`.comp`, `.init`, `.comp A<T> : T`) [souffle-lang.github.io/components] | a ruleset written once, specialized per relation-bearing component supplied at `.init` | full monomorphization, one namespace and code copy per instantiation; component-type arguments must be simple identifiers, so nesting needs wrapper components | none, every relation reference is a concrete table before execution |
| **Souffle `.functor`** [souffle-lang.github.io/functors] | user C++ functions in rule bodies | none | none, but arguments are primitive-only (`symbol`/`number`/`float`); a relation cannot be a functor argument. Souffle has **no runtime relation-valued term at all** |
| **Datafun** (ICFP 2016, POPL 2020) | `{A}` is genuinely first class; a set can be passed, returned, matched. Monotonicity is a typechecker fact, so higher-order code coexists with a sound `fix` | bidirectional inference over the monotone / discrete (`□`) discipline | first-order rule shapes reduce to ordinary semi-naive (the derivative **is** the delta join). Genuinely higher-order bodies need the general `Derive` transform; no published claim it compiles to static ahead-of-time SQL |
| **Flix** `#{ ... }`, `<+>`, `solve` [doc.flix.dev/fixpoints.html] | the fullest form: a rule set is an ordinary value, buildable, passable, composable, with the relation chosen at `solve` time | row-polymorphic inference over predicate schemas plus stratifiability folded into typecheck | a real fixpoint solver runs at `solve` time; batch semantics, no reactive retraction |
| **Prolog `call/N` + `:- meta_predicate`** [swi-prolog.org/pldoc/man?section=metapred] | any predicate passed as data and invoked with extra arguments. This is the standard answer, and the reason a bare functor cannot do it: a functor is inert data until `call` turns it into a goal | none; `meta_predicate` is a compile-time module-qualification annotation only | one clause-database lookup per invocation, per row |
| **Racket** [docs.racket-lang.org/datalog] | nothing in `racket/datalog`: "function-free Horn clauses" is definitionally the restriction that excludes relation-valued arguments. Racklog's `%apply` exists but is Prolog, outside Datalog | n/a | n/a |
| **SQL** [Oracle PTF docs, MS TVP docs] | no relation-valued parameter, because output schema must be known at parse/bind. Workarounds: dynamic SQL (no static checking, re-parse per name); T-SQL table-valued parameters (rows by reference, but schema fixed at `CREATE TYPE`); SQL:2016 polymorphic table functions | PTF resolves the unknown table shape via a mandatory `DESCRIBE` callback **at query compile time** | PTF: ordinary execution once schema is locked. Every vendor solved this by moving resolution **earlier**, never later |
| **Rust generics vs `dyn`** | generics: specialized, inlinable code per concrete type, zero dispatch. `dyn`: one shared body, type erased | generics: one codegen pass per instantiation (binary growth). `dyn`: none, but the trait must be object-safe | generics: none. `dyn`: one vtable indirection per call, no cross-call inlining |

### 4.1 Which of these this architecture can carry

The binding constraint is door two. The compiler emits **fixed SQL statement
text per relation, ahead of time**, and that text is graded byte-identical
against the reference engine. There is no later phase where a table identity
gets resolved.

That is the SQL:2016 lesson restated as a hard law. Oracle's polymorphic table
functions get away with an unknown input table only by running `DESCRIBE` at
*compile* time to lock the schema before the statement exists. This project's
door two already **is** that phase, with nothing analogous happening per query
at runtime.

So:

- **Souffle components and Rust generics are the shape this can carry.**
  Compile-time monomorphization is what door two already does for every ordinary
  rule. For each concrete rel a program supplies, emit a separate specialized
  statement with real column types baked in. No runtime relation value is
  invented.
- **Prolog `call/N`, Rust `dyn`, and Flix's runtime-composed constraint values
  are structurally impossible for door two** as literally spelled. Each resolves
  "which relation" at runtime through an indirect lookup. No SQL text can encode
  "decide which table to join when this row arrives."
- **Datafun's general `Derive` and Flix's fully open composition are the hardest
  cases.** Both papers' own framing requires an interpreter, or a compiler that
  specializes anew at each composition point. Door two could house the
  closed-instantiation-set subset; the open case is the same wall as `call/N`.

The reference engine (door one) could carry almost any of them. It has `call/N`
underneath it already. **The doors would then disagree about what is
expressible, which is the failure this project grades against.** That is the
whole argument in one sentence, and it is the same shape as Edge 3 in section
1.4, where the doors already disagree today over relation values in edge rules.

---

## 5. The crux: does a content id let you name a REL, not just a row?

This is where the user's instinct is sharpest and where the answer is most
useful.

> "that was why we did content hash for rels and instance hash for rels, so they
> have a true coordinate thru the type system"

### 5.1 The short answer

**The coordinate system exists, it has three axes not two, and it is a
compile-time coordinate.** It is exactly the right thing to have built. It does
not, and cannot, give a relation a runtime name, and that is a property of what
a relation *is*, not a gap in the implementation.

### 5.2 Why a content hash cannot name a relation

A content id names a **row** by its contents. Two things break if you try to
scale that up:

1. **A relation's contents change every tick.** Hash the rows and you get a value
   that is different after every arrival. It cannot be a stable name for the
   thing, because the thing it is naming has not changed identity, only contents.
   A name that changes when the contents change is a version stamp.
2. **A relation is a schema plus a set, and the schema is not data.** Column
   names and types exist at compile time. There is no row anywhere whose contents
   are "the shape of `def_python`".

So the content hash is at the wrong level. Not slightly wrong: it names members,
and a relation is not a member of itself.

### 5.3 What actually exists: three layers, and they are already separated

`rel_definition_hash`'s `identity_layer_receipt` proves the separation directly.
It builds two programs with the same column layout but different rule bodies:

```
PASS layout, program semantics, and stable storage identity separate
```

Concretely, the check asserts the two programs share a **shape hash**, differ in
**closure hash**, and both carry a stable **storage identity** of `state_a/2`.
Three axes:

| axis | what it names | changes when | example value |
|---|---|---|---|
| **shape / layout hash** | the column structure | a declared column is renamed or retyped | shared by both programs above |
| **closure / semantic hash** | what the rules compute | any rule body, conjunction order, or source rule order changes | differs between the two |
| **storage identity** | *which table* | never, for a given rel | `state_a/2`, a Name/Arity |

The third one is the answer to the user's question. **A relation's true
coordinate is its Name/Arity in the program namespace.** It is not derived from
content, it is not a hash, and it is stable precisely because it is declared
rather than computed.

The other two receipts complete the picture. `specialization_cache_receipt`:

```
PASS 6 calls -> 3 code templates, 6 state/storage instances
```

Six call sites collapse to three code keys and stay six instance keys. That
**is** the "content hash for rels and instance hash for rels" the user described,
built and passing. The content hash names the *specialization* (the generic rule
plus its type arguments); the instance hash names *this site's storage*. And
`lowered_sql_template_receipt`:

```
PASS renamed programs share abstract lowered SQL, raw SQL stays bound
```

Two programs differing only in rel names produce byte-identical *abstract* SQL
(with `$input` / `$state` placeholders) and different *raw* SQL. That is
monomorphization with the receipt already written: the abstract template is the
generic rule, the raw statement is one specialization.

### 5.4 So what does naming a rel actually require

Not a new kind of value. A **binding time**.

The pair (content hash of the specialization, instance hash of the site) is
exactly the coordinate a compile-time generic needs, and it is already computed.
What is missing is only a surface spelling that lets a user write the generic
rule and name the instantiations, plus the expansion that stamps out one
specialized rule per instantiation before anything reaches SQL.

The user's intuition was correct and the artifact they remember building is the
right artifact. The one correction is directional: those hashes are a
**specialization cache key**, not a runtime handle. They let the compiler know
two call sites want the same generated code. They were never going to let a row
hold a relation.

---

## 6. Was this already ruled? Yes, twice

Found in `chat_log/20260729.4.rel-edge-clock-fixpoint.pl`:

```prolog
locked(higher_order_runtime_boundary,
       'named relations and rules may be compile-time composition arguments,
        but specialization removes them before SQL; no function-valued rows').

locked(higher_order_lowering,
       'named rule argument specializes through a canonical compile-time
        signature into keyed rels and ordinary arrows; no function value
        survives into the checked graph or SQLite').
```

Those are the Souffle-components answer, in this project's own words, locked
before this lane existed. The independent prior-art survey in section 4 reached
the same conclusion from the outside, which is a useful cross-check rather than
a coincidence.

And it is prototyped. `v6/prolog/labs/generic_scan_instantiation/` implements
`scan_spec/6` as compiler metadata plus `specialize_scan/3` that erases it into
the ordinary program IR. At HEAD, 8 of its 9 remaining receipts pass
(`receipt_arithmetic_registry` fails, pre-existing registry drift, unrelated to
this lane):

```
PASS specialized scan folds ordered duplicate-capable events and partitions by key
PASS scan erases before the real checker and SQL lowerer; 3 named rels, 1 TEMP pre, 0 helper tables
PASS identical call sites share one definition-sensitive specialization and helper name
PASS explicit StateRel names select 2 separate tables or 1 shared table
PASS nested scan is an explicit composite-key child StateRel
PASS unknown init, type mismatch, multi reducer, recursion, and dynamic rel names refuse before lowering
PASS A <- B already composes concrete rels; selecting B as an algorithm argument is the higher-order remainder
```

Three of those deserve to be read slowly:

- *"scan erases before the real checker and SQL lowerer"* is monomorphization
  demonstrated against the real compiler, not a mock.
- *"dynamic rel names refuse before lowering"* is `scan_refusal/2`'s
  `dynamic_relation_name` clause, which fires on `\+ ground(RelRef)`. The lab
  already decided that a non-ground relation argument is a **named refusal**.
  That is the missing piece from section 3.3: the machinery to refuse the
  silent case is written, it just is not wired to the surface.
- *"A <- B already composes concrete rels; selecting B as an algorithm argument
  is the higher-order remainder"* is the lab stating the exact gap this lane was
  asked about.

**This lab supersedes none of that.** What it adds is: the grading of the six
candidate spellings in section 3 (which that lab did not enumerate), the four
edges of the landed feature in section 1.4, and the observation in 3.3 that the
requested spelling currently fails *silently* rather than via the
`dynamic_relation_name` refusal that lab already designed.

---

## 7. The generics recommendation

### 7.1 A correction to the question

The brief for this lane said the types-as-rels lab concluded `list(T)` should be
"the only parametric type". Reading
`plans/2026-07-28-types-as-rels-verdict.md:76`, that is not what it says. The
verdict's table maps `list(T)` to:

```
rel cons(id, head, tail) set key(2,3)  +  rel nil(id) set key(1)
```

marked "already exists: yes", under a table whose closing line is "Nothing in
the right-hand column is new. That is the whole result." The verdict concludes
there are **zero** parametric types, and that lists are two ordinary rels. There
is no `T` to be parametric over.

### 7.2 The recommendation

**Do not reopen the types-as-rels verdict. It is about a different plane and it
stands unchanged.**

The two questions only sound alike:

| | types-as-rels verdict | the user's rel-as-argument idea |
|---|---|---|
| plane | **values**: what shapes can a column hold | **rules**: which rel does this body read |
| answer | plain rels, no parameters, `list(T)` is `cons` + `nil` | compile-time specialization, per `locked(higher_order_lowering)` |
| binding time | n/a, nothing is parameterized | compile time, before SQL |

A rule generic over which rel it reads does not make any *value* parametric. It
makes a *rule text* reusable. After specialization the values are as concrete as
they ever were, which is the entire content of
`locked(higher_order_runtime_boundary)`.

So the recommendation is: **keep zero parametric types, and treat rule-level
generics as a separate, already-ruled axis.** If rule generics do land, the
value plane does not change, and `list(T)` stays `cons` + `nil`.

One thing worth watching. Rust is the cautionary case: generics over types
(monomorphization) and generics over rule text are the same machinery there, and
the ergonomics pressure to unify them is strong. This project has good reason to
keep them apart, because the value plane's simplicity is what makes the two-door
grading tractable. That is a reason to state the split explicitly rather than
leave it implicit.

---

## 8. Cards

Each card has at least two real options. None is a recommendation dressed as a
question.

### Card 1: `relation_value_in_edge_rule` (Edge 3, section 1.4)

The compiled door now refuses `post(user(Id, Name)) <+ user(Id, Name)`. The
reference engine still runs it. Lab 9 has a receipt that it used to lower
correctly. Edge rules are the state-machine idiom and the natural first customer
for a specialized generic rule.

- **1a. Treat as a blocker, fix before any higher-order work.** Restore edge-rule
  construction on the compiled side so the doors agree again. Cost: real work in
  the edge lowering path, which compiles against RelPlans alone today.
- **1b. Accept it as a limit, and make the reference engine refuse it too.**
  Cheap, restores door agreement immediately, and costs a working reference-engine
  capability.
- **1c. Leave as is.** Doors disagree; anything reaching for it hits a refusal on
  one side and a working program on the other.

### Card 2: the variable that slips past the ref-column refusal (Edge 1)

`relation_argument_violation` requires `nonvar(Value)`, so `span(Path, S, E)`
with a text `Path` is accepted by **both** doors, and the compiled side writes
the text into an `INTEGER NOT NULL` primary-key column.

- **2a. Extend the check to column types.** Refuse when a variable's inferred
  type does not match the declared ref type. Uses the type inference that already
  exists; catches the general case.
- **2b. Add a runtime shape assertion on the reference engine's store path.**
  Cheaper, catches it as a loud failure rather than a silent store, but only when
  a row actually flows.
- **2c. Add a fixture pinning current behavior and leave it.** Documents the hole
  without closing it.

### Card 3: the depth-one redundant join (Edge 2)

Uniform lowering costs depth one an extra table and a full-column join in both
the insert and delta statements, against a view over the very table already in
scope.

- **3a. Special-case depth one back to a direct `__id` projection.** Restores the
  old plan, costs a branch in the lowering, and the two red lab checks become the
  regression test.
- **3b. Keep uniform lowering, prove the cost is acceptable.** Measure the join
  at real corpus scale; if SQLite's planner collapses it, record that with a
  receipt and close the question.
- **3c. Change the receipt standard first.** Assert table count and statement
  shape, not only SEARCH-versus-SCAN, then decide. This one is independently
  worth doing whichever of 3a/3b wins.

### Card 4: the silent `call/N` (section 3.3)

The requested spelling produces a phantom `rel call/3` and zero rows, on both
doors, with no message. The `dynamic_relation_name` refusal that would name it
already exists in the `generic_scan_instantiation` lab.

- **4a. Add a named refusal for meta-call shapes now, ahead of any feature.**
  Small, self-contained, and turns the worst failure shape into a sentence that
  tells the user the truth. Would pay for itself even if rule generics never land.
- **4b. Narrow `edb_definition` so an unknown functor in goal position is not
  silently an input rel.** Bigger blast radius, addresses the general class rather
  than this instance.
- **4c. Leave it.** Anyone who tries the natural spelling gets an empty rel.

### Card 5: whether rule-level generics get a surface at all

Ruled in principle by `locked(higher_order_lowering)`, prototyped in
`generic_scan_instantiation`, never given a spelling.

- **5a. Souffle-component shape.** A named group of rules parameterized by rel
  arguments, instantiated explicitly. Closest to the existing locks and to the
  lab. Cost: a new declaration form, and generated statements scale with the
  number of instantiations.
- **5b. Signature-and-instantiation shape,** following the lab's `scan_spec/6`
  literally: metadata plus an expansion, no new surface syntax at all, instantiations
  written as ordinary declarations. Cheapest path, least expressive.
- **5c. Do not give it a surface.** Keep hand-monomorphization (B1) and the
  discriminant-column encoding (B2/B6) as the answer, and spend the budget
  elsewhere. Honest, and the receipts in section 3 show both encodings work today.

### Card 6: what to do with the labs this lane read

- **6a. Fold both.** Distil `rel_value_unification`'s standing conclusions into
  the permanent record and delete it, per the lab protocol, recording the commit
  that holds the last copy. Its two red checks move to Card 3 as regression tests
  first.
- **6b. Keep `rel_definition_hash` alive until rule generics are decided.** It is
  the only executable statement of the three-layer coordinate and Card 5 will want
  it.
- **6c. Fold `rel_value_unification`, keep `rel_definition_hash` and
  `generic_scan_instantiation`.** Splits the difference along the line this lane
  found: the row-identity work is finished, the rel-identity work is not.

---

## 9. Receipts index

Everything asserted above, and how it was produced. All runs hermetic
(`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`), all on base `609066ee`.

| claim | how |
|---|---|
| read side is sugar (A1) | scratch probe, both spellings in one program, reference engine |
| forwarding needs no term (A2) | scratch probe + lab 11 `typed_variable_forwards_opaque_identity_without_target_rejoin` PASS |
| variable slips the ref refusal (Edge 1) | scratch probe: `span/3 = [span('src/a.rs',10,20)]`; source at `0_program_check.pl` `relation_argument_violation` first line `nonvar(Value)` |
| both doors accept the type violation (Edge 1) | `program_plan/2` + `lower_program/2`: insert selects `b0."path_name"` into `"file"`, DDL declares `"file" INTEGER NOT NULL` in the primary key |
| depth-1 join regression (Edge 2) | `lower_program/2` on the lab-11 program; before-shape from lab 11's own assertion; `__ref_user` DDL dumped |
| lab 11 two checks red | `swipl -q -f v6/prolog/labs/rel_value_unification/11_ref_necessity.pl` |
| edge-rule refusal is new (Edge 3) | `git log -S relation_value_in_edge_rule` returns only `472320f4`; lab 9 run shows the red check and the refusal term |
| arity miscompile became a refusal (Edge 4) | lab 9 `key_only_constructor_is_not_a_current_relation_term` now red with `relation_pattern_not_a_relation_value` |
| B1/B2/B6 expressible, B3/B4/B5 silent | scratch probe suite, reference engine, output quoted in section 3 |
| `call/3` becomes a phantom EDB rel | `program_plan/2` plan dump: `rel call/3 kind=set cols=[col1,col2,col3]` |
| compiler accepts the meta-call silently | `program_plan/2` + `lower_program/2`, no throw |
| three identity layers | `rel_definition_hash` 11 PASS, `identity_layer_receipt` |
| specialization cache is real | same lab, `6 calls -> 3 code templates, 6 state/storage instances` |
| monomorphization against the real compiler | `generic_scan_instantiation`, 8/9 receipts, `receipt_arithmetic_registry` pre-existing red |
| the locks | `chat_log/20260729.4.rel-edge-clock-fixpoint.pl`, `higher_order_runtime_boundary` and `higher_order_lowering` |
| `list(T)` is not parametric | `plans/2026-07-28-types-as-rels-verdict.md:76` and the table's closing line |
| prior art | four local `theory/datalog-study` skill files plus fetched primary docs, cited inline in section 4 |
