# Generic and interface semantics over the DL6 type IR

> Historical plan. `issues/remove-rel-is/item.md` removed relation conformance
> suffixes and implementation rows on 2026-08-23. Declaration, parameter,
> member, constraint, application, argument, and derivation rows remain.

## Context

DL6 currently has four representations that must remain separate:

| Layer | Existing representation | Owner |
|---|---|---|
| Source declarations | `rel_template/3`, `col_type/3`, `type_decl/2`, `rel_is_implementation/2`, `keyed/2`, `kind/2`, `keep/2` | `v6/prolog/compile/parse_dl_dcg.pl:470-531` |
| Type plane | primitive atoms, named relation records, `option/1`, `json_list/1`, relational list constructors | `v6/prolog/0_type_plane.pl:64-151` |
| Storage plan | `rel(Ref, Kind, Columns, KeyOrNone)` plus resolved column types such as `ref(Name)` | `v6/prolog/lower.pl:1379-1643` |
| Introspection catalog | `row/11`; primitive, `json_list`, relation, and column rows connected by `type_id` | `v6/prolog/lower.pl:1379-1660` |

The parser already produces `rel_template(Segments, Parameters, Specs)` for a
curried generic relation declaration and `rel_is_implementation(Ref,
Applications)` for an `is` clause. Generic applications in ordinary column
type position were added on 2026-08-13. The generic expander already contains
the fixed-point, deterministic naming, and artifact lowering used by the
relational list families.

The first implementation of user template semantics directly converts every
ground application into `type_decl/2` and `col_type/3` rows in
`v6/prolog/0_generic_expand.pl`. Treat that implementation as a prototype until
the source type IR and catalog origin rows below are pinned. It demonstrates
that the existing storage planner can consume instantiated relation records.

`keyed/2`, `kind/2`, and `keep/2` are relation declarations consumed by the
checker and storage planner. Generic types do not require changing their
surface or their runtime semantics. Any later redesign of those declarations
belongs to a separate plan.

## Type signatures

The semantic type IR should distinguish declarations, applications,
parameters, constraints, and concrete instances before producing storage
relations.

```prolog
% parse_type_ir(+SurfaceDecls, -TypeDecls)
%
% TypeDecl = generic_rel(GenericId, QualifiedName, Parameters, Columns)
%          | interface(InterfaceId, QualifiedName, Parameters, Requirements)
%          | implementation(ImplementationId, Subject, InterfaceApplication)
%
% Parameter = type_parameter(ParameterId, Name, Constraints)
% Constraint = interface_application(InterfaceId, Arguments)
% Type = primitive(Name)
%      | named(TypeId)
%      | parameter(ParameterId)
%      | application(GenericId, Arguments)
%      | option(Type)
%      | json_list(Type)
%
% instantiate_type_ir(+TypeDecls, +Applications,
%                     -ConcreteTypes, -InstantiationEdges)
%
% ConcreteType = concrete_type(ConcreteId, GenericId, Arguments, Columns)
% InstantiationEdge = instantiation(ConcreteId, GenericId, Arguments)
%
% check_interface_bounds(+TypeDecls, +ConcreteTypes, -Findings)
%
% lower_concrete_types(+ConcreteTypes, +SurfaceDecls, -ExpandedDecls)
```

The source spelling for one bounded parameter can remain compact:

```dl6
rel pair(T: json_encodable)(first: T, second: T).
```

This parses to a parameter named `T` with one interface constraint. The
interface application receives `T` implicitly as its subject. Parameterized
interfaces can still accept additional explicit arguments later.

The first interface surface is a marker declaration, `interface name.`. A
bound receives an implicit subject only. Parameterized interface declarations
parse and carry arity for later use, while bounds with explicit interface
arguments remain outside this slice.

## Instance timelines

### Compiler instance

1. Module resolution produces one ordered surface program.
2. The type-IR pass allocates identities for generic declarations, parameters,
   interfaces, and implementations.
3. Application discovery walks column types and generated concrete columns.
4. Each distinct ground application creates one `concrete_type` during a
   fixed-point computation.
5. Bound checking runs against the concrete argument types.
6. Concrete types lower to the existing `type_decl/2` and `col_type/3` rows.
7. Existing option, enum, relation-reference, checker, and storage phases run.
8. The catalog records source declarations, applications, and concrete
   instances alongside the existing runtime relation rows.

The type-IR records live for the full compiler invocation. Concrete storage
relations live in the emitted program. No runtime polymorphic value or runtime
type argument is introduced.

### Target emitter instance

An emitter reads both source type IR and concrete instances. Its selected
policy determines output:

```text
preserve_generics -> source generic declaration plus constraints
monomorphize      -> concrete instances only
hybrid            -> generic declaration plus concrete aliases or wrappers
```

SQLite and the current runtime consume the monomorphized storage plan. Rust,
TypeScript, and Go type emitters may preserve generics because their output is
declaration code rather than the SQLite storage layout.

## Storage, reads, writes, and uniqueness

### Compiler storage

The type IR is an in-memory Prolog term graph during compilation. Identity is
allocated from qualified declaration name plus module identity. Parameter
references use parameter identities instead of atoms, preventing an ordinary
named type from colliding with a type variable.

Application discovery reads:

1. author column types;
2. generic template column types after substitution;
3. wrapper element types;
4. interface applications attached to declarations and parameters.

It writes one concrete instance for each unique pair:

```text
(generic declaration identity, ordered canonical argument types)
```

Canonical structural encoding remains the input to the generated-name hash.
The generated name is a storage symbol, while `ConcreteId`, `GenericId`, and
ordered argument edges preserve semantic identity.

