# TypeScript-shaped Interface Bounds

## Context

DL6 currently accepts only bare interface names in generic bounds. Adding
parameterized interface applications must avoid a predicate-shaped surface
that repeats the constrained parameter:

```dl6
rel Box(T: json_encodable(T))(value: T).
```

The compile-time `$type` plane added in `v6/prolog/0_generic_expand.pl` can
judge interface evidence relationally, but its authored constraint should read
like the corresponding TypeScript, Go, and Rust constraint.

```ts
type Box<T extends JsonEncodable<any>> = { value: T }
```

```go
type Box[T JsonEncodable[any]] struct { Value T }
```

```rust
struct Box<T: JsonEncodable<JsonValue>> { value: T }
```

## Decisions

### A bound names an interface application

```dl6
rel Box(T: json_encodable(any))(value: T).
```

Read it as: `T` must implement some `json_encodable<A>` application. The
`any` occupies the interface's generic argument. It does not stand for `T`.

Exact interface arguments remain expressible:

```dl6
rel TextBox(T: encodable_as(text))(value: T).
```

This requires evidence equivalent to `T is encodable_as(text)`.

### `any` is a type-pattern wildcard

For this slice, `any` is legal in interface-bound argument positions. It
unifies with one complete type term and is not emitted into runtime storage.
It does not erase the constrained type, disable checking, or become a SQLite
column type.

`any` is refused in relation columns, interface declarations, and
implementation arguments. It is reserved for interface-bound patterns in this
slice. TypeScript emission spells this wildcard `unknown` so generated types do
not acquire unchecked operations.

```text
bound:          T: Interface(PatternArgs...)
evidence:       T is Interface(ConcreteArgs...)
accept when:    PatternArgs unify with ConcreteArgs
```

Examples:

```text
T: json_encodable(any)    accepts T is json_encodable(json)
T: encodable_as(text)     accepts T is encodable_as(text)
T: encodable_as(text)     rejects T is encodable_as(bytes)
```

### Bare interfaces remain shorthand for zero arguments

```dl6
rel Plain(T: comparable)(value: T).
```

This checks `T is comparable`. No implicit insertion of `T` into an interface
argument occurs.

### Type signatures

```text
Bound                 = bound(TypeParameter, InterfacePattern)
InterfacePattern      = interface(Name, [TypePattern...])
TypePattern           = exact(Type) | any
Implementation        = implements(Type, InterfaceApplication)

matchBound
  : Bound × Implementation
  -> option<InterfaceProof>
```

Body sketch:

```text
matchBound(bound(T, interface(Name, Patterns)), implements(T, interface(Name, Args))):
  require same arity
  require every Pattern matches its corresponding Arg
  return proof(T, Name, Args)
```

Normalized rows retain the complete application:

```text
constraint(ConstraintId, ParameterId, ApplicationId)
implementation(ImplementationId, SubjectTypeId, ApplicationId)
application(ApplicationId, InterfaceId)
argument(ApplicationId, Ordinal, exact(TypeId) | any)
```

Constraint and implementation IDs include the interface plus ordered
arguments. `codec(text)` and `codec(bytes)` therefore remain distinct facts.

### Instance timeline and lifetime

1. Parse the generic parameter `T` and its interface pattern.
2. Validate every bound and implementation against the declared interface
   arity.
3. Collect explicit and structurally derived implementation facts.
4. Specialize a generic relation with a concrete type for `T`.
5. Match that concrete type's implementation facts against the bound pattern.
6. Accept one matching proof or return the existing named unsatisfied-bound
   diagnostic.
7. Erase compiler-local interface proof rows before runtime lowering.

The wildcard and proof exist for one compiler invocation. The specialized
relation retains the existing canonical concrete type identity.

### Storage, reads, writes, and uniqueness

The compiler reads implementation rows and writes proof rows in the in-memory
`$type` plane. Wildcard and proof rows add no SQLite table, runtime relation,
boot fact, host request, or Differential Dataflow collection. Existing
`type_row` catalog metadata retains interface applications and ordered
arguments so TypeScript and Rust type artifacts can render the authored bound.

Proof rows retain set semantics. Multiple matching implementations yield one
equivalent conformance result. Interface name and arity participate in the
match. Argument order remains significant.

Two implementations conflict only when subject and complete interface
application are equal. One subject may implement both `codec(text)` and
`codec(bytes)`. A wildcard bound may match both and still produces one
conformance result.

Structural `json_encodable` evidence has the concrete application
`json_encodable(json)`. A wildcard bound may match that evidence. The wildcard
never creates evidence by itself.

## Rejected alternatives

- `T: json_encodable(T)`: repeats the constrained type and models the
  interface as a predicate call.
- Implicit subject insertion: makes interface argument positions depend on a
  hidden convention.
- TypeScript-style unchecked `any`: would allow a missing implementation to
  satisfy a bound.
- Runtime wildcard values: belongs to runtime value typing rather than
  compile-time interface matching.

## Sequence

1. Parse and print interface applications inside generic bounds, including
   `any` arguments.
2. Carry interface name plus argument patterns into normalized `$type` rows.
3. Match implementation applications by constrained type, interface name,
   arity, and argument patterns.
4. Emit generic interface declarations and applications in TypeScript and Rust
   type artifacts.
5. Update the comprehensive DL6 golden and language syntax documentation.
6. Give `T: json_encodable(T)` a named repeated-subject refusal.

## Verification

- Parse, print, and reparse `T: json_encodable(any)`.
- Prove wildcard, exact-argument, wrong-argument, wrong-arity, and missing
  implementation cases.
- Prove `codec(text)` and `codec(bytes)` coexist for one subject and that an
  exact bound selects one while a wildcard bound set-deduplicates the result.
- Prove two independent parameters can carry different applications of the
  same interface.
- Prove recursive structural conformance still terminates.
- Prove compiler-local wildcard and proof rows do not occur in emitted SQLite,
  TypeScript runtime plans, Rust `ProgramJson`, or boot data; separately prove
  type-artifact catalog metadata retains ordered interface arguments.
- Add the syntax to `golden-flex.dl6` without adding explanatory comment bulk.
- Run compiler CI plus TypeScript and Rust emitted-program CI.

## Staffing

- Luna implements the parser, normalized rows, matching, goldens, and focused
  CI in an isolated worktree.
- Terra reviews the type meaning, compatibility behavior, wildcard boundary,
  and emitted-plane erasure. Terra reports findings before merge and may make
  corrections in a separate isolated worktree after the implementation commit.
- Base SHA: current `origin/main` when each worktree is created.
- One full compiler CI run after reconciliation. Focused tests during edits.

<!-- todo(feature): Implement TypeScript-shaped interface bounds with `any` argument patterns. -->
<!-- todo(docs): Document interface applications, exact arguments, and wildcard arguments in the DL6 syntax reference. -->
