# Sprefa V7 kernel reconciliation

Status: design seed after the DL6 donor audit.

V7 is a fresh `.dl7` source language compiled by SWI-Prolog. DL6 contributes
algorithms, semantic laws, fixtures, and the existing Rust engine contract.

```text
.dl7 text
    |
    v
prefix terms
    |
    v
bindings + resolved references
    |
    v
semantic facts and rules
    |
    v
compile-time fixpoint
    |
    v
checked target-neutral plan
    |
    v
ProgramJson adapter
    |
    v
sprefa-engine-rs
```

The next implementation work begins after five contracts have executable
examples and tests.

## 1. Reader term contract

The reader recognizes four kernel term categories:

```prolog
atom(Name).
literal(Value).
variable(Name, Identity).
form(Items).
```

The source surface is prefix and list-shaped:

```lisp
(send ?Message ?Target)
(: User (* (: id int) (: name text)))
(<- (reachable ?A ?C)
    (edge ?A ?B)
    (reachable ?B ?C))
```

Bare identifiers are atoms. `?Name` is a logic variable. Literals have reader
syntax and never enter name resolution.

### Signature

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).

% Scan comments and literals.
% Assign one variable identity to each lexical ?Name binding region.
% Preserve every form's source span in SourceMap.
% Return data only; perform no declaration or type expansion.
```

### Lifetime

1. Text and source positions live for one compilation.
2. Reader variable identities live until forms are resolved and lowered.
3. Semantic identities minted later do not depend on byte offsets.

### Storage and uniqueness

- `SourceMap` is keyed by reader node identity.
- Repeated `?Name` inside one rule shares a variable identity.
- The same spelling in another rule receives another identity.
- Bare atoms carry spelling only. Scope resolution supplies identity later.

### DL6 donors

- Extract literal, escape, comment, balanced-group, source-position, and parse
  diagnostic predicates from the reader audit.
- Leave statement dispatch, `rel`, brace declarations, named-argument puns,
  and infix surface parsing in DL6.

## 2. Binding, scope, and recursive name contract

`:` constructs a binding pair. The enclosing form decides where that pair is
installed.

```lisp
(: name expression)
```

The semantic scope graph uses one edge shape:

```prolog
binding(+Owner, +Name, +Ordinal, +Target).
scope_parent(+Scope, +ParentScope).
```

`Owner` identifies a scope, product, namespace, callable parameter list, or
another ordered collection of named targets. The edge has no globally unique
name. Its identity is the tuple `(Owner, Name, Ordinal)`.

### Signatures

```prolog
resolve_name(+Scope, +Name, -Target).

% Search Scope first, then its parent chain.
% Return one target.
% Report an unresolved or ambiguous reference instead of minting a target.

install_bindings(+Owner, +Pairs, -BindingRows).

% Check unique names and ordinals within Owner.
% Preserve authored order.
% Emit binding/4 rows.
```

### Recursive definitions

Recursive groups use reserve, resolve, and fill:

```text
1. Read every binding name in the group.
2. Mint one stable target identity per name.
3. Install those targets in the group's scope.
4. Resolve each binding body with the complete group visible.
5. Unify each reserved target with its completed definition.
```

This supports self-recursive and mutually recursive types without requiring
lists to solve name recursion.

### Lifetime

1. A lexical scope exists while its forms are resolved.
2. A semantic scope identity survives through compile time when emitted facts
   refer to it.
3. Target identities survive as long as the compiled plan or artifact refers
   to them.

### Storage and uniqueness

- `(Owner, Name)` identifies one visible local binding.
- `(Owner, Ordinal)` identifies one authored position.
- Shadowing is represented by different owners.
- Module paths and namespaces are scope edges. Target emitters may encode them
  into flat artifact names.
- No declaration or callable is inferred from an unresolved application.

### DL6 donors

- Adapt scope-tree insertion, collision checks, and path resolution.
- Replace declaration-list scans and `__`-joined semantic names with
  `binding/4` and `scope_parent/2` inputs.

## 3. Application and partial-application contract

Every source form in value position resolves its first item to a declared
callable. Applying that callable produces one result. Products represent
multiple fields inside that one result.

```prolog
callable(+Callable, +ParameterOwner, +ResultType).
parameter(+ParameterOwner, +Name, +Ordinal, +ExpectedType).
invoke(+Callable, +ArgumentBindings, -Result).
```

Example source:

```lisp
(: MaybeInt (Option int))
```

Expression lowering introduces the result variable that Datalog requires:

```prolog
invoke(Option, [argument(0, int)], MaybeInt).
```

No result object is duplicated. `MaybeInt` is the result identity. For a type
constructor, it is also the specialized type identity.

### Generic specialization

```prolog
specialization(+TypeId, +Constructor, +Arguments).

