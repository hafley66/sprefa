# Relational Interfaces and Result Arrows

> Superseded on 2026-08-23 by `issues/remove-rel-is/item.md`. The relation
> conformance suffix and implementation-row model described below were removed.
> Interface declarations, generic bounds, result arrows, and compiler relation
> annotations remain.

## Context

DL6 currently parses interfaces as `interface_decl/2` and implementations as
`rel_is_implementation/2` in `v6/prolog/compile/parse_dl_dcg.pl:534-547`.
Generic expansion converts those declarations into dedicated constraint and
implementation rows in `v6/prolog/0_generic_expand.pl:256-340`, then judges a
ground specialization against them.

Parameterized relation enums now parse as `rel_template_enum/3` at
`v6/prolog/compile/parse_dl_dcg.pl:481` and specialize into concrete enums.
This makes an authored `Result(Error, Value)` available as the return value of
a relation-shaped function surface.

The existing `sh` arrow at `v6/prolog/compile/parse_dl_dcg.pl:805` describes a
host demand/response boundary. The proposed relation arrow describes an
ordinary relation column split. It introduces no host execution or effect.

## Decisions

### Interfaces lower to compile-time relations

Surface:

```dl6
interface JsonEncodable(T).
rel Envelope(T: JsonEncodable)(value: T).
Document is JsonEncodable.
```

Conceptual lowering:

```dl6
rel $type.JsonEncodable(type: $type.Type).
$type.JsonEncodable(Document).
$type.EnvelopeAllowed(T) <- $type.JsonEncodable(T).
```

`$type.Type` contains canonical compiler type terms such as `text`,
`list(text)`, `Result(Error, Value)`, `module.Relation`, and
`module.Relation.id`. These rows exist during compilation and do not enter the
runtime SQLite or Differential Dataflow planes.

The existing `interface`, bound, and `is` spellings remain accepted sugar.
Their single judge becomes a compile-time relation query. Interface inheritance
and derived conformance become ordinary compile-time rules.

### Relation arrows append one named `return` column

Declaration:

```dl6
rel Parse(source: text) -> Result(ParseError, Ast).
```

Canonical lowering:

```dl6
rel Parse(source: text, return: Result(ParseError, Ast)).
```

Rules retain the ordinary relation-head form:

```dl6
Parse(Source, ResultValue) <- Body.
```

The output column is named `return`. `return` remains an ordinary identifier in
DL6 rather than a reserved word. The TypeScript emitter writes `return`
directly; the Rust emitter applies its existing keyword escaping and writes
`r#return`.

Its type may be scalar, product relation,
enum, generic specialization, wrapper, relation value, or relation identity.
The first slice permits exactly one arrow result. Multiple result values use a
declared product or sum type.

The arrow adds no call identity, directionality, uniqueness, evaluation order,
or effect semantics. Those remain properties of relation kind, keys, rules,
and host declarations.

### Type signatures

```text
lowerInterface : InterfaceDecl -> CompileRelDecl<TypeTerm...>
lowerImpl      : IsDecl        -> CompileFact
proveBound     : CompileDb × InterfaceApp -> Result<Proof, UnsatisfiedBound>

lowerArrowDecl : ArrowRelDecl<InputCols, OutputType>
              -> RelDecl<InputCols + [return: OutputType]>
```

### Instance timeline and lifetime

1. Parse interface, implementation, generic, generic-enum, and arrow forms.
2. Lower interface declarations and implementations into the compile-time
   relation plane.
3. Lower arrow declarations into ordinary relation declarations with a final
   `return` column. Rule heads already use the canonical ordinary form.
4. Discover ground generic applications.
5. Query compile-time interface relations for every bound application.
6. Mint each accepted concrete product or enum once by canonical type term.
7. Erase compile-time-only rows before runtime lowering.

Proof rows live for one compiler invocation. Concrete specialized type identity
remains deterministic across invocations through the existing canonical type
name and type-ID machinery.

### Storage, reads, writes, and uniqueness

Interface evaluation reads and writes an in-memory compile-time relation set.
It creates no SQLite DDL, boot facts, host plans, or runtime arrivals.

Arrow lowering writes only the existing declaration/rule IR. Runtime storage is
the same table that the equivalent explicitly appended `result` column would
produce.

One canonical interface application may have one proof row after set
deduplication. Multiple derivations of the same proof are equivalent. Conflicting
associated outputs remain outside this slice because interfaces have no
associated-output syntax yet.

One arrow declaration contributes exactly one final column named `return`.
An authored input column named `return` is a compile-time collision refusal.

## Rejected alternatives

- Runtime interface tables: mixes compiler proof rows with application data.
- Method-bearing interfaces: requires invocation and dispatch semantics outside this slice.
- Arrow-generated demand/response pairs: duplicates the existing `sh` host boundary.
- Arrow syntax in rule heads: combines `->` and `<-` in one statement and saves one argument spelling.
- Anonymous tuple returns: removes field names from generated Rust, TypeScript, and JSON Schema.
- Implicit multi-result flattening: recreates the flattening ambiguity that relation values avoid.

## Sequence

1. Replace the dedicated interface judge with a compile-time relational adapter
   while preserving current source syntax and diagnostics.
2. Add declaration arrow parsing, canonical lowering, printing, and
   explicit-form equivalence tests.
3. Put `Result(E, T)` arrow examples in the comprehensive language golden and
   generated TS/Rust/JSON Schema CI.

## Verification

- Parse/print/parse fixes both new surfaces.
- Existing interface programs emit byte-identical runtime programs.
- Derived compile-time conformance proves a bound through at least one rule.
- Missing conformance retains the named unsatisfied-bound diagnostic.
- Explicit and arrow relation forms emit identical relational IR, SQLite DDL,
  TypeScript plans, Rust `ProgramJson`, and query results.
- `Result(Error, Value)` emits discriminated TypeScript, Rust enum, and JSON
  Schema union artifacts through the existing type-generation CI.
- CI additions are compiler, emitted-runtime, and cross-target type-generation
  tests. No formatter or linter result is part of acceptance.

## Staffing

Two isolated implementation lanes may run independently after the plan commit:

- compile-time relational interfaces: Terra-class task, worktree required;
- relation arrow and `Result` use: Luna-class task, worktree required.

Base SHA: `b3be657e0`. Each lane runs its focused compiler tests first. The merge
lane runs existing Prolog compiler CI and TypeScript/Rust type-generation CI.
