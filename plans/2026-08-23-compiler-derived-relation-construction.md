# Compiler-derived relation construction

## Context

DL6 already has the semantic and fixpoint machinery needed to name and
observe constructed types:

- `application(ConstructorTypeId, OrderedArgumentTypeIds)` is the canonical
  application identity in `v6/prolog/0_type_ids.pl`.
- Closed `type_apply/3` calls bind that identity inside the current compiler
  closure in `v6/prolog/0_compiler_relations.pl:333-338`.
- Missing closed applications enter the bounded refreeze loop in
  `v6/prolog/0_generic_expand.pl:59-83`.
- Canonical compiler sources expose declarations, members, roles,
  applications, and arguments in `v6/prolog/0_generic_expand.pl:1042-1072`.
- Existing generic and anonymous expansion can mint ordinary relation
  carriers and canonical rows. The anonymous product path is visible in
  `v6/prolog/0_anonymous_expand.pl:175-196`.

The implemented path handles a constructor whose member body is already known:

```text
Box(Int)
  -> substitute Int into Box's authored member template
  -> mint a concrete declaration
  -> freeze canonical rows
```

The missing path handles a constructor whose member body is computed by
compiler relations:

```text
Partial(User)
  -> query User's canonical members
  -> derive one optional member per source member
  -> submit one complete relation shape
  -> mint a concrete declaration
  -> freeze canonical rows
```

Canonicalization already owns identities and normalized rows after a type has
entered the graph. This arc adds the entry seam from a complete compiler-derived
member set into that graph.

The parser already accepts compound terms inside head arguments through
`expr//1` and `compound_or_var//1` at
`v6/prolog/compile/parse_dl_dcg.pl:1193-1205` and
`v6/prolog/compile/parse_dl_dcg.pl:1639-1645`. The missing surface work is
signature-directed lowering of a type-valued head term into explicit
`type_apply/3` body goals. The compiler evaluator remains function-free after
that lowering.

Generic applications currently have two linked semantic objects:

```text
application identity
  application(ConstructorTypeId, OrderedArgumentTypeIds)

materialized declaration identity
  named(Module, relation, GeneratedName)
```

`derived_from(MaterializedDeclarationId, ApplicationTypeId)` links them in
`v6/prolog/0_generic_expand.pl:2165-2175`. Generated members are owned by the
materialized declaration. Compiler-facing member queries need to project that
link so authors can query members by the application identity returned from
`type_apply/3`.

This plan contains compiler work only. History capture, event storage,
reducers, higher-kinded constructor variables, kind signatures, and runtime
clock behavior remain outside this arc. The existing untracked
`plans/2026-08-23-relational-history-annotations.md` is left unchanged.

## Decisions

1. Add one general compiler-derived relation construction facility. `Partial`
   is the acceptance fixture and first proof of mapped structural construction.
   It does not receive dedicated parser syntax.

2. A literal type-returning compiler relation may act as a derived type
   constructor. Its constructor identity is its existing module-qualified
   relation `SemanticTypeId`. Its application identity remains:

   ```text
   application(ConstructorTypeId, OrderedInputTypeIds)
   ```

   The constructor arity is the ordered `type` inputs before the final
   `return: type` column. Constructor-valued variables, partial application,
   and kind arrows remain deferred.

3. Authored functional terms in `type`-typed compiler head columns lower to
   explicit relational IR. Example:

   ```dl6
   Partial(Source, Partial(Source)) <- source_relation(Source).
   ```

   lowers to:

   ```dl6
   Partial(Source, PartialType) <-
       source_relation(Source),
       type_apply(PartialConstructor, [Source], PartialType).
   ```

   Nested terms lower inside-out. Constructor resolution uses the expected
   column domain and the compiler symbol table. Capitalization does not decide
   whether the token is a variable. `Partial(Source)` is a compound type term;
   bare `Source` in a term slot remains a variable.

4. The first request IR consists of ordinary compiler relations:

   ```text
   type_requested(
     +ApplicationTypeId,
     +ConstructorTypeId,
     +OrderedArgumentTypeIds
   )

   derived_relation_request(
     +ApplicationTypeId,
     +ConstructorTypeId,
     +OrderedArgumentTypeIds,
     +MemberCount
   )

   derived_member_request(
     +ApplicationTypeId,
     +Position,
     +Name,
     +MemberTypeId
   )

   derived_member_role_request(
     +ApplicationTypeId,
     +Position,
     +Role,
     +RoleArgument
   )
   ```

   `MemberCount` makes a zero-member relation expressible and lets validation
   distinguish a complete request from a partial row set. These relations are
   compiler IR and erase before runtime planning.

