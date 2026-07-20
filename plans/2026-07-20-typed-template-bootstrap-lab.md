# Typed Template Bootstrap Lab

## Context

The isolated `labs/bootstrap-typegen-lab` spike proves that one Rust program can
read a declaration file and emit compilable Rust server code plus an executable
JavaScript fetch client. It does not yet prove the proposed language or its
bootstrap:

- `Field`, `TypeDecl`, `Endpoint`, and `Schema` are handwritten string-bearing
  Rust structs in `labs/bootstrap-typegen-lab/src/main.rs:3-27`.
- Parsing is line-oriented and recognizes only `type` and `endpoint` forms in
  `labs/bootstrap-typegen-lab/src/main.rs:29-75`.
- Template recognition scans only `{name}` substrings in
  `labs/bootstrap-typegen-lab/src/main.rs:124-136`.
- Server matching truncates a template at its first opening brace in
  `labs/bootstrap-typegen-lab/src/main.rs:138-184`.
- Stage one is a copied parser template that happens to use generated structs in
  `labs/bootstrap-typegen-lab/src/main.rs:228-277`.
- The schema at `labs/bootstrap-typegen-lab/schema.dl:1-26` describes four
  records and one HTTP endpoint. It has no first-class pattern, path, slot,
  matching, composition, destructuring, relation, or consumer declarations.

This arc replaces that spike with a single-binary, isolated language laboratory
whose central primitive is a typed template/path term. HTTP, filesystem paths,
field access, channels, queues, and arbitrary key formats are consumers of the
same semantic representation. No shipping `sprefa` crate, daemon, parser,
workspace member, or call site changes during this arc.

Base SHA: `b5c80ad7a60a0c5200a2f83d27c19d8acf1f84c7`.

## Decisions

1. The semantic center is `Pattern`, `Path`, `Slot`, and `Term`; HTTP endpoint
   declarations are one consumer.
2. `{name}` and `:name` are accepted spellings for named slots and normalize to
   one slot representation while preserving source spelling.
3. Positional and named invocation are properties of slots and calls rather than
   consequences of the chosen slot spelling.
4. Dot, slash, colon, and brace syntax lower into typed segments. Domain-specific
   validation occurs after normalization.
5. A pattern supports construction, matching, destructuring, composition, and
   slot enumeration from one semantic term.
6. The compiler is one Rust binary with an embedded relational evaluator.
7. Soufflé supplies language-design reference points only. There is no Soufflé
   process, C++ generation step, runtime installation, or FFI boundary.
8. Facet is deferred to an import adapter after the native type graph passes the
   bootstrap and consumer tests.
9. Stage zero may contain a small trusted parser and emitter kernel. Stage one
   must consume generated semantic model types and reproduce byte-identical
   normalized schema and generated artifacts.
10. Generated artifacts live under the lab's `target/` tree and are reproducible
    from checked-in source.

Rejected alternatives:

- Actual Soufflé backend: introduces an external compiler/runtime boundary and
  fixes rule programs at a different compilation phase.
- Facet as the semantic graph: Facet shapes describe Rust types and do not own
  dynamically authored language declarations.
- URL-specific route model: prevents field, filesystem, queue, and channel
  consumers from proving the generic pattern algebra.
- Separate template-type and runtime-template implementations: duplicates
  parsing, binding, matching, and composition semantics.
- General identifiers that greedily consume every delimiter: makes unquoted
  grammar boundaries and diagnostics unstable. Quoted identifiers carry literal
  slash, colon, dot, brace, and URI text.

## Scope

The lab must support:

```text
type UserId = String
type EventKind = "created" | "deleted"

type User {
  id: UserId
  profile: Profile
  tags: Array<String>
  metadata: Map<String, String>
}

pattern UserPath = `users/{id: UserId}`
pattern UserEvent = `users/:id/events/{kind: EventKind}`
pattern UserField = `users.{id: UserId}.profile.name`

consumer http {
  get UserPath -> User
}

consumer channel {
  subscribe UserEvent -> User
}
```

Required operations:

```text
bind(UserPath, id: "42")
bind(UserEvent, "42", "created")
match(UserEvent, "users/42/events/created")
destructure(UserField, "users.42.profile.name")
compose(UserPath, `events/{kind: EventKind}`)
slots(UserEvent)
paths(User)
```

Required consumers:

- Rust record and enum generation.
- JavaScript data declarations and typed JSDoc or TypeScript declaration output.
- HTTP server matcher and JavaScript fetch client.
- Filesystem path formatter and matcher.
- Channel or queue topic formatter and matcher.
- Record field-path enumeration, including arrays and maps.

