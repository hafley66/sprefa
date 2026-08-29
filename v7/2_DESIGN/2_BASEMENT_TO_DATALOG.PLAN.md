# DL7 basement: root Lisp to checked Datalog

Status: implementation plan.

The basement is every compiler layer before comptime fixpoint goals execute.
This slice ends with a checked, stratified Datalog program. Evaluation,
interning, type construction, compiler effects, runtime execution, and engine
emission begin after this boundary.

```text
.dl7 text
    -> canonical root datums
    -> nested bind, product, and sum graph
    -> resolved relation, seed, and rule lowering
    -> safety and dependency graph
    -> checked Datalog program
    -> [later] comptime fixpoint goals
```

## Surface covered

The existing prefix reader remains the only text path.

```lisp
; explicit relation declarations, with no `rel` form
(: edge
  (* (: from text)
     (: to text)))

(: reachable
  (* (: from text)
     (: to text)))

; nested binds create nested owners/scopes
(: result
  (+ (: ok  (* (: value text)))
     (: err (* (: message text)))))

; ground facts
(edge "a" "b")
(edge "b" "c")

; rules
(<- (reachable ?from ?to)
    (edge ?from ?to))

(<- (reachable ?from ?to)
    (edge ?from ?via)
    (reachable ?via ?to))
```

The basement recognizes only these semantic top-level forms:

```text
(: Name Target)                  bind Name to Target in the current owner
(* Binding...)                   product owner and ordered bind list
(+ Binding...)                   sum owner and ordered bind list
(Relation Argument...)           ground seed or relational goal by position
(<- Head BodyGoal...)            positive rule
```

Every `:` installs one ordered edge in its enclosing owner. A nested `*` or
`+` creates another owner, and its nested binds install edges there. Each such
owner has a `scope_parent/2` edge to its enclosing owner. Future dot access can
resolve one segment by reading `':'(Owner, Name, Target, Index)` and continue
from `Target`; no dot token is added in this slice.

The basement preserves target references and does no type evaluation. `->`,
nested expression application, partial application, imports, negation,
aggregates, interning, and compiler effects remain outside these three
milestones.

The four datum roles remain distinct across the boundary:

```text
atom(Name)       unresolved reader spelling
ref(Target)      resolved semantic name
var(Identity)    logic variable in a Datalog rule
const(Value)     integer, string, or symbol data
```

An atom becomes a reference only in a reference-bearing syntactic position.
A symbol literal always becomes `const(symbol(Name))`. Punning is reserved as
a later syntax expansion from `(: Name)` to an explicit bind whose edge label
and target reference are both retained. The canonical basement always carries
both fields, so punning will not require a second edge representation.

## Milestone 1: canonical root datums

The existing reader already owns atoms, variables, integers, strings, forms,
comments, source rows, and deterministic diagnostics. Complete the datum
contract with symbol literals.

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics).

% Read bare identifiers as atom(Name), for later positional resolution.
% Read 'name as literal(symbol(Name)), which never enters name resolution.
% Preserve empty and nested forms as form(Children).
% Preserve one explicit VariableIdentity for equal ?name occurrences inside
% one top-level form; each ?_ remains fresh.
```

### Lifetime

1. Reader node and source identities live for one compilation.
2. Variable identities survive through Datalog lowering.
3. Atom spellings remain unresolved until their syntactic position is known.
4. Symbol literals are data immediately and never become references.

### Storage and uniqueness

- `reader_node(Path, PreorderIndex)` keys reader nodes and source rows.
- A top-level form is the variable-sharing region.
- No new parser module or second AST is introduced.

### Verification

Extend the existing reader fixture and existing snapshot test. Add no test
file. Run the focused SWI reader command once.

## Milestone 2: lower nested binds and root forms

Add one production module in dependency order:

```text
v7/1_DATALOG/0_lower.pl
```

Public signature:

```prolog
lower_datalog(+Unit, -Program, -Origins, -Diagnostics).

