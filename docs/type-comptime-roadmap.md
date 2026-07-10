# Types as data: the comptime roadmap

Design analysis for the dl type system, written 2026-07-10 during the type-shapes arc.
Status markers reflect that date. Companion decls live on branches
`feat/type-shapes-prototype`, `feat/type-decl-row`, `feat/json-agg`.

## 1. The model

dl types are relations, not a separate compiler phase.

Two facts define the whole design:

1. **Type information is stored as rows.** A shape's columns are rows in `_shapes`
   and `rel_col`. An enum brand's variants are a column in `rel_col`. There is no
   type environment that exists only inside the checker.
2. **The tick is the compile step.** Any computation over types is an ordinary
   derived rule. Rules that produce types head `type_decl_row`; the engine persists
   the result at the end of tick N and consumes it at the declare phase of tick
   N+1. This one-tick phase delay is the boundary between "computing a type" and
   "using a type", the same boundary a conventional compiler hides inside itself.

The comparison to Zig is exact in one direction: Zig evaluates functions at compile
time to produce types; dl evaluates rules between ticks to produce schemas. dl adds
a property Zig does not have: the metaprogram is observable. `? type_decl_row(...)`
shows every computed type before it lands, `_shapes` is the compiled output as a
queryable table, and failures surface as diagnostics (`shape-pending`,
`shape-shadowed`, `shape-unknown-type`) rather than as expansion-time magic.

## 2. The registration law

A type feature is complete when it satisfies four requirements:

1. a declaration form (syntax or a reserved sink relation),
2. a load-time or declare-time check with a named diagnostic,
3. its facts landed in an introspection relation,
4. those facts consumable by `type_decl_row` rules.

Requirement 3 is the one that keeps the system composable. A feature whose facts
are not readable as rows cannot participate in any later type-level computation,
so it is a leaf by construction. Every rung below feeds the next one exactly
through its rows.

## 3. The ladder

Each rung states what it is, why the previous rungs are prerequisites, and what a
user can do after it ships that they could not do before.

### Rung 0: brands, enum brands, named shapes (SHIPPED)

`type path_like <: text.` (nominal newtype), `type severity = "error" | "warn".`
(closed literal set), `type finding(path: text, line: int, sev: severity).` with
`rel finding_rel: finding.` (named schema, expanded at load).

Prerequisites: none. This rung IS the base: it creates the column model (`Col`
carries a base type plus an optional brand) every later rung reads and writes.

Unlocks for users: a mistyped literal against a closed vocabulary is a load error
with a nearest-variant suggestion instead of a silently empty result. This targets
the most frequent recorded failure mode of agent-written programs: a typo'd kind
string matching zero rows with no signal. Named shapes remove copy-paste column
lists across related rel decls.

### Rung 0b: ambient builtin vocabularies + `rel_col` (SHIPPED)

Engine-owned enum brands on builtin kind columns (`type_edge.kind`,
`type_entity.kind`, `df_node.kind`, `checkout_done.action`) and the introspection
relation `rel_col(rel, pos, col, type, variants)`.

Prerequisites: rung 0 supplies the brand machinery; this rung applies it to the
engine's own columns and publishes the result as rows (registration law, part 3).

Unlocks for users: the builtin surface becomes self-describing. An agent queries
`rel_col` for the allowed values of a column instead of guessing from documentation,
and the enum check covers builtin columns without any user declaration.

### Rung 1: `type_decl_row` derived shapes (SHIPPED)

A reserved sink `type_decl_row(shape, pos, col, type)` headed by derived rules.
Persisted to `_shapes` at end of tick, consumed at the next declare.

Prerequisites: rung 0's shape expansion seam (a deferred `rel name: shape.` needs
somewhere to resolve from) and rung 0b's `rel_col` (the first useful input to a
type-producing rule). The digest machinery guards the persist so an unchanged
sink does not re-migrate tables every tick.

Unlocks for users: schemas computed from data. Two proven patterns: schema
inference (derive column names and types from an observed JSON document, then use
the result as a checked relation) and mapped types (one rule mints `partial_<rel>`
for every builtin relation from `rel_col`). Type-level programming becomes
ordinary dl, with no macro language.

### Rung 2: JSON output, aggregates and constructors (IN FLIGHT, branch feat/json-agg)

Head aggregates `json_group_array(x)` and `json_group_object(k, v)` beside
count/sum/min/max, plus scalar head functions `json_object(...)`, `json_array(...)`,
`json(x)`, all lowering to SQLite's own JSON functions. Aggregate output must be
deterministic (ORDER BY inside the aggregate) because relation content digests
drive downstream rebuild scoping; a flapping array order would invalidate digests
every tick.

Prerequisites: independent of rungs 0-1 mechanically (it extends the aggregate
and function seams), but it is the other half of a loop rung 1 opened: dl could
already CONSUME nested JSON (term-form `json`/`jsonp` over a bound string) and
infer types from it; after this rung it can PRODUCE nested JSON from flat
relations.

Unlocks for users: usable JSON out of the language. Building an API payload, a
report, or a config file becomes two strata of rules (children aggregate into an
array, the parent embeds it), where previously the only outputs were flat rows
and text templates. This was a directly logged request from another session.

Why this precedes nesting-in-types: nested output needs constructors before
nested types need semantics. Shipping the constructors first means the type-level
nesting rung has something concrete to hydrate into.

### Rung 3: `typeof(sample)` literal inference (UNBUILT, size S)