Excluded from this arc:

- Changes to shipping `sprefa` crates or V6 traits.
- LSP, incremental parsing, source maps, package resolution, and daemon wire
  integration.
- Production HTTP transport behavior, authentication, retries, streaming, or
  OpenAPI completeness.
- Arbitrary user-defined effects or native plugins.
- Higher-kinded polymorphism beyond generic type application and generic
  pattern slot types.

## Source layout

New Rust source files follow dependency and reading order:

```text
labs/bootstrap-typegen-lab/src/
  0_ids.rs
  1_symbols.rs
  2_source.rs
  3_syntax.rs
  4_parser.rs
  5_types.rs
  6_patterns.rs
  7_store.rs
  8_check.rs
  9_eval.rs
  10_facts.rs
  11_rules.rs
  12_codegen_rust.rs
  13_codegen_js.rs
  14_bootstrap.rs
  15_cli.rs
  main.rs
```

Tests mirror source numbering under `tests/` or remain colocated when private
types make integration tests mechanically expensive.

## Type signatures

### Identity, symbols, and source

```rust
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct TypeId(u32);

struct PatternId(u32);
struct TermId(u32);
struct Symbol(u32);
struct SourceId(u32);

struct Span {
    source: SourceId,
    start: u32,
    end: u32,
}

struct SymbolTable {
    interner: lasso::Rodeo,
}

impl SymbolTable {
    fn intern(&mut self, text: &str) -> Symbol;
    fn resolve(&self, symbol: Symbol) -> &str;
}
```

Pseudo bodies:

```rust
fn intern(&mut self, text: &str) -> Symbol {
    // Intern once in the process-wide compilation store.
    // Convert the lasso spur into the lab's opaque Symbol.
}

fn resolve(&self, symbol: Symbol) -> &str {
    // Borrow text from the symbol table for the duration of the store borrow.
}
```

### Syntax

```rust
enum SyntaxDecl {
    Type(SyntaxTypeDecl),
    Pattern(SyntaxPatternDecl),
    Consumer(SyntaxConsumerDecl),
    Relation(SyntaxRelationDecl),
    Rule(SyntaxRuleDecl),
}

enum SyntaxTypeExpr {
    Name(SyntaxName),
    Literal(SyntaxLiteral),
    Record(Vec<SyntaxField>),
    Union(Vec<SyntaxTypeExpr>),
    Apply {
        constructor: Box<SyntaxTypeExpr>,
        args: Vec<SyntaxTypeExpr>,
    },
}

struct SyntaxTemplate {
    quote_span: Span,
    parts: Vec<SyntaxTemplatePart>,
}

enum SyntaxTemplatePart {
    Literal { text: String, span: Span },
    Slot(SyntaxSlot),
}

struct SyntaxSlot {
    spelling: SlotSpelling,
    name: Option<SyntaxName>,
    ty: Option<SyntaxTypeExpr>,
    span: Span,
}

enum SlotSpelling {
    Braces,
    Colon,
}

fn parse(source: SourceId, text: &str) -> ParseOutput;
```

Pseudo body:

```rust
fn parse(source: SourceId, text: &str) -> ParseOutput {
    // Tokenize declarations and quoted template bodies without semantic lookup.
    // Parse both {name: Type} and :name slot spellings.
    // Preserve delimiters and spans on every syntax node.
    // Recover at declaration boundaries and return syntax plus diagnostics.
}
```

### Semantic types and patterns

```rust
enum Type {
    Primitive(Primitive),
    Literal(Value),
    Record(RecordType),
    Union(Vec<TypeId>),
    Array(TypeId),
    Map { key: TypeId, value: TypeId },
    Optional(TypeId),
    GenericParam(Symbol),
    Apply { constructor: TypeId, args: Vec<TypeId> },
    Pattern(PatternId),
    Error,
}

struct RecordType {
    name: Symbol,
    fields: Vec<Field>,
}

struct Field {
    name: Symbol,
    ty: TypeId,
    span: Span,
}

struct Pattern {
    name: Option<Symbol>,
    parts: Vec<PatternPart>,
    output: TypeId,
    span: Span,
}

enum PatternPart {
    Literal(Symbol),
    Slot(Slot),
}

struct Slot {
    name: Option<Symbol>,
    position: u32,
    ty: TypeId,
    spelling: SlotSpelling,
    span: Span,
}

enum Term {
    Value(Value),
    Type(TypeId),
    Pattern(PatternId),
    Path(Path),
    Apply { callee: TermId, args: Vec<Argument> },
}

enum Argument {
    Positional(TermId),
    Named { name: Symbol, value: TermId },
}
```

