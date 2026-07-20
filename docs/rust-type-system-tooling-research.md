# Rust tooling for a sprefa-native type system

## Purpose

This is a reconnaissance record for a programmable type environment with:

- Prolog and Datalog-style facts, rules, and recursive queries
- compile-time type inspection
- generic type constructors
- nested field and path enumeration
- array and key/value traversal
- projections and transformations
- Rust and TypeScript generation

The intended implementation is compositional. A library can own a subproblem
without defining the language semantics.

## Tool map

| Subproblem | Library | Role |
|---|---|---|
| Lossless syntax | rowan | Green/red concrete syntax trees and source-preserving edits |
| Parser recovery | chumsky | Recursive parsers, Pratt parsing, partial ASTs, recovery |
| Lexing | logos | Generated deterministic lexer |
| Unification | ena | Type variables, union-find, snapshots, rollback |
| Declaration storage | la-arena or indexed_arena | Stable typed indices for recursive graphs |
| Symbol storage | lasso | Interned names and paths |
| Incremental queries | salsa | Memoized on-demand compiler queries |
| Embedded Datalog | datafrog | Lightweight monotonic tuple relations |
| Rewrite normalization | egg | Equality saturation and type-expression rewrites |
| Diagnostics | miette | Source spans, labels, related errors, and codes |
| Rust emission | syn, quote, prettyplease | Rust syntax trees, token generation, formatting |

## Fit with sprefa

The repository already owns a relational engine, AST extraction, shell
execution, file generation, and incremental dataflow. Those remain the main
composition and query layer.

The current TypeArena in src/engine/type_arena.rs provides content-addressed
logical type identity. Generic arena crates do not replace that function. They
can store declarations, fields, paths, and mutable inference structures around
it.

    rowan / tree-sitter-dl
        source syntax and source-preserving editing

    ena
        inference variables and unification

    la-arena or indexed_arena
        declarations, fields, constructors, recursive references

    lasso
        interned names, field names, module paths

    sprefa engine
        facts, recursive traversal, joins, filters, projections, generation

    salsa
        cached parse/resolve/type-application queries where needed

    Alloy
        Rust and TypeScript output structure, scopes, imports, references

datafrog is relevant prior art for embedded Datalog, but sprefa already has a
larger relational runtime. Salsa should memoize compiler-shaped functions and
not become a second dataflow runtime. egg belongs behind a later normalization
boundary for type aliases and rewrite rules.

## Type representation

The semantic core needs two layers.

### Interned type identity

    enum TypeNode {
        Primitive(Primitive),
        Record(DeclId),
        Array(TypeId),
        Map(TypeId),
        Optional(TypeId),
        Union(Vec<TypeId>),
        Apply { constructor: DeclId, args: Vec<TypeId> },
    }

TypeId identifies a canonical resolved type. Equal applications share an ID.

### Queryable declaration facts

    type_decl(TypeId, Name, Kind)
    type_param(TypeId, ParamId, Name, Constraint)
    type_field(TypeId, FieldId, Name, TypeId, Flags)
    type_variant(TypeId, Name, Value)
    type_apply(TypeId, Constructor, Position, Argument)
    type_parent(TypeId, Parent)

This makes type structure available to the existing query model.

## Dotted paths and nested traversal

Dotted access should lower to field projection. It should not be implemented as
a string-key convention.

    field(User, "profile", Profile)
    field(Profile, "avatar", Optional<String>)

    User.profile.avatar

Equivalent path facts:

    path(User, ["profile", "avatar"], Optional<String>)

Array and map traversal require explicit path segments:

    Field("orders")
    AnyIndex
    Field("id")

    Field("metadata")
    AnyKey

Examples:

    path(User, ["orders", "*", "id"], String)
    path(User, ["metadata", "{key}"], String)

The path relation supports fields, paths, leaves, arrays, maps, optional paths,
and references. Recursive traversal emits a path template for each leaf.

## Generic application and unification

Generic constructors are compile-time functions over types:

    Page<T> = {
        items: Array<T>
        next: Optional<String>
    }

    Page<User>

