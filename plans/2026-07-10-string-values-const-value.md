# String values: df_lit + const_value + occurrence-level scip (2026-07-10)

## Context

Agent-session feedback: SCIP resolves the LEFT side of a route table
(`const routes = { home: '/home' }` — every reference to `routes.home`) but the
RIGHT side (the literal string) is unreachable in any rel, so joining call sites
to concrete URL shapes — or rewriting the paths — falls back to handwritten
sg + replace per shape. Two new capabilities requested: a value-propagation rel
folding a resolved definition to its literal string/template, and intra-procedural
string flow so template interpolation counts as dataflow.

Code facts that shape the design (verified 2026-07-10, base 0fc47da):

- Template interpolation flow ALREADY exists: `template` df_nodes with an edge
  from every `${expr}` (typegraph.rs:1462, tagged templates :1474).
- `lit` df_nodes carry EMPTY `var` (typegraph.rs:1257 and every other lang's
  lit push) — no rel anywhere holds a literal's text. This is the real gap.
- `df_field(id, field, value)` already captures TS object-literal properties;
  the value column is a df_node id that dead-ends at a textless lit node.
- `EntityKind::Const` exists in the tag vocabulary (typegraph.rs:58) but NO
  lift constructs it — plain consts emit no type_entity row.
- TS `+` concat: BinaryExpression falls to the `_ => expr` arm — operands are
  not edged in at all (a general flow hole, not just a string gap).
- The scip name override is name-level; `scip_occurrence` (v0.6.24) has exact
  0-based per-occurrence spans nobody consumes. Ledgered as the with-scip
  headroom (rust capped at 27.5% despite holding the truth index).

## Design

### 1. `df_lit(id, text, kind)` + `df_lit_rev` (dataflow family)

One row per STRING-carrying value node: id = the df_node id, text = the cooked
string value for plain literals, the raw source slice (holes intact) for
templates/concats, kind ∈ lit|template|concat. Emitted where the lifts already
push lit/template nodes — same walk, no second parse. String literals only
(numbers/bools/regex excluded; bounded rows, and the use case is strings).
Rev twin dual-emits like df_field/df_field_rev (D5 pattern, rev-salted ids).
TS/TSX/JS first (the use case); Rust `syn::Lit::Str` if trivial in the same
pass; Kotlin/Go/Python ledgered.

### 2. `concat` df_node kind (TS lift)

`a + b` chases both operands into a node of NEW kind `concat` (added to the
df_node_kind brand variants) instead of the unchased `expr`. Not overloading
`template`: queries for string construction match kind IN (template, concat)
explicitly. Any-operand `+` qualifies (JS `+` flow is true value flow whether
string or numeric).

### 3. `const_value(repo, sym, field, text, kind, file, line)` + `_rev` (type family)

The requested fold, emitted from the entity walk (rides TypeFacts like
doc_comment — the AST is in hand, no scip join needed on the DEF side):

- `const name = '/x'` → (sym = file::const::name, field = "", text, kind=lit).
  Requires ALSO minting the missing `type_entity` const row (EntityKind::Const)
  so sym joins work.
- `const routes = { home: '/home', nested: { a: '/a' } }` → one row per
  string-valued property, field = dotted key path ("home", "nested.a").
  Spread properties counted loudly, not followed.
- Template initializers → kind=template, text = source slice with `${}` intact.
- String enum members (`enum Routes { Home = '/home' }`) → sym = the enum's
  sym, field = member name, kind=lit.
- SOUNDNESS RULE: `const` (and `as const`) only. A `let`/`var` string init is
  counted loudly per refresh, never emitted — a mutable binding makes the fold
  a lie. Fails toward exclusion.

Consumers then join reference→def with scip (name-level today, occurrence-level
after item 5) and read the value: `scip_ref ⋈ const_value` replaces the
handwritten sg + replace.

### 4. `std/strings.dl` string_flow view (dl authoring, no engine work)

`string_flow(from, to)` = df_edge restricted to the string-carrying subgraph:
recursive rel seeded from df_lit ids (the flow-ctor precedent — a closure rel
cannot be read unpinned in a rule body), edges followed forward through
template/concat/var/call nodes. Plus a readable trace view joining df_lit.text.
Example program demonstrating the route-table shape end to end.

### 5. Occurrence-level scip resolution (P1 from the scip damage plan)

The resolve closures in extract.rs consult position before name: build a
per-(repo, file) map from `scip_occurrence` (line 0-based → symbol) ⋈ scip_def,
look up (file, call_site.line - 1) filtered to the site's as-written text
(descriptor name or scip_binding local alias). Exactly one match → resolved;
same-line same-name multi → refuse; miss → fall back to the existing name map.
The name-conflict refusal (9fd029b) becomes moot wherever positions exist.
ONE line-base conversion, commented (df/call 1-based, scip 0-based — the
scorer precedent).

Gates on the corpus oracle (tests/it/oracle_corpus.rs, SPREFA_CORPUS_DIR):
per-language parity strictly above the current with-scip arm (rust 27.5% /
go 89.0% / python 78.0% / ts 20.4%), precision ≥ the without-scip arm.
Expect rust to move the most (trait-method call sites carry exact occurrences).
scip_gate.rs conflict test REWRITES to the stronger claim: with ranges on the
occurrences, both same-name sites resolve to their OWN defs by position.

## Non-goals

- Cross-statement const folding (`const a = '/x'; const b = a + '/y'` folding
  b's VALUE) — string_flow gives the reachability; folding is a later pass.
- Kotlin/Go/Python df_lit + const_value (ledgered follow-up).
- Barrel/re-export chasing (separate ledger item, module_export_rev).

## Staffing

Two worktree agents off base 0fc47da, hand-merged sequentially (both touch
extract.rs — the Go+Python double-land precedent):

- Agent A (Sonnet): items 1-4. typegraph.rs lifts, rel decls + reserved-name
  guard + catalog (builtin-rel checklist), refresh plumbing, std/strings.dl,
  tests (typegraph units + e2e per rel + std flow e2e).
- Agent B (Opus): item 5. extract.rs resolve closures, scip_gate.rs rewrite,
  corpus re-measure with numbers in the debrief.

## Verification

- `cargo test --lib` + `--test it` green in each worktree, then on merged main.
- Rail sweep: `dl --check` (magic-rel audit must stay green — new rels are
  catalogued RelDecls, never literal-name reads), README/reference regen.
- Corpus oracle two-arm re-run for item 5 with before/after table.
- Dogfood: a route-table fixture query resolving `routes.X` references to
  literal paths via scip_ref ⋈ const_value.