### Runtime storage

Generic declarations and interfaces create no data tables by themselves.
Concrete relation instances lower through the current named-relation path and
therefore receive the existing table, dictionary, reference, delta, frontier,
and catalog behavior. Interface conformance metadata may enter the catalog
without becoming a user relation table.

### Catalog representation

The current `row/11` catalog can express nodes and edges using `kind`,
`parent_id`, `type_id`, and `ordinal`. New row kinds should encode:

| Kind | Parent | `type_id` | Ordinal |
|---|---|---|---|
| `generic_rel` | declaring module | 0 | 0 |
| `type_parameter` | generic or interface row | 0 | parameter position |
| `interface` | declaring module | 0 | 0 |
| `constraint` | parameter or implementation row | interface row id | constraint position |
| `application` | consuming declaration or column | generic row id | 0 |
| `type_argument` | application row | argument type id | argument position |
| `concrete_type` | generated relation row | generic row id | 0 |
| `implementation` | subject type row | interface row id | 0 |

This uses the existing graph-shaped catalog instead of introducing a second
catalog table. The allocation pass must assign relation and synthetic type IDs
before rows whose `type_id` references them. `catalog_list_rows/5` currently
resolves only primitive and nested-list element IDs, so relation-valued JSON
elements and generic arguments require the same precomputed type-ID map.

Generic and interface metadata extends `__rel` with graph row kinds. Programs
without this metadata retain their existing row IDs.

## Interface semantics

The first interface slice should describe a compile-time capability with no
methods and no runtime dispatch. `json_encodable` is the first receipt:

```text
primitive JSON types implement json_encodable
option(T) implements json_encodable when T does
json_list(T) implements json_encodable when T does
a named record implements json_encodable when every column type does
an enum implements json_encodable when every variant payload does
```

These rules operate over the semantic type IR. SQLite storage classification
does not decide conformance. This permits Rust Serde, TypeScript JSON values,
JSON Schema, and a future non-SQLite runtime to consume one capability result.

Later interfaces with requirements need a separate requirement vocabulary.
They should not be inferred from Rust traits, TypeScript interfaces, or Go
method sets during this arc.

Marker interface declaration records, implementation records, name and arity
checking, duplicate checks, and capability closure are implemented. Interface
members remain deferred.

## Sequence

1. Freeze the semantic IR terms and catalog row mapping with direct Prolog
   tests. Do not change relation modifiers.
2. Refactor the user-template prototype so parsing produces parameter
   identities and applications before monomorphization.
3. Preserve template and argument origin records while retaining the current
   concrete `type_decl/2` and `col_type/3` storage boundary.
4. Add interface declarations and implementation facts without requirements.
5. Implement `json_encodable` closure over primitives, wrappers, records, and
   enums.
6. Add bounded parameters and check them at each concrete instantiation.
7. Expose source generics, concrete instances, and conformance through catalog
   rows.
8. Add emitter policy selection for preserved, monomorphized, and hybrid type
   output.

The user-template path now emits normalized `generic_rel`, `interface`, and
`implementation` semantic records before concrete lowering.

<!-- todo(feature): Add emitter policy tests proving that one catalog can render preserved generics and concrete monomorphizations. -->

## Decisions

- Generic declarations remain compile-time schema declarations.
- SQLite and the current runtime receive concrete relation instances.
- Source generic identity and concrete storage identity are both retained.
- Bounds attach to type parameters and use an implicit subject.
- Interface checking runs on the semantic type IR, independently of SQLite.
- `keyed/2`, `kind/2`, and `keep/2` remain outside this arc.
- Existing `type_decl/2`, `col_type/3`, and storage `rel(...)` records remain
  the lowering target until the semantic IR has executable parity tests.
- Runtime polymorphism, higher-kinded types, generic rules, default type
  arguments, variadic type arguments, and interface methods are deferred.

## Verification

Parser and printer receipts:

- bounded parameter round trip;
- nested generic application round trip;
- parameter identity does not collide with a named type;
- both parser doors produce equivalent semantic IR.

Instantiation receipts:

- one application produces one concrete instance;
- repeated equal applications reuse it;
- different ordered arguments remain distinct;
- nested applications close to a fixed point;
- recursive instantiation cycles and non-ground applications are named;
- wrong generic and interface arities are named;
- generated storage names remain deterministic under declaration-block
  permutation.

Interface receipts:

- primitive, wrapper, record, and enum closure for `json_encodable`;
- one non-encodable leaf names the complete type path;
- absent, duplicate, and conflicting implementations are named;
- a failed bound prevents storage lowering.

Catalog receipts:

- application argument order is represented by ordinal;
- concrete instance links to generic declaration and generated relation;
- module ownership survives imports and mounts;
- existing primitive, list, relation, and column IDs remain unchanged for a
  program without generics or interfaces.

Emitter receipts:

- the same catalog emits preserved and monomorphized TypeScript, Rust, and Go;
- JSON Schema consumes concrete applications and interface-derived
  encodability;
- generated artifacts compile with the target compilers.

Run focused Prolog suites after each stage, then the complete compiler test
file. Record unrelated baseline failures separately rather than changing
their expectations in this arc.

## Staffing

- Implementation: one agent lane after human review of this plan and the
  semantic IR signatures.
- Worktree: yes, because parser, expansion, type plane, catalog, printer, and
  compiler tests overlap.
- Base SHA observed while planning: `2bf60561`.
- Suite budget: focused parser/type-IR/catalog suites per commit; complete
  `plunit_tests.pl` before handoff; target-language compiler checks only after
  emitter policy lands.