TypeNode::Apply stores the constructor and argument IDs. ena handles unknown
type variables and reversible constraints. Snapshots allow speculative
resolution and rollback when a candidate branch fails.

chalk is a separate tool for Rust-style goals, associated types, and
logic-programmed trait solving. It becomes relevant if the language adopts that
semantic model.

## Incrementality

There are two useful incremental shapes:

1. relation-level changes, handled by sprefa's existing engine
2. memoized compiler queries, handled by Salsa where the query graph benefits
   from cached inputs and outputs

Candidate Salsa queries:

    parse(file) -> SyntaxTree
    resolve(module) -> Scope
    apply(constructor, args) -> TypeId
    paths(type_id) -> PathSet
    emit(target, type_id) -> GeneratedFiles

Each computation should have one owner. A query should not be represented
simultaneously as a Salsa query and a relation fixpoint unless the boundary is
explicit.

## Code generation

The existing Alloy helper is the output layer for Rust and TypeScript. Its
symbols, scopes, references, and import generation correspond to the
declaration graph produced by the type system.

Rust-native alternatives:

- syn for constructing Rust syntax trees
- quote for token generation
- prettyplease for generated-code formatting

The emitter input should be a resolved declaration graph, not parser nodes.

## Memory and performance reconnaissance

The standalone lab at labs/type-system-rust does two runs:

    cargo run
    cargo run --release -- --stress 100000

The stress mode creates a chain of declarations. Each declaration contains a
recursive record reference, an array field, and a map field. It reports:

- declaration count
- type-node count
- interned-name count
- deepest type ID

Use /usr/bin/time -l around the release run to capture maximum resident set
size on macOS:

    /usr/bin/time -l cargo run --release \
      --manifest-path labs/type-system-rust/Cargo.toml -- --stress 100000

The stress mode is a smoke test, not a benchmark. It tests whether the data
structures exhibit unexpectedly large per-declaration overhead. A meaningful
benchmark should add separate cases for repeated identical types, unique field
names, wide records, deep recursive records, large unions, generic
applications with repeated arguments, and concurrent interning.

Record node count, unique symbol count, serialized bytes, elapsed construction
time, and maximum RSS. Compare those measurements with a Vec-plus-HashMap
baseline before selecting a production structure.

### Initial lab result

The first implementation uses la-arena for declarations, lasso for names, and
HashMap interning for type nodes. An in-process global allocator recorded:

| Declarations | Type nodes | Interned names | Peak allocated bytes |
|---:|---:|---:|---:|
| 100,000 | 200,002 | 100,003 | 39,506,871 |
| 1,000,000 | 2,000,002 | 1,000,003 | 517,421,260 |

The one-million declaration run is a memory warning. The lab shape is
deliberately allocation-heavy and includes HashMap capacity, arena storage,
metadata, and allocator bookkeeping. The result does not establish that
la-arena or lasso are unsuitable. It establishes that the complete naive
combination needs a memory comparison against typed vectors, compact IDs,
interned repeated field names, and a separate structural interner before being
used at repository scale.

## Recon lab

The standalone experiment is at labs/type-system-rust/README.md. It imports no
sprefa code and is outside the workspace member list.

It demonstrates:

- ena type-variable unification
- la-arena declaration storage
- lasso symbol interning
- serde_json input and graph output
- miette diagnostics
- records, arrays, maps, optionals, unions, and generic application
- recursive dotted-path enumeration with array and map wildcards

## Sources

- https://docs.rs/rowan/latest/rowan/
- https://docs.rs/chumsky/latest/chumsky/
- https://docs.rs/logos/latest/logos/
- https://docs.rs/ena/latest/ena/unify/
- https://docs.rs/la-arena/latest/la_arena/
- https://docs.rs/indexed_arena/latest/indexed_arena/struct.Idx.html
- https://docs.rs/lasso/latest/lasso/
- https://salsa-rs.github.io/salsa/
- https://docs.rs/datafrog/latest/x86_64-unknown-linux-gnu/datafrog/index.html
- https://docs.rs/egg/latest/index.html
- https://docs.rs/miette/latest/miette/
- https://docs.rs/quote
- https://docs.rs/prettyplease/latest/prettyplease/