% Require a ground constructor and ground argument identities.
% Intern the tuple (Constructor, Arguments).
% Emit the same TypeId for every equal tuple.
```

This relation records why the type exists. The functional dependency is:

```text
(Constructor, Arguments) -> TypeId
```

### Partial application

Argument bindings are ordered edges. A call with unfilled declared parameters
can therefore produce another callable:

```prolog
partial_callable(+Partial, +BaseCallable, +BoundArguments).
```

The grammar needs no separate partial-application form. The first V7 evaluator
must still rule on two behaviors before implementation:

1. whether an unsaturated call automatically returns `Partial`, or requires an
   explicit callable operation;
2. whether named and ordinal arguments may be mixed in one call.

### Lifetime

1. Source call nodes live through expression lowering.
2. Compile-time calls run during the compiler fixpoint.
3. Ground type specializations are interned for the compilation and receive
   deterministic artifact encodings.
4. Runtime calls enter the execution plan after compile-time calls are erased.

### Storage and uniqueness

- One callable declares one ordered parameter owner and one result type.
- One argument may bind each parameter slot.
- Equal ground constructor and argument tuples share one specialization ID.
- Calls preserve explicit source references. An undeclared atom never becomes
  a callable through use-site inference.

### DL6 donors

- Extract semantic ID encoding and canonical generated-name encoding.
- Adapt the current `application(Constructor, Arguments)` rows and
  `type_apply` request loop to the `invoke/3` and `specialization/3` contracts.
- Preserve generic, partial type operator, and type identity fixtures as
  oracles.

## 4. Compile-time relation and fixpoint contract

Compile time uses the same relational rule evaluator as ordinary relational
execution. Phase ownership determines inputs, lifetime, and allowed effects.

```prolog
evaluate_fixpoint(+SeedRows, +Rules, +Options, -ClosureRows, -Requests).

% Validate rule safety and functional dependencies.
% Freeze the dependency graph and compute strata.
% Evaluate positive recursive groups with SWI tabling.
% Read negation and aggregates from completed lower strata.
% Return new interning and derived-definition requests.
```

The compile-time timeline is:

```text
resolved forms
    -> seed semantic rows
    -> freeze rule dependency graph
    -> stratified tabled closure
    -> collect ground construction requests
    -> intern requested identities and definitions
    -> repeat while new requests exist
    -> run checks