### Storage

```rust
struct Store {
    symbols: SymbolTable,
    sources: la_arena::Arena<Source>,
    types: la_arena::Arena<Type>,
    patterns: la_arena::Arena<Pattern>,
    terms: la_arena::Arena<Term>,
    declarations: IndexMap<Symbol, Declaration>,
    facts: FactStore,
    diagnostics: Vec<Diagnostic>,
}

impl Store {
    fn alloc_type(&mut self, ty: Type) -> TypeId;
    fn alloc_pattern(&mut self, pattern: Pattern) -> PatternId;
    fn lookup(&self, name: Symbol) -> Option<&Declaration>;
    fn lower_module(&mut self, syntax: &SyntaxModule) -> SemanticModule;
}
```

Pseudo body:

```rust
fn lower_module(&mut self, syntax: &SyntaxModule) -> SemanticModule {
    // Pass 1: intern declaration names and reserve stable IDs.
    // Pass 2: lower type expressions against reserved declarations.
    // Pass 3: normalize templates into literal and slot parts.
    // Pass 4: typecheck slots, consumers, calls, and compositions.
    // Pass 5: emit normalized facts from the completed graph.
}
```

### Pattern operations

```rust
struct Bindings {
    positional: Vec<Value>,
    named: IndexMap<Symbol, Value>,
}

fn bind(store: &Store, pattern: PatternId, args: &[ArgumentValue])
    -> Result<String, PatternError>;

fn match_pattern(store: &Store, pattern: PatternId, input: &str)
    -> Result<Bindings, PatternError>;

fn compose(store: &mut Store, left: PatternId, right: PatternId)
    -> Result<PatternId, PatternError>;

fn enumerate_slots(store: &Store, pattern: PatternId)
    -> impl Iterator<Item = &Slot>;
```

Pseudo bodies:

```rust
fn bind(...) -> Result<String, PatternError> {
    // Resolve each slot by name first and position second.
    // Validate each supplied value against the slot TypeId.
    // Render literals and encoded slot values in declaration order.
    // Reject missing, duplicate, and unused bindings.
}

fn match_pattern(...) -> Result<Bindings, PatternError> {
    // Compile or retrieve the pattern matcher.
    // Match literal boundaries deterministically from left to right.
    // Parse captures through their TypeId validators.
    // Return both positional order and named lookup over the same values.
}

fn compose(...) -> Result<PatternId, PatternError> {
    // Concatenate normalized parts.
    // Reassign positions while retaining names and source spelling.
    // Reject incompatible duplicate named slots.
    // Intern structurally identical composed patterns.
}
```

### Paths

```rust
struct Path {
    root: TypeId,
    segments: Vec<PathSegment>,
}

enum PathSegment {
    Field(Symbol),
    Index,
    MapKey,
    Slot(Slot),
}

fn enumerate_paths(store: &Store, root: TypeId) -> Vec<TypedPath>;
fn resolve_path(store: &Store, path: &Path) -> Result<TypeId, TypeError>;
```

Pseudo body:

```rust
fn enumerate_paths(store: &Store, root: TypeId) -> Vec<TypedPath> {
    // Traverse records in declaration order.
    // Add [*] for arrays and {key} for maps.
    // Descend through optional values without changing the textual segment.
    // Track visited (TypeId, path depth) states for recursive declarations.
    // Return deterministic path and leaf TypeId pairs.
}
```

### Facts and rules

```rust
enum Fact {
    TypeKind { ty: TypeId, kind: TypeKind },
    Field { owner: TypeId, name: Symbol, ty: TypeId },
    PatternPart { pattern: PatternId, position: u32, part: PartFact },
    SlotType { pattern: PatternId, position: u32, ty: TypeId },
    Consumer { domain: Symbol, pattern: PatternId, output: TypeId },
    Path { root: TypeId, path: Symbol, leaf: TypeId },
}

trait Rule {
    fn evaluate(&self, input: &FactStore, output: &mut FactDelta);
}

fn saturate(rules: &[Box<dyn Rule>], facts: &mut FactStore) -> RuleStats;
```

Pseudo body:

```rust
fn saturate(...) -> RuleStats {
    // Evaluate rules over indexed fact columns.
    // Insert only novel tuples into the next delta.
    // Continue until the delta is empty.
    // Record iteration, tuple, and allocation counts.
}
```