`type payload = typeof({ "name": "alice", "age": 30 }).` runs the rung-1
inference at load time over an example literal, with no tick delay, inside
`expand_shapes`.

Prerequisites: rung 1 proved the inference logic (the type-from-json example is
the reference implementation); this rung moves it from a derived rule to a syntax
form for the common case where the sample is known when the program is written.

Unlocks for users: example-as-declaration. The cheapest possible way to get a
checked schema: paste a representative object. Combined with rung 0's checking,
a find against the shaped relation with a typo'd field fails loudly, which is
the object-form (lodash-style) find made safe.

### Rung 4: shape keys and reference columns (UNBUILT, size M)

A shape declares a key column; a column whose declared type is another shape name
stores that shape's key. `type customer(id: int key, name: text). type order(id:
int key, customer: customer).` The `order.customer` column holds a customer id,
checked as such. Storage stays flat.

Prerequisites: rung 0 shapes (there must be a named schema to reference) and the
existing lattice `key(...)` qualifier (the parse and PK machinery already exist
on rel decls; this rung moves the concept onto shapes).

Unlocks for users: typed joins. A join through a reference column is verifiable
(the checker knows which relation the value points into), dangling references
become a lintable condition, and "type nesting" acquires its honest meaning:
a type referencing a type is a reference, never an embedded row. This is the
same resolution SQL reached (first normal form plus foreign keys) and the same
one Souffle reaches internally (records are hash-consed to flat ids).

Why it must precede rungs 5 and 6: hydration needs to know which rows a
reference denotes, and container generics (`list_of<T>`) are meaningless until
a column can point at another shape's rows.

### Rung 5: shapes as codecs, pull-style hydration (UNBUILT, size M)

Decode direction: `json(body, :finding)` derives the match pattern from the
shape decl, keys matched by column name, values checked on the way in. The
`q:{...}` pattern language remains for genuinely structural matches (recursive
descent, arrays, key regexes). Encode direction: a pull-style specification
walks reference columns (rung 4) and emits nested JSON via the rung 2
constructors.

Prerequisites: rung 0 (the shape is the contract), rung 2 (the encoder's
constructors), rung 4 (references define the tree the encoder walks).

Unlocks for users: one declaration serving as type, extractor, and serializer.
The Serde-derive economy: today the same field list appears in a `type` decl,
a `q:{}` pattern, and an output template; after this rung it appears once.
Round-trip (ingest JSON, compute over flat relations, emit JSON) with a single
schema source of truth.

### Rung 6: generic shape syntax (UNBUILT, size M)

`type pair<first_type, second_type>(first: first_type, second: second_type).`
with instantiation `rel scored: pair<text, int>.` Lowering: the instantiation
site emits a demand row; a generated rule substitutes parameters and heads
`type_decl_row` with a name-mangled shape (`pair_text_int`); the rel decl defers
exactly like any derived shape.

Prerequisites: rung 1 IS the instantiation mechanism (the phase delay plays the
role of monomorphization; no unification engine is added), and rung 4 makes
container generics meaningful. Note the capability exists today without the
syntax: a rule minting `"partial_" + rel` is a generic in monomorphized form.
This rung is sugar over a proven mechanism, which is why it is safe to defer.

Unlocks for users: reusable parameterized schemas without name-mangling by hand.
Constraint checking on parameters (`<T: some_brand>`) is one check rule on the
demand row.

### Rung 7: cross-language projection (UNBUILT, size S-M per piece)

`type_member(sym, member, type)` (declaration-level field name and type per
entity, recoverable from the existing per-language type walks that today keep
names only for dataflow), plus `type_map(from_lang, from_type, to_lang, to_type)`
mapping facts, plus a rendering rule surfacing as hover diagnostics.

Prerequisites: the type_entity/type_edge extraction tier (shipped, three
languages, shared kind vocabulary) and rung 0b's introspection habit. Independent
of rungs 3-6.

Unlocks for users: a lens that shows a Rust struct as its best-attempt TypeScript
(or the reverse) on hover, and dl shapes minted from source-language types via
`type_decl_row`, making dl the interlingua between the analyzed languages and
its own schema layer.

### Riders (each size S-M, independent, after their prerequisite rung)

- Refinement-style checks: one `diag`-heading rule per shape ("port is 1..65535"),
  needs only rung 0.
- Mercury-style modes: a `mode` column on `rel_col` consumed by a planner lint,
  needs rung 0b.
- Schema history: a `_shapes_rev` twin mirroring `type_entity_rev`, making shape
  drift diffable across revs, needs rung 1.

## 4. Guardrails

Two rails keep the ladder out of the classical trap (nested terms in a fixpoint
make datalog non-terminating, since recursion can mint values forever):

1. **Storage stays flat.** Nesting is references (rung 4) plus constructed output
   (rung 2), never embedded rows. Type-level computation therefore terminates by
   the same argument the fixpoint does: it is stratified datalog over a finite
   universe, with no value constructors.
2. **Every metaprogram effect crosses the tick boundary through `_shapes`.**
   The delay is observable (pending and shadow diagnostics, a queryable table)
   rather than hidden. Same-tick consumption was considered and refused as a
   phase circularity: schemas are needed at declare, rows exist after derive.

## 5. Non-goals

Ruled out 2026-07-09: exhaustiveness checking, type inference over rule bodies,
span-carrying type errors, and any type feature whose payoff is theoretical
rather than a recorded user or agent failure mode. The ladder above is justified
rung by rung against observed failures (silent zero-row typos, copy-pasted
schemas, JSON assembly by string template) and stops where the record stops.