```

### Phase boundary

Compiler relations must be declared as compiler-owned facts or rules. V7 does
not use DL6's heuristic that a relation belongs to the compiler plane because
one column has type `type`.

### Termination

- Positive recursion operates over a finite set of ground rows.
- Negation and aggregates read completed lower strata.
- Construction requests require ground constructor and argument identities.
- A repeated request interns to the same identity and adds no row.
- Recursive type construction needs an explicit termination rule before the
  current fixed cap of 16 refreeze rounds can be removed.

### Lifetime

1. Seed and closure rows live for one compilation.
2. SWI tables live for one evaluation and are cleared through a cleanup
   boundary.
3. Interned semantic identities survive into checking and planning.
4. Compiler transport rows are erased before runtime planning.

### Storage and uniqueness

- Relations use declared functional keys.
- Closure rows are set-valued within a fixpoint.
- Aggregates are grouped by explicit key positions.
- Requests are deduplicated by their complete ground tuple.
- Mutable SWI state is hidden behind one evaluation entry point and cleanup
  scope.

### DL6 donors

- Extract authored-order safety, strata relaxation, aggregate separation,
  tabled closure, functional-key checks, and request deduplication.
- Adapt `:=` binding forms, compiler-plane partitioning, and DL6 declaration
  carriers.
- Preserve recursion refusal, stratification, aggregate, and userland type
  operator tests as oracles.

## 5. Semantic plan and engine contract

V7 lowers checked semantic facts into one target-neutral plan value. Emitters
read that value without querying compiler modules.

```prolog
build_plan(+SemanticRows, +Rules, -Plan, -Diagnostics).

% Assign runtime relation and column identities.
% Build dependency levels, edge writes, retention, arrivals, and queries.
% Carry type and layout metadata required by emitters.
% Exclude source syntax and compiler-only transport rows.

emit_program_json(+Plan, -ProgramJson).

% Adapt the target-neutral plan to the existing Rust engine schema.
% Preserve ir_version and the engine's tick phase ordering.
```

The plan contains five explicit graphs or row families:

```text
binding graph     scope and name ownership
type graph        type identities, fields, sums, products, specializations
rule graph        reads, writes, polarity, strata, aggregates
layout graph      runtime storage identity and encoded column representation
temporal graph    arrivals, ticks, retention, occurrences, boundary deltas
```

The binding and type graphs originate in compile time. The rule, layout, and
temporal graphs feed runtime planning. A target adapter may derive SQL DDL,
Rust declarations, TypeScript declarations, schemas, or another artifact from
the same plan.

### Lifetime

1. The semantic plan exists after compiler checks pass.
2. Target adapters consume an immutable plan.
3. ProgramJson lives at the compiler-to-engine boundary.
4. Runtime state remains owned by `sprefa-engine-rs`.

### Storage and uniqueness

- Plan relation and column IDs are stable within one artifact.
- Artifact names are target encodings rather than semantic identities.
- ProgramJson remains versioned by `ir_version`.
- The engine's arrival DTOs and tick phase order remain compatibility oracles.

### DL6 donors

- Extract graph algorithms from lowering and checks.
- Adapt `plan/9`, `lowered/8`, and emitter inventory logic behind `Plan`.
- Preserve ProgramJson fields, arrival DTOs, tick order, and cross-door engine
  fixtures.
- Keep SQLite-specific DDL production in the ProgramJson adapter rather than
  in the V7 semantic kernel.

## Module extraction order

Numeric prefixes express the dependency and reading order:

```text
v7/
  0_READER/       text -> prefix terms and source map
  1_KERNEL/       term, identity, binding, and diagnostic contracts
  2_RESOLVE/      lexical scope, paths, recursive bind groups
  3_COMPTIME/     rule safety, strata, tabling, requests, interning
  4_CHECK/        type, constraint, clock, and dependency checks
  5_PLAN/         target-neutral runtime plan
  6_EMIT/         ProgramJson and later target adapters
  7_ORACLES/      syntax-independent semantic fixtures
```

SWI-Prolog modules expose only the signatures above and pure helper
predicates. Rust reuses the same contracts through serialized fixtures and the
versioned plan boundary. Parser terms and SWI mutable state do not cross that
boundary.

## Decisions still required before implementation

1. Exact atom, string, number, comment, and quoted-name reader spelling.
2. Exact forms that create scopes and recursive binding groups.
3. Unsaturated application behavior and named versus ordinal argument mixing.
4. Syntax or metadata that declares a relation as compiler-owned.
5. The first V7 plan schema and its mapping to the current ProgramJson fields.