5. `type_requested/3` is the demand boundary. It projects applications found
   in authored type positions and applications requested by interpreted
   `type_apply/3` calls. Derived constructors emit shapes only for matching
   demand rows. This prevents a mapped constructor from applying itself to
   every declaration discovered in later refreeze rounds.

6. A request becomes eligible for materialization only after the compiler
   closure is complete for that round. Validation groups rows by
   `ApplicationTypeId` and requires:

   ```text
   exactly one header after set deduplication
   header constructor and arguments reconstruct the application identity
   exactly MemberCount members
   positions equal 1..MemberCount
   member names unique within the application
   every member type is a canonical or structurally valid SemanticTypeId
   every role targets an existing requested member position
   every request row is ground
   ```

7. Exact duplicate request rows deduplicate. Conflicting headers, positions,
   names, member types, or roles receive named diagnostics carrying the
   application identity and conflicting values.

8. Materialization reuses the existing carrier and freeze pipeline. The bridge
   creates the equivalent of:

   ```text
   type_decl(GeneratedName, OrderedColumns)
   col_type(GeneratedName/Arity, Name, TypeTerm)
   semantic_decl_module(relation, GeneratedName, Module)
   application(ApplicationId, ConstructorId)
   argument(...)
   declaration(MaterializedId, ..., relation, materialized)
   derived_from(MaterializedId, ApplicationId)
   member(...)
   member_role(...)
   ```

   `freeze_type_rows/2` remains the authority that merges and validates the
   canonical graph. The bridge must factor or reuse current generic and
   anonymous relation minting. It must not establish a second generated-type
   registry or inject a parallel member representation after freeze.

9. The application identity is the compiler-visible type returned by
   `type_apply/3`. The generated declaration name remains artifact-boundary
   materialization data. Existing `derived_from/2` retains the mapping.

10. Add an ergonomic compiler source view whose member type column is a
    `SemanticTypeId`:

   ```text
   type_field(
     +MemberId,
     +OwnerTypeId,
     +Position,
     +Name,
     +ValueTypeId
   )

   type_field_count(
     +OwnerTypeId,
     +MemberCount
   )
   ```

   The view projects materialized members through `derived_from/2` so an
   application can be queried directly. It also unwraps canonical
   `type_ref(...)` transport into the member's semantic value type. Existing
   raw `type_member/5` behavior remains unchanged. `type_field_count/2`
   supplies one deterministic count row, including zero for a declared
   zero-member relation. Canonical `member/5` rows continue to be owned by the
   materialized declaration ID.

11. Derived construction uses the existing immutable-round boundary. Request
    rows produced in round N become carriers for round N+1. Compiler joins in
    round N never observe a partially constructed type.

12. The existing 16-round limit, canonical-row stability check, structural
    request deduplication, and recursive-construction refusal remain the
    termination contract for this slice. General chase-termination proofs and
    higher-kinded recursion remain separate work.

13. `Partial(User)` maps each source member type `T` to `option(T)`, preserves
    source position and name, and emits no source key role. Its acceptance
    purpose is to prove mapped member generation, nested `type_apply`, complete
    request assembly, refreeze, and target monomorphization.

14. A program with no derived relation requests must retain byte-identical
    runtime declarations, rules, storage plans, and emitted artifacts.

Rejected alternatives:

- A `partial` keyword or decorator grammar creates a second construction
  surface beside ordinary compiler relations.
- Writing compiler rules directly into canonical `member/5` rows permits
  incomplete graphs to enter the semantic authority.
- Using generated names as application identity discards the structural
  identity contract already used by `type_apply/3`.
- Replacing application IDs with materialized declaration IDs changes landed
  generic identity and artifact mappings.
- Constructor-valued parameters expand the slice into higher-kinded checking.
- Runtime construction of relation schemas crosses the compile/runtime phase
  boundary and prevents target monomorphization.

## Type signatures and lowering bodies

Surface declaration used by the acceptance fixture:

```dl6
rel Partial(Source: type) -> type.
```

Compiler-rule pseudocode:

```dl6
Partial(Source, Partial(Source)) <-
    type_requested(_, Partial, [Source]).

derived_relation_request(
    PartialType,
    Partial,
    [Source],
    MemberCount
) <-
    type_requested(PartialType, Partial, [Source]),
    type_field_count(Source, MemberCount).

derived_member_request(
    PartialType,
    Position,
    Name,
    option(MemberType)
) <-
    type_requested(PartialType, Partial, [Source]),
    type_field(_, Source, Position, Name, MemberType).
```