% Pass 1: mint the file owner plus every nested product and sum owner.
% Pass 2: reserve every ':' name in its owner before resolving targets.
% Pass 3: emit pending bind edges, scope parents, relation declarations,
% ground seeds, and positive rules in authored order.
% Reify each reader VariableIdentity as var(Identity).
% Preserve source ownership in ground origin rows.
% Return no partial Program when diagnostics exist.
```

Canonical output:

```prolog
basement_program(
  root_graph(
    [node(NodeIdentity, module | product | sum), ...],
    [scope_parent(ChildOwner, ParentOwner), ...],
    [pending_edge(Owner, Name, TargetTerm, Index), ...]
  ),
  datalog_program(
    [relation(RelationTarget, Arity), ...],
    [call(PendingRelationReference, [const(Value), ...]), ...],
    [rule(call(PendingRelationReference, [var(Id), ...]),
          [call(PendingRelationReference, [var(Id) | const(Value)]), ...]),
     ...]
  )
).
```

`TargetTerm` is `name(CurrentOwner, Name)` for a bare target atom,
`target(NodeIdentity)` for a nested `*` or `+`, or `const(Value)` for literal
data. `PendingRelationReference` is `name(FileOwner, Name)`. This milestone
records every bind and call site without resolving or evaluating it.

Every named product is a relation-shaped node. Its outgoing colon edges are
its ordered tuple columns, while the same edges remain available to later type
checking. A named sum remains a type-shaped node whose outgoing edges name its
variants. No separate member or column-edge relation is introduced.

Provenance is separate ground data:

```prolog
origin(node(NodeIdentity), ReaderNodeId).
origin(edge(Owner, Name, Index), ReaderNodeId).
origin(relation(RelationTarget), ReaderNodeId).
origin(seed(SeedIndex), ReaderNodeId).
origin(rule(RuleIndex), ReaderNodeId).
origin(goal(RuleIndex, GoalIndex), ReaderNodeId).
```

The lowerer requires explicit binds. A use does not invent a relation. Every
owner reserves all of its bind names before targets and calls are resolved, so
source order does not constrain recursion, forward references, or sibling
references.

### Lifetime

1. Reservation rows exist only during `lower_datalog/4`.
2. Program and origin rows survive through static checks and evaluation.
3. Reified `var(Identity)` terms are ground and survive serialization.

### Storage and uniqueness

- Owner identity is derived from the immutable unit identity and reader node.
- `(Owner, Name)` and `(Owner, Index)` are unique bind keys.
- Nested owners carry exactly one `scope_parent/2` row.
- Relation arity is the count of outgoing binds on its product target.
- Seeds are retained in authored order until the checker canonicalizes rows.
- Rules and body goals retain authored order.

### Verification

Use one direct SWI receipt over an in-memory unit or the existing loader. Add
no test file and run no V6, Rust, TypeScript, or engine suite.

## Milestone 3: resolve, check, and graph the Datalog program

Add one production module after lowering:

```text
v7/2_DATALOG/0_check.pl
```

Public signature:

```prolog
check_datalog(+BasementProgram, +Origins, -Checked, -Diagnostics).

% Resolve name(Owner, Name) locally, then through scope_parent/2.
% Resolve int, text, any, and type through the primitive root.
% Replace successful name terms with ref(Target) and pending edges with ':'/4.
% Check unique binds and dense zero-based indices.
% Check every call against a resolved product relation and arity.
% Require seeds to be ground.
% Require every head var(Identity) to occur in a positive body call.
% Emit the positive dependency graph and SCC strata.
% Sort diagnostics by origin and return no Checked value on failure.
```

Successful output:

```prolog
checked_datalog(
    root_graph(Nodes, ScopeParents, ColonEdges),
    datalog_program(Relations, Seeds, Rules),
    [depends(HeadRelation,
             BodyRelation,
             positive), ...],
    [stratum(Relation, NonNegativeInteger), ...]
).
```

Resolved calls are ground compiler data:

```prolog
call(ref(RelationIdentity), [var(Identity) | const(Value)]).
```

This representation keeps relation identity separate from source spelling and
keeps logical variables explicit. The later evaluator performs unification
over these reified `var/1` terms.

All dependencies in this slice are positive. Mutually recursive relations
share one strongly connected component and therefore one stratum. With no
negative edge, every component may have stratum zero; the explicit rows keep
the later negation extension data-shaped.

### Lifetime

1. Check indexes and graph worklists live for one `check_datalog/4` call.
2. `checked_datalog/3` is the input contract for a later evaluator.
3. Diagnostics retain reader origins without retaining temporary indexes.

### Storage and uniqueness

- One dependency row exists per distinct `(HeadRef, BodyRef, Sign)` tuple.
- One colon edge exists per distinct `(Owner, Name, Index)` tuple.
- One stratum row exists per resolved product relation.
- Checked rows are sorted by standard term order for deterministic receipts.

### Verification

Use one direct SWI receipt proving nested product and sum edges, parent-scope
resolution, a recursive dependency graph, an undeclared relation, an arity
mismatch, and an unsafe head variable. Add no test file. Run no V6, Rust,
TypeScript, or engine suite.

## File and test ceiling

These three milestones may change or add only:

```text
v7/0_SWIPL/0_README.md
v7/0_SWIPL/1_reader.pl
v7/0_SWIPL/test/0_reader.test.pl
v7/0_SWIPL/test/fixtures/0_minimal.dl7
v7/1_DATALOG/0_lower.pl
v7/2_DATALOG/0_check.pl
v7/3_TASKS/00_PROGRESS.md
```

No new test file is allowed. Production modules target 300 nonblank,
noncomment lines and stop before 500.

## DAG

```text
root datums
    -> Datalog lowering
        -> Datalog checks and dependency graph
            -> [later] comptime fixpoint evaluator
```

Each milestone lands as its own reviewed commit before its dependent worker is
spawned. A worker stops and reports when source syntax, semantic identity, or
an out-of-scope construct requires a new ruling.
