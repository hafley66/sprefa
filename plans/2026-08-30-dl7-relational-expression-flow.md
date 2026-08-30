# DL7 relational expression flow

## Context

V7 currently parses every parenthesized form uniformly, then narrows the
semantic surface during lowering. Bind targets accept products, sums, bare
references, and literals; another form stops as `unsupported_bind_target` in
`v7/src/2_comptime/0_lowerer.pl`. Call arguments similarly stop as
`nested_call_argument`.

The programmable compiler branch already proves the harder downstream pieces:

- relation calls are checked with functional keys and authored-order modes;
- the shared evaluator supports positive recursion, stratified negation,
  completed-stratum count, reversible bounded `cons/3`, and tabling;
- compiler rounds freeze type edges and interned applications;
- `Partial`, `Pick`, and `Exclude` derive type graph rows in userland;
- generated `def/head/head_arg/body/body_arg` rows become checked executable
  rules in a later compiler round;
- `HistoryV1` generates both a relation shape and executable behavior.

The remaining surface gap is the one discussed in favorites 26 through 37:
an expression-position relation call omits one declared result column, lowering
inserts a fresh logical variable in that column, and the surrounding construct
uses that variable. A type expression is an ordinary relation expression whose
result column has type `type`.

The current `partial_request/1` fixture is temporary scaffolding. The word
`request` has no place in the language model. An occurrence of `(Partial User)`
is sufficient to drive `Partial(User, Result)`.

## Decisions

1. A full relation call always retains every tuple column. It remains available
   for forward, reverse, and mixed-mode queries.
2. An expression-position call may omit exactly one column labeled `return` in
   the callable's declared product shape.
3. The `return` label controls surface projection only. The checked relation
   remains symmetric relational data.
4. Expression lowering has one result shape:

   ```prolog
   lower_expression(+Node, +Owner, +Environment,
                    -Value, -Goals, -Origins, -Diagnostics).
   ```

5. Atom references and literals produce a value and zero goals. A call
   recursively lowers its operator and arguments, inserts one fresh variable
   at the declared return position, and appends one ordinary relation goal.
6. Every nested value position uses the same lowering. Generated goals are
   hoisted into the nearest enclosing rule body in dependency order.
7. A top-level bind with generated goals becomes a generated compiler rule
   deriving the canonical `:/4` edge. A bind with zero goals remains an
   authored edge fact.
8. Relation declarations remain the source of callable arity, return position,
   argument order, argument types, and functional keys. There is no separate
   type-application grammar or evaluator.
9. Expression use initially requires the supplied argument positions to
   functionally determine the return position. Full relation calls retain
   nondeterministic and reverse-query use.
10. A call with fewer supplied inputs than the selected callable mode produces
    a canonical partial-application value. Compile-known later application is
    expanded to a direct first-order relation call before checked Datalog.
11. Edge labels are semantic values. Lexical scope bindings continue to require
    a resolvable atom label; ordered product and sum edges may use a ground
    compound label produced by ordinary expression lowering.
12. `Key` will exercise compound edge labels. Its options and field identity
    remain on the literal edge, and rules can select keyed edges by matching the
    label value.

Rejected alternatives:

- User-authored `*_request` relations duplicate application occurrences.
- A dedicated `type_apply` surface creates separate value and type realities.
- Treating an expression result as exactly one runtime row discards ordinary
  relational cardinality.
- Dynamic higher-order dispatch in checked runtime Datalog prevents direct SQL
  lowering; the first partial-application slice is compile-known and erased.
- Inferring callables from unresolved names invents declarations.

## Ten milestones

### 1. Expression result carrier

Add the internal `Value + Goals + Origins` carrier and exact tests for atoms,
literals, variables, and a rejected unresolved form. Keep it inside the
existing numbered comptime files unless a file crosses the repository's hard
size boundary.

### 2. Declared return position

Read one edge labeled `return` from the callable declaration. Emit diagnostics
for zero or multiple return edges when the relation is used as an expression.
Full relation calls require no return edge.

### 3. RHS call lowering

Lower:

```lisp
(: UserPatch (Partial User))
```

into one ordinary `Partial(User, Result)` body goal and one derived `:/4` head.
The source name binds to the returned type identity.

### 4. Nested applications

Lower:

```lisp
(: MaybePatch (Option (Partial User)))
```

into ordered flat goals:

```text
Partial(User, PartialUser)
Option(PartialUser, MaybePatch)
```

### 5. Uniform nested positions

Run the same expression lowerer for arguments nested in rule heads and body
goals. Hoist generated goals into the containing body and retain Datalog head
safety.

### 6. Remove `partial_request`

Delete the declaration, seed fact, prelude dependency, fixture dependencies,
and test wording. The authored `(Partial User)` occurrence becomes the sole
construction root.

### 7. Reverse-query parity

Add one full-tuple query that binds the source from a known result and prove it
uses the same `Partial/2` relation. Expression lowering must not rewrite full
arity calls.

### 8. Expression mode and cardinality checks

Use checked relation key sets to prove that supplied positions determine the
declared return position. Emit one positioned diagnostic for an ambiguous
expression mode. Preserve zero-or-many answer behavior for explicit full calls.

### 9. Compile-known partial application

Represent an unsaturated call by canonical callable identity plus ordered bound
arguments. Applying the value later appends arguments and emits a direct full
relation goal. Add one curried two-input generic fixture and prove the final
checked program contains no dynamic application operator.

### 10. Compound edge label and `Key` proof

Permit a ground expression value in an ordered type edge's label slot. Define
one userland `Key` constructor with typed options, attach its value to a literal
edge, and derive the ordered composite key rows by matching all keyed edges of
the closed owner.

## Instance timelines

### Complete expression application

```text
reader form
    -> resolve callable and declaration
    -> lower nested argument expressions
    -> insert fresh return variable
    -> emit flat ordinary goal
    -> surrounding bind/head/body consumes the variable
    -> checker validates arity, keys, mode, and safety
    -> evaluator derives zero or more rows
```

### Type-producing application

```text
(Partial User)
    -> Partial(User, Result)
    -> intern(Partial, [User], Result)
    -> freeze Result application identity
    -> later round derives Result edges
    -> bind edge points at Result
```

### Partial application

```text
(Pair User)
    -> canonical partial Pair+[User]
    -> later apply Order
    -> Pair(User, Order, Result)
    -> partial carrier erased before checked runtime Datalog
```

## Storage, reads, writes, and uniqueness

- Reader nodes and fresh lowering variables live for one compilation.
- Full calls write no expression objects. They become ordinary checked goals.
- Canonical type applications retain the existing functional dependency
  `(Constructor, Arguments) -> Result`.
- Partial applications use `(Callable, ordered BoundArguments) -> PartialId`.
- Lexical bindings remain unique by `(Owner, AtomLabel)` and ordered by
  `(Owner, Index)`.
- Compound type-edge labels remain unique under the existing edge key policy;
  the key proof must state whether options participate in edge identity.
- Generated bind rules enter the existing bounded refreeze timeline and leave
  no second compiler transport relation.

## Verification

- Keep one consolidated V7 PLUnit file and add named cases rather than one file
  per syntax fragment.
- Keep the Tree-sitter corpus consolidated; its generic nested-form grammar
  should need only expectation updates if token syntax is unchanged.
- Run focused SWI after each milestone cluster.
- Run the complete V7 SWI and Tree-sitter gates before each integration commit.
- Assert exact normalized goals, return-variable sharing, diagnostics, compiler
  closure rows, and absence of `partial_request` and residual dynamic apply.
- Record test counts and wall time in `v7/tasks/00_PROGRESS.md`.

## Staffing

- Base: `606379b98` on `feature/dl7-count-aggregate`.
- One Terra design/blast-radius review, worktree yes, no implementation edits.
- One Luna implementation lane for milestones 1 through 4 after review.
- Milestones 5 through 8 remain with the coordinating high-reasoning agent
  because they alter rule safety and mode checking.
- Milestones 9 and 10 receive separate bounded briefs after the first eight
  pass the full V7 gate.
- Suite budget per implementation lane: focused SWI up to four runs, complete
  V7 SWI once, Tree-sitter once.

<!-- todo(feature): Complete milestones 1 through 4: expression carrier, declared return position, RHS calls, and nested applications. -->
<!-- todo(feature): Complete milestones 5 through 8: uniform nested positions, removal of partial_request, reverse-query parity, and expression mode checks. -->
<!-- todo(feature): Complete milestone 9: compile-known partial application erased before checked runtime Datalog. -->
<!-- todo(feature): Complete milestone 10: compound edge labels and the userland Key proof. -->
