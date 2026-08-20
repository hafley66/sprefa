# Applicative type annotations

## Context

DL6 already has type-valued compiler relations, a single `return` column from
`-> type`, keyword arguments, runtime list syntax, semantic type identities,
and key-wrapper normalization. Compiler relations returning `type` are callable
directly in type position.

The first supported form is:

```dl6
rel key(Target: type) -> type.
rel min(Target: type, Value: int) -> type.

rel Revision(
  id: key(int),
  age: min(int, Value: 1)
).
```

The annotation site is `Revision.id`; its stored type is `int`; `key(int)` is a
compiler relation application. Composition uses ordinary nesting.

## Decisions

- A compiler relation with a first `type` input and final `return: type` is callable wherever a type expression is accepted.
- The input type is explicit. `B(A(T), x)` composes `A` and `B` inside-out.
- A relation whose `return` column has type `type` is evaluated at compile time for this arc. Its ordinary arguments are therefore required to be compile-time-known values without additional surface syntax.
- Keyword argument labels resolve against relation columns independent of capitalization.
- Each step must produce exactly one type result. Zero and multiple results receive named compiler diagnostics.
- The final stored type is the final composition result.
- Every successful step also retains site evidence containing the member identity, input type, application, and output type.
- `key(Target) -> Target` is the first site-effect consumer. Existing key normalization consumes its evidence and emits existing positional key IR.
- Annotation relations remain compiler-plane relations and do not become SQLite runtime tables.
- The removed `@(Type, [...])` surface receives `annotation_surface_removed`.

Rejected for this arc:

- Python-style `Annotated[T, ...]`: redundant long-form spelling.
- Decorator stacks: ordering becomes visually distributed.
- Whitespace application: conflicts with the requested application grammar.
- A second bespoke annotation evaluator: compiler relations already provide fixpoint evaluation.
- General runtime annotation reflection: retained compiler/catalog metadata is sufficient for this slice.
- Explicit per-parameter `comptime(T)` staging and stage-elision inference: deferred until relations need compile-time configuration mixed with runtime rows.

## Type signatures

```text
parse_type_relation_application(+Tokens, -Application)

elaborate_annotations(
  +OwnerMemberId,
  +InputTypeId,
  +Applications,
  -OutputTypeId,
  -EvidenceRows
)

apply_annotation(
  +CompilerPlane,
  +SiteId,
  +InputTypeId,
  +Application,
  -OutputTypeId,
  -EvidenceRow
)

normalize_key_evidence(
  +Declarations,
  +EvidenceRows,
  -KeyedDeclarations
)
```

Pseudo-code:

```text
application = resolve nested compiler relation application
require first input type and final return:type
evaluate inner application before its outer consumer
require exactly one returned type per call
retain annotation(site, input, application, returned)
rewrite column type to the outer returned type
consume recognized site evidence in later normalization phases
```

## Instance timeline

1. Parse retains the ordinary nested type application without evaluating it.
2. Module and generic resolution establish the owning member identity and concrete input type.
3. Compiler-relation partitioning resolves each annotator signature.
4. Compiler-relation evaluation applies the ordered composition and retains evidence.
5. Schema wrappers such as `key` consume recognized evidence.
6. Existing option, enum, relation-value, catalog, SQL, TS, and Rust phases receive the normalized type and keyed declaration IR.
7. Compiler-plane declarations, rules, and proof rows remain absent from runtime relations and boot facts.

## Storage and uniqueness

Annotation applications do not add SQLite tables. Evidence is compiler metadata
keyed by annotation site plus sequence position. Two identical applications at
different positions remain distinguishable. The `key` consumer produces the
existing relation key positions, so SQLite uniqueness and replacement use the
current lowering.

Compiler evaluation reads the resolved annotator declaration and its closure,
writes one evidence row per application step, and refuses a step unless its
result cardinality is exactly one.

For the initial evaluator, `return: type` selects compile-time evaluation for
the complete invocation. `Target: type`, `Value: int`, and other columns remain
ordinary typed relation columns; their invocation values must all be known at
the annotation site.

## Sequence

```text
source type expression
  -> ordinary type-application AST
  -> module/generic resolution
  -> ordered compiler-relation applications
  -> annotation evidence rows
  -> key evidence normalization
  -> existing type/storage lowering
  -> TS/Rust/JSON Schema artifacts
```

## Verification

- Parse, print, reparse equality for plain, direct, configured, and nested applications.
- Tree-sitter CST covers the same forms.
- Capitalized declared argument labels and lowercase call labels resolve to the same column.
- Composition proves that the output of step N is the `Target` of step N+1.
- Named diagnostics cover unknown annotator, wrong input signature, wrong keyword, wrong keyword type, zero results, multiple results, and non-type return.
- Plain `int` requires no empty annotation carrier.
- `key(int)` produces the existing SQL key/upsert behavior.
- Compiler annotation relations and evidence produce no runtime table, boot row, or DD relation.
- Golden DL6 compiles and executes through Prolog, TS + SQLite, and Rust + SQLite.
- Generated TS, Rust, JSON Schema, and ProgramJson retain the intended stored type and site metadata.

## Staffing

- Surface and elaboration: Flash4 in an isolated worktree.
- Compiler execution and key bridge: Terra in an isolated worktree after the surface contract lands.
- Cross-target CI and review corrections: Flash4 in an isolated worktree after implementation.
- Base SHA at plan creation: `50eb5f919fcd3242ac8c578e5c4167f09fff6e30`.
- CI budget: focused parser/compiler suites during implementation; one cross-target authored golden at closeout.

<!-- todo(feature): Parse and print direct compiler-relation type applications, update the CST, and elaborate nested calls. -->
<!-- todo(feature): Execute typed annotator relations, retain site evidence, and feed key evidence into existing key normalization. -->
<!-- todo(feature): Prove annotation composition and key parity through the authored DL6, TS, Rust, JSON Schema, and ProgramJson CI paths. -->
<!-- todo(feature): Define explicit mixed-stage relation parameters and comptime-elision rules after annotation composition establishes the first compile-time invocation model. -->