The final implementation may use internal names which follow existing
compiler-source naming. The argument domains and functional dependencies are
fixed by this plan:

```text
(ConstructorTypeId, OrderedArgumentTypeIds) -> ApplicationTypeId

ApplicationTypeId -> ConstructorTypeId, OrderedArgumentTypeIds, MemberCount

(ApplicationTypeId, Position) -> Name, MemberTypeId

(ApplicationTypeId, Name) -> Position, MemberTypeId
```

Head-term lowering body:

```prolog
lower_type_head_term(ExpectedDomain, Term, Lowered, AddedGoals) :-
    % Leave variables and already elaborated SemanticTypeIds unchanged.
    % Resolve a literal compound constructor in the compiler symbol table.
    % Recursively lower type-valued arguments inside-out.
    % Replace the compound term with a fresh application variable.
    % Append type_apply(ConstructorId, ArgumentIds, ApplicationVar) after the
    % positive body goals which bind its argument variables.
    % Preserve ordinary head values in non-type domains.
    true.
```

Request materialization body:

```prolog
materialize_derived_relation(RequestRows, SourceDecls, AddedDecls) :-
    % Group by ApplicationTypeId.
    % Validate one complete deterministic shape per application.
    % Convert member SemanticTypeIds back through the existing type-term bridge.
    % Mint one deterministic concrete declaration and derived_from edge.
    % Produce ordinary declaration carriers for the next refreeze round.
    % Return only compiler transport plus carriers; freeze remains downstream.
    true.
```

## Instance timelines and lifetimes

### Compiler round N

```text
freeze canonical $type snapshot N
  -> expose application demand and semantic-owner field views
  -> lower functional head terms to type_apply body goals
  -> evaluate positive compiler relations to set fixpoint
  -> collect derived relation request rows
  -> group and validate complete application shapes
  -> append deterministic declaration carriers to next frontier
```

The snapshot, compiler closure, request rows, and proof rows live for one
compiler round. Request collection reads only the completed closure.

### Compiler round N+1

```text
expand generated carriers
  -> expand nested option/generic/anonymous member types
  -> freeze canonical application, declaration, derivation, member, and role rows
  -> rerun compiler relations against the new immutable snapshot
```

The generated semantic rows live for the remainder of compilation and feed
target lowering. Compiler request transport is erased before the runtime plan.

### Artifact lifetime

```text
ApplicationTypeId
  -> stable semantic equality during compilation

MaterializedDeclarationId and GeneratedName
  -> deterministic bridge to target artifacts

SQLite table, TS interface, Rust struct, JSON Schema definition
  -> ordinary outputs of the existing target lowerers
```

No runtime registry, schema mutation API, boot fact, arrival, or durable request
row is introduced.

## Storage, reads, writes, and uniqueness

The compiler reads the frozen semantic graph and writes in-memory request rows.
After validation, it writes declaration carriers into the next construction
frontier. The next freeze writes one merged canonical semantic row set.

Uniqueness contracts:

```text
application identity = constructor TypeId + ordered argument TypeIds
request header key   = application identity
requested member key = application identity + position
member name key      = application identity + exact name
canonical member key = materialized declaration ID + position + exact name
materialization key  = application identity
```

Source term order and Prolog variable identity do not participate in equality.
Member order is the requested integer position. Exact duplicate rules and
derivations converge under existing set semantics.

The target read path begins only after the final stable freeze:

```text
final canonical rows
  -> physical relation and column rows
  -> SQLite/TS/Rust/JSON Schema emitters
```

## Implementation sequence

### Phase 0: fail-first contracts

- Add a focused fixture for `User` and `Partial(User)`.
- Pin the functional-head-term lowering IR.
- Pin exact request rows before materialization.
- Pin one demand row for `Partial(User)` and zero unsolicited demand rows for
  unrelated declarations.
- Pin the expected canonical application, materialized declaration,
  `derived_from`, member, role, and nested option application rows.
- Pin request transport erasure and unchanged runtime output without requests.

### Phase 1: functional type-head lowering

- Add signature-directed traversal for `type`-typed compiler head arguments.
- Lower nested compound terms inside-out into `type_apply/3` body goals.
- Register literal type-returning compiler relations as fixed-arity derived
  constructors.
- Preserve current safety, grounding, arity, unknown-constructor, and cycle
  diagnostics.

### Phase 2: complete derived-shape request IR

- Add compiler relation signatures and closure extraction for application
  demand and the three request relations.
- Project authored and interpreted application demand through
  `type_requested/3`.
- Group requests by structural application identity.
- Validate count, position, name, type reference, role, grounding, and conflict
  contracts.