The first rule set is fixed in Rust. User-authored rule syntax enters only after
the fact vocabulary, monotonicity contract, and diagnostics pass the bootstrap
suite.

### Code generation and bootstrap

```rust
trait Emitter {
    fn emit(&self, store: &Store, module: &SemanticModule)
        -> Result<Vec<Artifact>, EmitError>;
}

struct Artifact {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn bootstrap(stage0: &Compiler, source: &str) -> Result<BootstrapReport, Error>;
```

Pseudo body:

```rust
fn bootstrap(stage0: &Compiler, source: &str) -> Result<BootstrapReport, Error> {
    // Compile the language's own semantic declarations with stage zero.
    // Emit stage-one Rust sources using those declarations.
    // Compile stage one into a temporary binary.
    // Run stage one over the identical source corpus.
    // Compare normalized semantic dumps and generated artifacts byte for byte.
    // Return paths, hashes, timings, and mismatch diagnostics.
}
```

## Instance timelines and lifetimes

One `Compiler` instance owns one `Store` for one invocation.

```text
CLI invocation
  create Compiler and Store
  intern builtins
  load source buffers
  parse borrowed source text into owned syntax nodes plus spans
  reserve declarations
  lower and typecheck into arenas
  emit facts
  saturate rules
  run requested evaluators and emitters
  write artifacts atomically
  drop Store and all arena data
```

Source text is owned by `Source` entries. Spans use offsets rather than Rust
references. Syntax and semantic nodes therefore have no source-text lifetimes.
`SymbolTable::resolve` returns a borrow tied to `&Store`; generated artifacts own
their bytes before writes begin. Match results own captured `Value`s so they can
outlive temporary matcher state while remaining bounded by the compiler
invocation.

Stage-zero and stage-one compiler processes never share pointers or arena IDs.
Bootstrap equality compares canonical dumps and content hashes rather than raw
IDs.

Compiled pattern matchers are cached by `PatternId` inside a compiler invocation.
They borrow no source text and are dropped with the store.

## Storage, reads, writes, and uniqueness

### Storage ownership

- `SymbolTable` uniquely owns interned text.
- Each arena uniquely owns one semantic node category.
- IDs are category-specific and valid only within one `Store`.
- `declarations` uniquely maps each module-level symbol to one reserved
  declaration. Duplicate declarations produce diagnostics and retain the first
  canonical reservation.
- `FactStore` owns normalized tuple sets and indexes; semantic arenas remain the
  source of truth.
- Generated output is staged in memory and written under
  `target/bootstrap-generated/.staging/` before atomic rename.

### Read and write sequence

```text
read schema sources
write Source arena
read Source arena
write syntax module
read syntax declarations
write symbol table and declaration reservations
read reservations plus syntax
write type, pattern, and term arenas
read semantic arenas
write initial facts
read fact indexes and deltas
write derived facts until fixed point
read semantic graph and derived facts
write staged artifacts
rename staged artifacts into generated output
```

### Uniqueness conditions

- Declaration names are unique per module namespace.
- Field names are unique per record.
- Named slots are unique per pattern unless repeated occurrences have identical
  types and explicitly request equality matching.
- Slot positions are contiguous after normalization and composition.
- A supplied named argument binds one slot; a positional argument binds one
  unbound position.
- Generated artifact paths are unique per emitter invocation.
- Structurally equivalent types and patterns may be interned only after source
  diagnostics retain their original spans independently.

## Sequencing

### Phase 0: preserve and fence the spike

- Capture current generated artifacts as fixtures.
- Mark the old stringly compiler path as `legacy_spike` inside the lab.
- Add a command proving that no dependency points at a shipping `sprefa` crate.

### Phase 1: semantic kernel

- Add numbered identity, symbol, source, type, pattern, path, and store modules.
- Implement records, arrays, maps, optionals, literal unions, and generic
  application.
- Add canonical semantic dump snapshots.

### Phase 2: parser and normalization

- Parse declarations, quoted identifiers, template literals, `{name}` slots,
  `:name` slots, positional slots, and typed slots.
- Normalize dot, slash, brace, and colon forms into segments and slots.
- Emit span-bearing diagnostics for malformed and ambiguous templates.

### Phase 3: evaluator

- Implement bind, match, destructure, compose, slot enumeration, path
  resolution, and path enumeration.
- Add deterministic snapshots over mixed slot spellings and invocation styles.

### Phase 4: embedded relations

- Lower semantic nodes into typed facts.
- Add indexed fixed-point evaluation for path derivation, consumer compatibility,
  missing bindings, and route overlap.
