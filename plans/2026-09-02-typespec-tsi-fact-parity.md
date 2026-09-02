# TypeSpec 1.15 and TSI fact parity

## Context

This document compares TypeSpec's current language and compiler model with the
Type Semantics Index target described in
`.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md`.
The comparison unit is one idea or fact. Each row asks whether the same fact can
be written in TypeSpec source, recovered from the TypeSpec compiler, stored by a
TypeSpec library, and emitted into TSI without inference loss.

The checked upstream baseline is TypeSpec 1.15.0. GitHub identifies
`typespec-stable@1.15.0` as the latest stable compiler release available on
2026-09-02. This version adds stage-aware `Program.currentStage` and
`Program.useCache`, and it adds programmatic application of auto decorators.
The release receipt is the official
[TypeSpec 1.15.0 release note](https://typespec.io/release-notes/typespec-1-15-0/)
and the
[Microsoft TypeSpec release feed](https://github.com/microsoft/typespec/releases).

Earlier repository research covered TypeSpec 1.15 module placement, visibility,
generics, and emitter mechanics in
`plans/2026-08-12-typespec-module-ir.RESEARCH.md`. This document reuses those
receipts and changes the axis: the rows below follow TSI's fact vocabulary and
the programmable DL7 comptime target.

### Comparison layers

TypeSpec exposes four distinct places where an idea may exist:

```text
.tsp source
    |
    v
compiler semantic objects: Model, Union, Operation, TypeMapper, ...
    |
    +--> decorator state: Program.stateMap/stateSet or auto dec records
    |
    v
emitter or semantic adapter
    |
    v
TSI facts
```

The parity labels are:

| Label | Meaning |
|---|---|
| `source` | TypeSpec has direct source syntax and the compiler retains the meaning. |
| `semantic` | The compiler exposes the fact, while ordinary `.tsp` declarations do not state it as a first-class relation. |
| `auto metadata` | An `auto dec` declaration and application can store the fact without JavaScript implementation code. |
| `JS library` | A decorator, function, mutator, checker call, or emitter written in JavaScript must calculate or store the fact. |
| `derived` | A TSI adapter can derive the fact deterministically from exposed TypeSpec semantic objects. |
| `extension` | TSI must retain the fact in a TypeSpec-namespaced relation because the common TypeSpec graph has no matching concept. |
| `absent` | TypeSpec 1.15 exposes no corresponding language or compiler fact. |

`source`, `semantic`, and `auto metadata` can coexist in one row. The most
important boundary is whether the same declaration can be expressed in `.tsp`
alone or requires JavaScript.

## Core type graph parity

TypeSpec's compiler exposes a discriminated semantic graph containing models,
model properties, unions, union variants, enums, enum members, scalars,
operations, interfaces, namespaces, template parameters, literal types, and
values. `navigateProgram` and `navigateType` traverse that graph. The official
[emitter guide](https://typespec.io/docs/extending-typespec/emitters-basics/)
documents semantic walking, and the
[TypeSpec JS API index](https://typespec.io/docs/standard-library/reference/js-api/)
lists the semantic object families.

| TSI fact or idea | TypeSpec source | Compiler or library representation | Parity | Adapter rule or retained difference |
|---|---|---|---|---|
| `tsi.type(Type)` | Every model, union, enum, scalar, interface, operation, member, literal type, and anonymous type expression | Every semantic type has `entityKind: "Type"` and a discriminating `kind` | `semantic`, `derived` | Emit one `tsi.type` for every reachable `Type`. Values remain separate entities. |
| `tsi.denotes(Symbol, Type)` | A declaration binds a name in a namespace or lexical template scope | Declaration node, namespace membership, and semantic type carry the binding | `source`, `derived` | Use SCIP symbol identity for `Symbol`; map the declaration's semantic type to `Type`. |
| `tsi.has_type(Occurrence, Type)` | Type references and values occur in typed contexts | `checker.getTypeForNode` and `checker.getValueExactType` expose the resolved type | `semantic`, `derived` | Emit for each indexed source occurrence. This uses the advanced checker API, whose stability guarantee is narrower than the public typekit API. |
| `tsi.origin(Type, Language, Range)` | Source declarations and expressions have locations | Semantic types usually retain `node`; `getSourceLocation` returns a source range | `semantic`, `derived` | Dynamically built types may have no node. Give those a synthetic origin carrying library, compiler version, and producer. |
| `tsi.product(Type)` | Named or anonymous `model`, model expression, intersection result, tuple, operation parameters | `Model.properties`, `Tuple.values`, `Operation.parameters` | `source`, `derived` | Preserve subtype rows for `extends` and source-copy rows for `is`, spread, and intersections. |
| `tsi.sum(Type)` | Named `union`, union expression, and `enum` | `Union.variants`, `Enum.members` | `source`, `derived` | Preserve `tsp.union`, `tsp.enum`, and named versus expression facts. An enum member carries an optional scalar value; a union variant carries a type. |
| `tsi.callable(Type)` | `op`, experimental `fn`, decorator declarations, and generic declarations each have callable shape | `Operation`, function/decorator definitions, template parameter lists | `source`, `derived` | Emit a callable category fact so service operations, compile-time functions, decorators, and type constructors stay distinguishable. |
| `tsi.primitive(Type, Class)` | Built-in and user-defined `scalar` declarations, literal types, intrinsic types | `Scalar`, literal types, `IntrinsicType`, scalar base chain | `source`, `derived` | Emit the portable primitive class plus `tsp.scalar` and scalar-base facts. TypeSpec has widths such as `int8` through `int64`, arbitrary `integer`, decimal classes, date/time types, bytes, `unknown`, `never`, `void`, and `null`. |
| `tsi.edge(Edge, Owner, Label, Target, Position)` | Model property, union variant, enum member, interface operation, namespace declaration, operation parameter | `ModelProperty`, `UnionVariant`, `EnumMember`, `Operation`, namespace maps; ordered collections retain source order | `source`, `derived` | Mint one edge ID from owner, edge category, label, and position. Keep category facts because TypeSpec has several member kinds rather than one universal source-level edge declaration. |
| anonymous structural identity | Model and union expressions have no declaration name | Semantic object identity exists for one compilation; `node` identifies source construction | `semantic`, `derived` | TSI canonicalizes the closed ordered edge graph. TypeSpec object identity alone is process-local. Recursive graphs require provisional IDs followed by closure. |
| nominal identity | Named declarations and scalar/model inheritance retain names and source symbols | `name`, `namespace`, declaration node, `baseModel`, scalar base | `source`, `derived` | Use resolved declaration symbol identity. TypeSpec assignability for models remains structural even when a model has a name. |
| alias identity | `alias Name = Expression` | Alias resolves to its target and has no distinct type-graph node | `source` only | Emit the alias symbol and `denotes` relation from syntax/SCIP data. The official [alias documentation](https://typespec.io/docs/language-basics/alias/) explicitly states that aliases have no representation in the type graph. |
| namespace and scope | `namespace`, file namespace, nested namespace, `using`, imports | Merged `Namespace` objects and source symbols | `source`, `semantic`, `derived` | Emit lexical declaration edges, import edges, and namespace containment. `using` introduces local bindings and does not add members to the target namespace. See [namespaces](https://typespec.io/docs/language-basics/namespaces/). |

### Product, sum, and edge details

| Same idea | TypeSpec spelling | TSI projection | Remaining fact |
|---|---|---|---|
| Ordered record | `model User { id: int64; name: string; }` | `product(User)` plus two ordered `edge` rows | `tsp.model(User)` |
| Anonymous record | `{ id: int64; name: string; }` | Structural product ID plus two ordered edges | Source-expression origin |
| Closed alternatives | `union Result { ok: User, error: Error }` | `sum(Result)` plus two variant edges | `tsp.union(Result)` and variant names |
| Literal enumeration | `enum State { open, closed }` | `sum(State)` plus member edges to literal/value types | `tsp.enum(State)` and explicit representation values |
| Intersection | `Animal & Pet` | Product containing the effective edges | `tsp.intersection(Result, Animal, Pet)` and `Model.sourceModels` provenance |
| Copy with metadata | `model X is Y` | Product edges copied from `Y` | `tsp.model_is(X, Y)` and copied decorators |
| Inheritance | `model X extends Y` | Product edges plus `tsi.subtype(X, Y, Witness)` | `tsp.model_extends(X, Y)` |
| Spread | `model X { ...Y; z: string; }` | Product edges inserted at the source position | `tsp.spread(X, Y, Position)` |

TypeSpec's model API records `baseModel`, `sourceModel`, `sourceModels`, ordered
properties, template metadata, and whether a type has finished. These fields
support lossless distinction among inheritance, `is`, spread, and intersection
while still deriving one effective TSI product. See the official
[`Model` interface](https://typespec.io/docs/standard-library/reference/js-api/interfaces/model/).

## Generics, calls, and callable types

TypeSpec calls generics “templates.” Aliases, models, operations, interfaces,
and scalars may declare template parameters. Parameters support constraints,
defaults, named arguments, type arguments, and `valueof` arguments. Arguments
are evaluated in declaration order. See the official
[template documentation](https://typespec.io/docs/language-basics/templates/).

| TSI fact or idea | TypeSpec source | Compiler representation | Parity | Adapter rule or retained difference |
|---|---|---|---|---|
| `tsi.parameter(Parameter, Constructor, Position, Variance)` | `model Page<Item = string>` | `TemplateParameter`, template declaration node, constraint, default | `source`, `derived` | Emit declaration ordinal, constraint, and default. TypeSpec supplies no variance annotation, so emit `tsp.variance(Parameter, unspecified)` rather than inventing invariance. |
| generic type constructor | `model Page<Item> { item: Item; }` | Templated semantic type with template node and parameter list | `source`, `derived` | Mark the declaration callable from type/value arguments to a type result. |
| `tsi.called(Result, Callee, ArgumentList)` | `Page<User>` | Template instance carries `templateMapper`, `templateNode`, and `instantiationParameters`; compiler caches instantiations | `semantic`, `derived` | Resolve the declaration callee, emit ordered actual arguments, then canonicalize result identity from callee and arguments. |
| `tsi.argument(List, Position, Argument)` | `Test<V = "x", T = User>` | The checker evaluates named arguments in declaration order | `source`, `semantic`, `derived` | Normalize names to declaration positions. Preserve source-written name and order in `tsp.argument_syntax`. |
| type argument | `Page<User>` | `Type` argument | `source`, `semantic` | Emit `tsi.argument` with a type ID. |
| value argument | `Format<"json">` with `valueof` constraint | `Value` argument and exact value type | `source`, `semantic` | TSI's current `ArgumentTypeId` field is too narrow. Add an entity argument relation or box values as value nodes before claiming complete TypeSpec template coverage. |
| default argument | `Page<Item = string>` and omission of a trailing argument at use | Template parameter default; evaluated during instantiation | `source`, `semantic` | Emit supplied/defaulted provenance per slot. The closed `called` identity uses effective arguments. |
| named argument | `Test<V = "x", T = User>` | Resolved against public parameter names | `source`, `semantic` | Emit source label plus resolved position. |
| constrained argument | `T extends { id: string }` | Assignability check against constraint | `source`, `semantic` | Emit constraint edge and an assignability witness for accepted instantiations. |
| operation inputs | `op get(id: string): User` | `Operation.parameters` is a model | `source`, `derived` | Emit `tsi.input` rows from ordered parameter properties. Preserve parameter model identity. |
| operation outputs | operation return type | `Operation.returnType` | `source`, `derived` | Emit one output slot whose target may itself be a product or sum. |
| multiple logical outcomes | `User | Error` return type | One return type pointing to a union | `source`, `derived` | TSI represents one output edge to the sum. Runtime cardinality remains a separate callable fact. |
| operation reuse | `op readPet is ReadResource<Pet>` | Operation source and instantiated template relationship | `source`, `semantic` | Emit `called` for `ReadResource<Pet>` and an operation-copy relation for `readPet`. |
| partially open instance | Templated operations inside an incompletely instantiated templated interface | `isFinished = false`; compiler model documents partial template instances | `semantic` | TypeSpec can contain unresolved template parameters during compilation. This does not expose arbitrary source-level partial application. |
| arbitrary partial application | Supply any proper subset of slots and receive a callable for the remainder | No general source form | `absent` | DL7/TSI requires canonical partial-call nodes and remaining `tsi.input` rows. TypeSpec named arguments only skip slots that already have defaults. |
| currying | Call one argument at a time and receive callable results | No general source form | `absent` | Represent in DL7 as repeated partial applications. A TypeSpec adapter only imports fully resolved template instances and compiler-internal partial states. |
| higher constructor parameter | Parameter accepts a generic constructor such as `Type -> Type` | Template constraints accept TypeSpec entities and values; no constructor-kind constraint or generic-constructor application variable appears in source | `absent` | Preserve DL7 higher-callable contracts as TSI facts. A TypeSpec library can simulate selected cases with an `extern fn` implemented in JavaScript. |
| compile-time function | `extern fn rename(m: Reflection.Model, ...): Reflection.Model` | Function signature in TypeSpec, implementation in JavaScript | `source`, `JS library` | Emit callable input/output facts. Function behavior remains an external implementation fact. Functions are experimental in 1.15. |

The TypeSpec compiler's `TypeInstantiationMap` maps ordered type/value argument
arrays to instantiated types. The official
[`TypeInstantiationMap` API](https://typespec.io/docs/standard-library/reference/js-api/interfaces/typeinstantiationmap/)
is direct evidence for the `called + argument` projection. TypeSpec semantic
objects also document unfinished template declarations and partially resolved
instances. The TSI adapter must avoid treating an unfinished object as a closed,
canonical result.

## Relations, conformance, and constraints

TypeSpec has structural model assignability, explicit model and scalar
inheritance, interface-to-interface operation composition, and operation
signature reuse. Its interfaces group operations. They do not declare a data
shape contract implemented by models. The relevant source receipts are
[type relations](https://typespec.io/docs/language-basics/type-relations/),
[models](https://typespec.io/docs/language-basics/models/), and
[interfaces](https://typespec.io/docs/language-basics/interfaces/).

| TSI relation | TypeSpec source or API | Parity | Witness emitted into TSI |
|---|---|---|---|
| `tsi.subtype(Source, Target, Witness)` | `model Source extends Target`, `scalar Source extends Target`, interface `extends` for operation groups | `source`, `derived` | Declaration symbol and source range. Retain subtype category because these three `extends` forms have different member behavior. |
| `tsi.assignable(Source, Target, Witness)` | Structural model and scalar hierarchy rules; typekit `isAssignableTo` | `semantic`, `derived` | Adapter invocation record containing compiler version, source, target, and diagnostics. TypeSpec returns a boolean plus optional diagnostics rather than a proof tree. |
| `tsi.conforms(Source, Contract, Witness)` | Structural assignability can witness model-shape satisfaction; interface composition only covers operations | `derived`, `extension` | Derive when a selected TSI contract maps to a TypeSpec type. Preserve `tsp.assignable` as the native relation. There is no Rust-style `impl` declaration. |
| `tsi.equivalent(Left, Right, Witness)` | Semantic object identity and effective structural comparison can answer selected cases | `semantic`, `derived` | State the equivalence policy in the witness. Alias resolution, `model is`, and structural equality are separate policies. |
| constraint on generic parameter | `T extends Constraint` | `source`, `semantic` | Parameter declaration and successful assignability check. |
| interface implementation | Interfaces contain operations and can extend other interfaces | `extension` | Emit `tsp.interface` and operation edges. A model-to-interface implementation row requires custom metadata or an adapter policy. |
| trait implementation with associated items | No native declaration | `absent` | Use language-native TSI rows from Rust/DL7 producers. |
| relation rule or query | No Datalog/Prolog rule body | `absent` | DL7 owns rule facts and fixpoint evaluation. TypeSpec can supply type facts consumed by those rules. |
| recursive transitive closure | No source-level recursive query | `absent` | Run in DL7 comptime after TypeSpec extraction. |
| exhaustiveness over sum | Unions and enums are closed semantic collections; TypeSpec has no value-level `match` expression | `derived` for membership, `absent` for match execution | Emit every reachable variant under complete semantic coverage. DL7's match checker can consume those rows. |

The typekit API exposes `entity.isAssignableTo` and `type.isAssignableTo`, with
diagnostic-producing variants. It also creates models, model properties,
unions, tuples, records, arrays, operations, values, and other synthetic types.
See the official [typekit API](https://typespec.io/docs/standard-library/reference/typekits/).

## Edge metadata and annotations

TypeSpec 1.14 introduced auto decorators, and 1.15 added programmatic application
through `setAutoDecorator`. An auto decorator declaration supplies a typed target
and typed arguments. The compiler stores an argument record in program state,
keyed by the decorator's fully qualified name and target type. The
[decorator implementation guide](https://typespec.io/docs/extending-typespec/create-decorators/)
documents the storage shape and generated readers.

Auto decorators use the `auto-decorators` experimental feature in 1.15. A
library may enable that feature through its own `tspconfig.yaml`. Functions and
the `internal` access modifier are also documented as experimental in 1.15.

This is the closest TypeSpec counterpart to a TSI relation that tags a node or
edge with typed options.

| Idea or fact | TypeSpec 1.15 expression | Parity | TSI extraction consequence |
|---|---|---|---|
| Node annotation | `auto dec history(target: Model, version: valueof int32);` then `@history(1) model Event {}` | `auto metadata` | Emit `tsp.decorator(Event, History, Args)` and optionally normalize to a shared `history` fact. |
| Edge annotation | `auto dec keyPart(target: ModelProperty, order: valueof int32);` then `@keyPart(0) id: int64;` | `auto metadata` | The `ModelProperty` maps to `EdgeId`, so all owner, label, target, and position data remain recoverable. |
| Flag annotation | `auto dec interned(target: Scalar);` | `auto metadata` | Emit a zero-argument annotation fact keyed by scalar type ID. |
| Typed options | Decorator parameters can be optional, rest, type-valued, or `valueof` values | `source`, `auto metadata` | Preserve parameter declaration, supplied values, omitted slots, and source application. |
| Apply annotation elsewhere | `@@tag(Dog, "sample");` | `source`, `semantic` | Emit application origin independently from target origin. |
| Built-in key | `@key` on a model property | `source`, `semantic` | Emit a key-role fact on the edge. Composite-key ordering or named key groups need custom metadata. |
| Optional edge | `name?: string` | `source`, `semantic` | Emit `tsp.optional(Edge)`. Keep absence separate from union-with-`null`. |
| Default value | `name?: string = "Rex"` | `source`, `semantic` | Emit a value node and `tsp.default(Edge, Value)`. |
| Read-only API view | `@visibility(Lifecycle.Read)` | `source`, `semantic` | Emit lifecycle visibility set. TypeSpec's OpenAPI helper calls a property read-only when only `Read` is active; this is view metadata rather than a general immutability guarantee. |
| Visibility class | Any enum can define a visibility class | `source`, `semantic` | Emit class, active enum-member set, and default set per property edge. |
| Access control | experimental `internal` modifier on supported declarations | `source`, `semantic` | Attach to symbol/binding identity. The official [access-modifier guide](https://typespec.io/docs/language-basics/access-modifiers/) defines it as a symbol property. |
| Validation facets | `@minLength`, `@maxValue`, `@pattern`, `@format`, `@encode`, and related decorators | `source`, `semantic` | Emit shared constraint rows where semantics match; preserve protocol-specific encoding rows. |
| Arbitrary metadata fact | Custom `auto dec` | `auto metadata` | Fully expressible for one target type plus a typed argument record. |
| Arbitrary N-ary relation among types | Auto decorator target plus arguments can encode a directed relation | `auto metadata`, `derived` | One participant must be the decorated target. Normalize the record into a TSI N-ary fact in the adapter. |
| Rule that derives more facts | Decorator callback, function, mutator, validator, or emitter in JavaScript | `JS library` | TypeSpec source states the signature/application; behavior lives in JS. There is no `.tsp` rule body or fixpoint relation. |
| Rule that creates types | JS implementation calls typekit `create`, `clone`, and `finishType` APIs | `JS library` | Emit synthetic type origin, transformation provenance, and generated edges. |
| Graph-finish validation | Decorator `onGraphFinish` callback | `JS library` | Can inspect the complete TypeSpec graph once. It remains imperative callback execution rather than a recursive relation closure. |

### `HistoryV1` comparison

The same user-authored declaration can carry the requested marker and options:

```typespec
namespace Sprefa;

auto dec historyV1(
  target: Model,
  sequence: valueof string,
  timestamp: valueof string,
);

@historyV1("version", "recordedAt")
model User {
  @key id: int64;
  name: string;
}
```

This source yields a typed metadata record attached to `User`. A TypeSpec
emitter can read the model, its keyed property, and the metadata record. The
`.tsp` program does not contain a rule body that creates `UserHistory`, adds
sequence/timestamp edges, derives rows, or specifies temporal storage behavior.
Those steps require a JavaScript mutator/emitter or a TSI/DL7 rule after
extraction.

Conceptual TSI projection:

```text
tsi.product(user)
tsi.edge(user_id, user, id, int64, 0)
tsi.edge(user_name, user, name, string, 1)
tsp.key(user_id)

tsp.decorator(user, sprefa_history_v1, history_args)
tsp.decorator_argument(history_args, sequence, "version")
tsp.decorator_argument(history_args, timestamp, "recordedAt")
```

DL7 comptime can then derive `HistoryV1(user, options, result)`, result edges,
layout facts, and temporal rules. TypeSpec auto decorators cover declaration and
storage of the marker. DL7 rules cover programmable closure over that marker.

## Type transformations

TypeSpec has built-in model transformations for optional properties, picked
properties, omitted properties, omitted defaults, visibility views, and update
views. They are exposed as standard templates such as `OptionalProperties`,
`PickProperties`, and `OmitProperties`, with decorator forms for mutating a
target model. See the official
[built-in data types](https://typespec.io/docs/standard-library/built-in-data-types/)
and
[built-in decorators](https://typespec.io/docs/standard-library/built-in-decorators/).

| Operation | TypeSpec expression | Parity with DL7 userland type rules |
|---|---|---|
| Partial | `OptionalProperties<Source>` or `@withOptionalProperties` | Same effective edge transformation. TypeSpec implementation belongs to the standard library/compiler. DL7 target allows an ordinary user rule over edge facts. |
| Pick | `PickProperties<Source, Keys>` or `@withPickedProperties` | Same effective selected edge set. TypeSpec `Keys` is a string/union input. |
| Omit | `OmitProperties<Source, Keys>` or `@withoutOmittedProperties` | Same effective excluded edge set. |
| Remove defaults | `OmitDefaults<Source>` or `@withoutDefaultValues` | Same effective edge metadata transform. |
| Visibility projection | `Read<T>`, `Create<T>`, `Update<T>`, `withVisibility` family | Same family of filtering/copying transformations with TypeSpec-specific lifecycle semantics. |
| Concatenate models | Spread, `is`, or intersection | Effective edge concatenation exists; each spelling carries different provenance and inheritance meaning. |
| User-defined mapped type in `.tsp` | Templates can compose existing language and library transformations | Custom iteration over every edge and conditional edge generation requires a JS decorator/function or library primitive. |
| Conditional type operator | No TypeScript-style conditional type syntax | Use JS/compiler APIs or retain a TSI/DL7 operator relation. |
| Recursive user transform | No recursive `.tsp` function body | Use JS or DL7 recursive rules. |
| Type interning | Compiler caches template instantiations and holds semantic object identity | TSI needs stable cross-run IDs derived from symbols or structural content; TypeSpec exposes no stable wire ID. |

## Compiler programmability

| Capability | TypeSpec 1.15 | TSI and DL7 target | Parity |
|---|---|---|---|
| Parse and bind declarations | Compiler-owned phases | Reader/parser and binding facts | Same information can be exported. |
| Type checking | Compiler checker and typekit assignability | Rules and host/native checker facts | Semantic adapter can import TypeSpec results. |
| Typed metadata declaration | `auto dec` entirely in `.tsp` | Relations over node/edge IDs | Direct metadata parity. |
| Metadata behavior | JS decorator callbacks | DL7 rules | TypeSpec needs JS. |
| Type-producing function | experimental `extern fn`, JS implementation | DL7 callable relation with type output | Signature parity; implementation model differs. |
| Type creation | Typekit create/clone/finish APIs | Interned node and edge facts derived by rules | Semantic result parity through an adapter. |
| Complete-graph callback | `onGraphFinish` | Fixpoint closure over type facts | Both can observe a closed input graph; evaluation and provenance differ. |
| Compiler stage identity | `Program.currentStage` reports parsing, checking, validating, linting, emitting | Macrotime/comptime/runtime clocks and strata | Stage fact can be emitted. TypeSpec library code still follows compiler-controlled callbacks. |
| Cache | `Program.useCache` plus program state maps | Relation memoization/tabling and content-addressed host-call cache | Cache behavior can be recorded as producer metadata. |
| Macro syntax rewriting | No general source macro system | Planned macrotime over DL7 forms | `absent` in TypeSpec source. |
| Recursive logical rules | No `.tsp` rule body | Positive recursive DL7 rules, stratified negation and aggregates | `absent` in TypeSpec source. |
| Compile-time host effect | JS library or emitter can call host APIs | Typed compiler effect event and next-round result facts | Similar external capability; TypeSpec has no common typed effect relation. |
| Runtime program semantics | TypeSpec compilation ends at emitted artifacts | Runtime relations, ticks, arrivals, retractions, effects | `absent` in TypeSpec. |

TypeSpec functions are declared with typed `extern fn` signatures and
implemented in JavaScript. They support optional and rest parameters, value and
type arguments, and return constraints. The official
[functions guide](https://typespec.io/docs/extending-typespec/implement-functions/)
documents that boundary.

## Extraction and protocol parity

TSI also specifies facts about the extraction process. TypeSpec exposes enough
data to produce these rows, while it does not define a standard external fact
stream.

| TSI protocol fact | TypeSpec source/compiler data | Parity | Required adapter work |
|---|---|---|---|
| `extract.run(Run, Mode, Tool, Version, Scope)` | Compiler version, project root, compiler options, source files | `derived` | Mint one run row and hash effective configuration/dependencies. |
| `extract.fact(Fact, Relation, Arguments)` | Emitter-created serialization | `absent` as standard wire | Canonically serialize relation and ordered arguments. |
| `extract.witness(Fact, Run, Method)` | Source node, declaration, checker call, decorator state, or adapter derivation | `derived` | Classify witness method and retain source range or API invocation. |
| `extract.coverage(Run, Relation, partial|complete)` | Semantic walker traverses reachable TypeSpec types | `derived` | Adapter must define roots, library filtering, synthetic type policy, unfinished templates, and relation-specific enumeration. |
| syntax mode | Parser AST and source files | `semantic` | A syntax extractor can emit declarations and candidate references without checking. |
| semantic mode | Checked `Program`, semantic walker, checker, typekits, state maps | `semantic` | Emit every supported reachable row and coverage declarations. |
| stable fact ID | No standard external ID | `absent` | Hash relation plus canonical argument IDs. |
| stable type ID | Named source symbols and source ranges; semantic object identity is in-memory | `derived` | Nominal IDs use SCIP symbols. Anonymous and called types use structural/application interning. |
| proof object | Assignability API returns boolean and diagnostics | `absent` as derivation tree | Emit an opaque witness containing compiler/version/query/diagnostics. |
| reverse ingest | Typekit can create synthetic types programmatically | `JS library` | A TSI-to-TypeSpec adapter must reconstruct compiler types or generate `.tsp`; TypeSpec defines no generic fact-stream loader. |
| language-native extension rows | Decorators, type categories, compiler fields | `derived` | Emit `tsp.*` rows alongside common TSI rows. |

The `Program` API exposes source files, the global namespace, resolution,
diagnostics, host access, `stateMap`, and `stateSet`. See the official
[`Program` interface](https://typespec.io/docs/standard-library/reference/js-api/interfaces/program/).
`getSourceLocation` supplies source positions for semantic and diagnostic
targets through the official
[`getSourceLocation` API](https://typespec.io/docs/standard-library/reference/js-api/functions/getsourcelocation/).

### Coverage boundary

`navigateProgram` visits all types, including standard-library and imported
library types. A semantic adapter therefore needs an explicit root and scope
policy. Complete coverage for `tsi.edge` means:

1. Every reachable owner type has been visited.
2. Every member category supported by the protocol has been enumerated.
3. Inherited model properties follow a stated policy.
4. Anonymous and dynamically built types have IDs.
5. Template declarations, closed instances, and unfinished instances are
   distinguished.
6. Decorator-created synthetic types are included or explicitly excluded.

A whole-program walk by itself does not prove those six conditions. Coverage
is emitted per relation after the adapter checks them.

## Facts TypeSpec can express entirely in `.tsp`

The following fact families have direct source forms in TypeSpec 1.15:

| Family | Source forms |
|---|---|
| Names and scopes | imports, namespaces, declarations, `using`, aliases |
| Product structure | named/anonymous models, properties, tuples, arrays, records |
| Sum structure | named/anonymous unions and enums |
| Primitive hierarchy | scalars, scalar inheritance, literals, intrinsic types |
| Service call signatures | operations, parameters, return types, interfaces |
| Generic declarations and closed calls | templates, constraints, defaults, named arguments, type/value arguments |
| Composition | `extends`, `is`, spread, intersection, operation reuse |
| Edge presence | optional properties, defaults, ordering, indexers |
| Built-in constraints | key, range, length, pattern, format, encoding, visibility |
| Arbitrary typed metadata | auto decorators on supported type targets |
| Compile-time external callable signatures | decorators and experimental functions |

## Facts requiring JavaScript in TypeSpec

| Family | JavaScript mechanism |
|---|---|
| Custom decorator behavior | Decorator implementation and program state |
| Custom function behavior | Experimental `extern fn` implementation |
| New type construction | Typekit or checker create/clone/finish APIs |
| Whole-graph custom validation | `onGraphFinish` callback or linter rule |
| Custom model transformation | Decorator, function, mutator, or emitter traversal |
| Artifact generation | `$onEmit`, semantic walker, emitter framework |
| Host filesystem/network/process effects | Compiler host or ordinary JS APIs under library policy |
| TSI serialization | Custom semantic adapter/emitter |

## Facts supplied by DL7 beyond TypeSpec

| Family | TSI or DL7 representation |
|---|---|
| User-defined recursive compiler relation | Rules over `tsi.*` and user relation facts |
| Fixpoint-derived type graph | Interned result nodes and derived edges |
| Arbitrary partial generic application | Canonical partial callable plus remaining input slots |
| Higher callable contract | Callable accepting or returning callable types |
| Relational cardinality and modes | Deterministic, semideterministic, and multi-result relation facts |
| Rule provenance | Witness relation and derivation premises |
| Syntax/semantic evidence merge | Run, witness, and coverage rows with stratified acceptance |
| Runtime dataflow | Relations, arrivals, retractions, ticks, clocks, and effects |
| Storage and layout projection | Target-independent logical facts followed by physicalizer facts |
| Temporal history behavior | Userland rules that generate history nodes, keys, sequence/time edges, and runtime rows |
| Reverse fact ingestion | TSI rows entering the same comptime relation closure |

## Worked end-to-end example

TypeSpec source:

```typespec
namespace Example;

auto dec interned(target: Scalar);
auto dec historyV1(target: Model, keep?: valueof string);

@interned
scalar UserName extends string;

model Entity<Id> {
  @key id: Id;
}

@historyV1("all")
model User extends Entity<int64> {
  name: UserName;
  email?: string;
}

union LookupResult {
  found: User,
  missing: null,
}

op lookup(id: int64): LookupResult;
```

Direct and derived TSI rows:

```text
tsi.type(entity)
tsi.product(entity)
tsi.parameter(entity_id, entity, 0, unspecified)

tsi.called(entity_int64, entity, args_entity_int64)
tsi.argument(args_entity_int64, 0, int64)

tsi.type(user)
tsi.product(user)
tsi.subtype(user, entity_int64, user_extends_witness)
tsi.edge(user_name, user, name, user_name_scalar, 0)
tsi.edge(user_email, user, email, string, 1)
tsp.optional(user_email)
tsp.key(inherited_user_id_edge)

tsi.type(lookup_result)
tsi.sum(lookup_result)
tsi.edge(found_variant, lookup_result, found, user, 0)
tsi.edge(missing_variant, lookup_result, missing, null, 1)

tsi.callable(lookup)
tsi.input(lookup, 0, int64)
tsi.output(lookup, 0, lookup_result)

tsp.decorator(user_name_scalar, interned, empty_args)
tsp.decorator(user, history_v1, history_args)
tsp.decorator_argument(history_args, keep, "all")
```

Subsequent DL7 comptime rules may consume the two decorator rows:

```text
interning_policy(user_name_scalar, text_intern_pool)
HistoryV1(user, history_args, user_history)
```

The TypeSpec producer supplies source declarations, checked semantic types, and
typed metadata. DL7 supplies rule execution, canonical generated identities,
layout selection, and runtime semantics.

## Decisions

1. A TypeSpec semantic adapter emits common `tsi.*` rows and lossless
   `tsp.*` rows. Common rows alone cannot retain all TypeSpec source semantics.
2. TypeSpec semantic object identity is scoped to one compilation. TSI type IDs
   derive from SCIP symbols, structural closure, or callee plus ordered
   arguments.
3. TypeSpec aliases require syntax/SCIP extraction because aliases disappear
   from the semantic type graph.
4. TypeSpec template calls map to `tsi.called` plus ordered `tsi.argument`
   rows. TSI must admit value arguments before claiming complete template
   parity.
5. TypeSpec auto decorators map directly to typed node/edge annotation facts.
   JavaScript is unnecessary for metadata-only declarations in TypeSpec 1.15.
6. TypeSpec custom graph transformations map to JS decorator/function/typekit
   implementations. DL7 custom graph transformations map to userland rules.
7. TypeSpec assignability results produce opaque compiler witnesses containing
   compiler version and diagnostics. They do not produce derivation trees.
8. A TypeSpec semantic run claims complete coverage relation by relation, after
   root, inheritance, anonymous type, synthetic type, and template-state policy
   checks.
9. History, storage, DBSP, clock, and runtime effect facts remain downstream
   DL7 concerns. TypeSpec can state typed metadata that selects those rules.
10. The first TypeSpec adapter surface reads `Program`, semantic types, auto
    decorator state, and source locations. Reverse TSI ingestion remains a
    separate adapter direction.

## Verification

| Check | Receipt |
|---|---|
| Current release | Official TypeSpec 1.15.0 release note and GitHub stable release feed, checked 2026-09-02 |
| Core source constructs | Official language pages for models, unions, enums, operations, interfaces, templates, intersections, aliases, values, namespaces, and type relations |
| Semantic graph | Official JS API pages for `Program`, `Model`, `Union`, `Checker`, `TypeInstantiationMap`, `navigateProgram`, and `getSourceLocation` |
| Metadata | Official decorator pages, including 1.15 auto-decorator read/write APIs |
| Type construction | Official typekit API for create, clone, finish, traversal, and assignability |
| Existing Sprefa target | `.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md` |
| Higher callable target | `.agents/skills/sprf-dl7-prolog-compiler/references/5_soopy_calls_and_higher_types.md` |
| Earlier TypeSpec work | `plans/2026-08-12-typespec-module-ir.RESEARCH.md` and `plans/2026-08-16-typespec-parity-typegen.PLAN.md` |

This change adds documentation only. It adds, changes, or removes no CI
coverage.

## Staffing

| Item | Value |
|---|---|
| Work | Current upstream research, TSI mapping, and one repository document |
| Agent | Codex, one lane |
| External sources | Official TypeSpec documentation and Microsoft TypeSpec release repository |
| Code ownership | No source or test files changed |