- Add named diagnostics with deterministic payloads.

### Phase 3: canonical materialization and refreeze

- Factor the existing generated relation carrier minting seam.
- Materialize one concrete relation declaration per application request.
- Emit the existing application, argument, declaration, derivation, member,
  and role rows through `freeze_type_rows/2`.
- Extend transport erasure and frontier deduplication.
- Add `type_field/5` and `type_field_count/2` as semantic-owner compiler
  views.

### Phase 4: `Partial` proof and target lowering

- Implement the `Partial` compiler fixture using ordinary rules and request
  relations.
- Prove exact optionalized fields for a named relation with multiple fields.
- Prove the generated relation reaches SQLite, TypeScript, Rust, JSON Schema,
  and catalog output through existing lowerers.
- Prove nested and repeated applications deduplicate.

<!-- todo(feature): Lower functional type terms in compiler heads into explicit type_apply body goals. -->
<!-- todo(feature): Add demand-driven complete derived relation request rows with deterministic validation and named diagnostics. -->
<!-- todo(feature): Materialize validated compiler-derived relation shapes through existing carriers and bounded refreeze. -->
<!-- todo(feature): Prove Partial(User) through canonical rows, compiler reflection, transport erasure, and cross-target monomorphization. -->

## Verification

### Focused compiler tests

- `Partial(Source)` in a `type`-typed head column lowers to one
  `type_apply/3` goal after the body goals binding `Source`.
- Nested `option(MemberType)` lowers inside-out and creates one structural
  option application per distinct member type.
- A capitalized literal constructor resolves by expected type position.
- A variable constructor remains refused under the higher-kinded deferral.
- `type_requested/3` contains `Partial(User)` when that application is authored
  or requested and does not enumerate `Partial` over unrelated declarations.
- `Partial(User)` returns exactly
  `application(PartialConstructorId, [UserTypeId])`.
- The complete request creates one materialized declaration and one
  `derived_from(MaterializedId, ApplicationId)` row.
- Compiler `type_field/5` can query generated members using the application
  identity and returns semantic value TypeIds.
- Compiler `type_field_count/2` returns exact named and application member
  counts, including zero-member relations.
- Source member order and names are preserved.
- Source key roles are absent from `Partial(User)`.
- Zero-member relations construct successfully from `MemberCount = 0`.
- Duplicate identical rows deduplicate.
- Conflicting header, count, position, name, type, and role requests produce
  exact named diagnostics.
- Non-ground and incomplete requests produce exact named diagnostics.
- Unrelated source declaration and rule reordering preserves every semantic ID
  and canonical row.
- Repeated and nested requests reach a stable canonical row set.
- A mapped constructor does not generate `Partial(Partial(...))` without
  explicit nested demand.
- Round-limit and recursive-construction diagnostics remain reachable.

### Existing behavior rails

- Existing generic template applications retain their exact application and
  materialization identities.
- Existing anonymous products, enums, options, annotations, interfaces, and
  compiler relations retain their focused snapshots.
- Existing `type_apply` tests retain arity, unknown constructor, non-ground,
  recursive construction, identity reuse, refreeze, and erasure behavior.
- Programs without derived request rows emit byte-identical runtime plans and
  type artifacts.

### Cross-target proof

One fixture emits `Partial(User)` through:

```text
canonical semantic row snapshot
compiler reflection closure
TypeScript type golden
Rust type golden
JSON Schema golden
SQLite DDL or catalog storage snapshot
```

The fixture asserts complete output snapshots rather than presence-only checks.

### CI

- Run focused compiler relations, type relation IR, semantic type identity,
  generic expansion, anonymous type, annotation surface, and typegen suites
  during implementation.
- Run the complete Prolog compiler suite after the integrated implementation.
- Run the existing TS and Rust generated-artifact gates for the cross-target
  fixture.
- Report which tests add or change CI coverage. Formatter and linter results
  are outside CI reporting.

## Staffing

- Implementer: Terra xhigh in one isolated Boop worktree.
- Dispatch: only after user review of this plan.
- Base SHA: resolve at dispatch; planning workspace HEAD was
  `3b2064aaf09db41d56c3eeffbef58ab8c06adb2b`.
- Ownership: compiler lowering, request validation, canonical materialization,
  focused tests, cross-target proof, and issue-card updates remain in the same
  lane so the semantic contract does not split across agents.
- Review: independent review after Terra reports a clean worktree, exact diff,
  focused receipts, and full compiler CI.
- Suite budget: focused suites after each phase, one complete Prolog compiler
  run after integration, and one TS plus one Rust cross-target closeout.