- Record rule iteration and tuple counts.

### Phase 5: emitters and consumers

- Generate Rust and JavaScript types from the semantic graph.
- Generate HTTP matcher and fetch client from pattern operations rather than
  prefix strings.
- Generate filesystem and channel/queue matchers from the same patterns.
- Exercise generated client against generated server.

### Phase 6: bootstrap

- Describe the semantic model in the language itself.
- Generate stage-one Rust model, parser tables, evaluator tables, and emitters.
- Compile stage one with `rustc` or an isolated generated Cargo package.
- Require stage-zero and stage-one canonical dumps and generated artifacts to
  match byte for byte.

### Phase 7: memory and scale

- Benchmark repeated and unique symbols, deep records, wide records, nested
  arrays/maps, large patterns, and fact saturation.
- Compare interned and non-interned symbol storage.
- Record peak allocated bytes, bytes per declaration, bytes per pattern part,
  fixed-point iterations, and generated artifact sizes.

## Verification

### Unit and snapshot tests

- Parser snapshots for records, aliases, unions, generics, quoted identifiers,
  `{name}`, `:name`, positional slots, and mixed delimiters.
- Semantic snapshots with stable textual IDs.
- Bind and match round trips:

  ```text
  match(P, bind(P, args)) == args
  ```

- Composition associativity for compatible patterns after normalization.
- Duplicate-name, missing-binding, extra-binding, ambiguous-match, and type
  mismatch diagnostics.
- Path snapshots for records, arrays, maps, optionals, unions, and recursive
  types.
- Fact and fixed-point snapshots, including tuple and iteration counts.

### End-to-end gates

```sh
cargo test --manifest-path labs/bootstrap-typegen-lab/Cargo.toml
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml -- check labs/bootstrap-typegen-lab/schema.dl
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml -- generate labs/bootstrap-typegen-lab/schema.dl
cargo run --manifest-path labs/bootstrap-typegen-lab/Cargo.toml -- bootstrap labs/bootstrap-typegen-lab/bootstrap.dl
```

The generated Rust package must pass `cargo check`. The generated JavaScript
must pass `node --check`. The generated client must call the generated server
and produce a deterministic inline-snapshot response.

### Isolation gate

The lab's Cargo metadata must remain an empty standalone workspace. `cargo tree`
must show no path dependency on repository crates. A repository diff outside
`labs/bootstrap-typegen-lab`, this plan, and the generated `PLANS.md` index fails
the arc.

### Bootstrap gate

Stage zero and stage one must produce equal:

- normalized declaration dump;
- type and pattern graph dump;
- initial and saturated fact dump;
- Rust model artifacts;
- JavaScript model and client artifacts;
- consumer matcher tables.

### Performance and memory gates

Measurements use a counting global allocator and release builds. Each case runs
in a fresh process.

```text
100,000 repeated-name declarations
100,000 unique-name declarations
1,000 records x 1,000 fields
100,000 patterns x 10 parts
one 10,000-segment pattern
100,000 bind operations
100,000 successful matches
100,000 rejected matches
```

The first report records measurements without setting pass/fail thresholds.
Any result with superlinear retained bytes, unbounded fixed-point iterations, or
allocator counts inconsistent with the input cardinality requires a rerun with
heap profiling before interpretation.

<!-- todo(decision): Select the embedded relation storage strategy after measuring Vec-plus-indexes against a Rust Datalog crate on the fixed fact vocabulary. -->

<!-- todo(perf): Record fresh-process allocation and wall-time measurements for every required scale case before selecting interning and matcher-cache policies. -->

<!-- todo(feature): Define the exact positional-slot surface spelling after named brace and colon slots pass parser and normalization tests. -->

<!-- todo(decision): Decide whether repeated named slots mean equality constraints or duplicate-binding errors after matcher ambiguity tests exist. -->

## Staffing

- Implementer: one Codex coding agent, current model class, in the existing
  worktree.
- Worktree: no additional worktree. Preserve all unrelated dirty changes.
- Base SHA: `b5c80ad7a60a0c5200a2f83d27c19d8acf1f84c7`.
- Review points: human review after type signatures, after parser/normalization
  snapshots, after evaluator semantics, and before bootstrap code generation.
- Suite budget: targeted lab tests after each phase; full lab end-to-end suite
  before each review point; no repository-wide format or test run during the
  isolated arc.
- Network budget: none for normal development. Dependency downloads require
  explicit approval and must remain confined to the standalone lab.
