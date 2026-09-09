# From relation application to a programmable type kernel

Date: 2026-08-25

Status: design walkthrough and conversation ledger. This document separates
current implementation facts from the emerging language model. Syntax marked
as proposed is exploratory and has not been approved as a parser change.

Related implementation reference:
[`0_graph-and-requests.md`](0_graph-and-requests.md).

Related active plan:
[`2026-08-25-relational-semantic-planes.md`](../../../plans/2026-08-25-relational-semantic-planes.md).

## Reading map

This discussion began with history and event relations, then uncovered a more
general compiler foundation. The path was:

```text
history annotations
  -> annotations are ordinary relations
  -> types must be queryable compiler data
  -> type constructors must be callable in compiler rules
  -> functional terms in rule heads need relational lowering
  -> generated type shapes need a bounded compiler fixpoint
  -> rel, type, scope, and module share one graph model
  -> type application may be ordinary relation application
  -> compile time may run through the ordinary DL6 evaluator
  -> prefix notation exposes the small underlying syntax kernel
```

Four labels are used throughout:

- **Implemented** means behavior exists in the current worktree.
- **Held direction** means the conversation converged on the model, while some
  representation or syntax work remains.
- **Proposed syntax** means a concrete spelling used to inspect the model.
- **Open** means a design contract still has a tracked ruling or implementation
  card.

## 1. The original pressure: history as ordinary relational structure

The initial goal was to replace special temporal declaration syntax such as:

```dl6
log keep(all).
```

with ordinary declarations and compile-time rules. A history relation needs
stable entity identity, a per-identity sequence or version, transaction time,
and retention semantics. An event relation needs occurrence identity. A state
relation needs replacement identity. Those concepts should be queryable as
data instead of hidden inside one emitter.

The discussion quickly reached a simpler observation. A log relation can be
modeled as a relation whose key includes time or sequence. A target such as
SQLite or PostgreSQL can realize the resulting logical requirements with
ordinary tables, keys, indexes, and companion artifacts. Therefore `log`,
`keep`, `history`, and `event` need not begin as compiler keywords.

They can begin as relations over compiler-visible type and flow facts:

```dl6
history(?Relation).
event(?Relation).
retention(?Relation, all).
```

That route requires user code to inspect relation declarations and derive new
compiler facts. The missing foundation was a programmable compile-time type
graph.

## 2. The kernel domain is `TypeId`

The emerging model has one identity domain for types:

```text
TypeId
```

A type can be viewed through several classifiers:

```text
TypeId
  + product-shaped
  + sum-shaped
  + namespace-shaped
  + generic-parameter-shaped
  + generic-specialization-shaped
  + anonymous-shaped
  + primitive-shaped
```

These classifiers do not require separate identity systems. They are ordinary
facts about one node.

```dl6
product(?Type).
sum(?Type).
namespace(?Type).
primitive(?Type).
```

The type algorithms consume `TypeId` regardless of classifier. Primitive
classification matters to boxing, codecs, storage, and target lowering. Graph
projection and unification still see a type node.

This resolves the earlier circular wording:

```text
a rel is a type
a type is a rel
an edge is a rel
```

The useful mechanical statement is narrower:

```text
a declared relation owns a TypeId
a relation-shaped TypeId owns ordered outgoing edges
a primitive-shaped TypeId is a leaf for structural projection
every compiler fact, including edge facts, is represented by a relation row
```

"Everything is a relation" therefore describes representation and
programmability. It does not require every node to have outgoing fields.

## 3. A relation is a named edge set, a scope, and a collection

The conversation collapsed `scope`, `module`, and `relation` in the current
language model.

```text
rel = named type node + lexical scope + ordered edge set + runtime collection
```

A file receives an implicit outer relation derived from its module path.
Declaring `User` adds a named edge from that file relation to the type denoted
by `User`.

```text
FileRel --User--> UserType
UserType --name--> text
UserType --age---> int
```

Nested declarations use the same operation:

```text
FileRel --User----> UserType
UserType --Address--> AddressType
```

Brace nesting therefore acts as a name prefix. It adds no implicit parent
column and performs no key shift. A nested relation that stores a parent
reference declares that edge explicitly.

Variables remain local rule binders. They can disappear during lowering and do
not need persistent graph identities unless future compiler reflection over
local variables is requested.

## 4. Edges are the primordial structure

The held public shape is:

```dl6
type.edge(
  ?Owner,
  ?Name,
  ?Target,
  ?Index
).
```

Semantically:

```text
Owner --Name--> Target
```

`Index` preserves authored order. The held logical identity is `(Owner, Name)`.
The edge row itself can be referenced by that key, so a synthetic public
`EdgeId` is unnecessary.

The current implementation still exposes compatibility rows with more fields:

```dl6
type.node(Id, Kind, Label).
type.edge(Edge, Owner, Role, Position, Label, Target).
type.member(MemberId, Owner, Position, Name, Target).
```

The open `@type-edge-view` card owns the public four-column edge relation and
the migration from the current carriers. The word `member` remains an
implementation compatibility name. The language model treats a field as a
named edge.

Classifications such as field, variant, nested declaration, constructor,
argument, return, and annotation can be represented as ordinary relations over
nodes or keyed edges. Their exact public signatures remain part of the
semantic-plane rulings.

## 5. Name binding, projection, and edge reference

Three operations emerged:

```text
bind      create a named edge in the current owner
project   follow a named edge and return its target
edge-ref  return the named edge row itself
```

Candidate punctuation:

```text
:    bind
.    project
::   edge-ref
```

Examples:

```dl6
User: UserShape
User.name
User::name
```

Graph interpretation:

```text
current scope --User--> UserShape
UserShape     --name--> text
```

```text
User.name   evaluates to text
User::name  evaluates to the keyed edge row (UserShape, name)
```

One label is an atom:

```text
name
```

A path is an ordered list of labels:

```text
User.address.city
[User, address, city]
```

Nested projection is left-associated:

```text
User.address.city
(. (. User address) city)
```

This distinction matters. A label does not need to be a list. Namespace and
projection paths are lists of labels. A compiler can intern the complete path
for canonical identity while retaining each segment for graph traversal.

## 6. Relation outputs already live in the tuple

The application discussion remained confusing until the arrow was reduced to
ordinary relation columns.

Consider a value-level relation:

```dl6
add: (?Left: int, ?Right: int) -> int.
```

Its underlying relation has three columns:

```text
add(Left, Right, Result)
```

A rule derives the entire tuple:

```dl6
add(?Left, ?Right, ?Result) <-
  ?Result := ?Left + ?Right.
```

Expression syntax introduces a fresh output variable:

```dl6
add(1, 2)
```

lowers to:

```dl6
add(1, 2, ?Result)
```

The surrounding expression consumes `?Result`.

The arrow therefore marks output positions for expression lowering:

```text
inputs -> outputs
```

Datalog still stores and derives one relation tuple:

```text
relation(input columns..., output columns...)
```

This is the key to unifying value-level and type-level calls.

## 7. Type application is relation application in type position

The intended model gives `type apply` no separate surface meaning.

```text
type application = ordinary relation application whose result is typed `type`
```

For example:

```dl6
Option: (?Element: type) -> type.
```

The expression:

```dl6
Option(int)
```

lowers to:

```dl6
Option(int, ?ResultType)
```

Compare value and type calls:

```text
add(1, 2)     -> add(1, 2, ?Value)
Option(int)   -> Option(int, ?Type)
```

The declaration controls the output domain:

```text
?Value : int
?Type  : type
```

Replacing generic angle brackets with parentheses enables one parser node:

```text
Option<int>   old type-specific grammar
Option(int)   uniform application grammar
add(1, 2)     uniform application grammar
```

All three can parse as:

```text
Call(Constructor, Arguments)
```

The current compiler still contains specialized `type_apply/3` machinery.
That implementation performs canonical type identity construction and
participates in the outer refreeze loop. The held direction treats it as
internal lowering machinery rather than a distinct language concept.

## 8. Why the word "application" caused trouble

The discussion used one word for three objects:

```text
written call       key(int)
specialized node   the canonical node representing key applied to int
return facade      the type exposed by that specialization
```

Use these terms instead:

```text
constructor       key
arguments         [int]
specialization    key(int) as a canonical TypeId
return facade     int
```

For an annotation wrapper:

```text
User --id--> key(int) --return--> int
                 |
                 +--constructor--> key
                 +--argument[0]---> int
```

Preserving the middle node lets compiler rules distinguish:

```dl6
id: int
```

from:

```dl6
id: key(int)
```

Both may expose `int` as the runtime value facade. The specialization node
retains the `key` annotation for integrity rules.

## 9. Canonical identity and interning

The compiler needs one stable identity for repeated construction of the same
specialization:

```text
(Constructor, ordered Arguments) -> TypeId
```

Example:

```text
(key, [int]) -> K17
```

Every occurrence receives the same ID:

```text
first key(int)  -> K17
second key(int) -> K17
third key(int)  -> K17
```

The logical relation is a uniquely keyed dictionary:

| Constructor | Arguments | TypeId |
|---|---|---|
| `key` | `[int]` | `K17` |

Three implementation representations are possible:

1. A structural term such as `application(key, [int])` serves directly as the
   semantic ID.
2. A canonical serialization or content hash of constructor and arguments
   serves as the semantic ID.
3. A surrogate ID is minted under a uniqueness constraint on constructor and
   arguments.

The semantic functional dependency is identical in all three cases. The final
public TypeId representation remains an open identity ruling.

`intern(?Constructor, ?Arguments, ?TypeId)` was used in the discussion to name
this kernel operation. It should not necessarily appear in authored DL6.
Programmers should be able to write `Option(int)` while the compiler owns the
stable identity operation, just as Zig owns its internal compiler `Type`
objects.

## 10. Generic substitution is ordinary variable flow

One attempted explanation used this rule:

```dl6
Option(?Element, ?Specialization) <-
  specialize(Option, [?Element], ?Specialization).
```

That rule is circular. It moved the unexplained operation into another
predicate.

A generic can instead be understood as an ordinary relation from type inputs
to a type output. Its body derives the returned shape.

Conceptual prefix form:

```lisp
(: Option
  (->
    ((: element type))
    type))

(<-
  (Option ?Element ?Result)
  (sum
    ((: none unit)
     (: some ?Element))
    ?Result))
```

Calling:

```lisp
(Option text)
```

lowers to:

```lisp
(Option text ?Result)
```

Ordinary relation binding gives:

```text
?Element = text
```

The body then passes `text` to `sum`:

```lisp
(sum
  ((: none unit)
   (: some text))
  ?Result)
```

The apparent substitution comes from variable binding and ordinary argument
passing. A type expression is an ordinary expression whose output column has
type `type`.

## 11. Inline type expressions use ordinary expression lowering

Proposed infix form:

```dl6
User: (
  id: key(int),
  nickname: Option(text)
).
```

Proposed prefix form:

```lisp
(: User
  (*
    (: id (key int))
    (: nickname (Option text))))
```

Conceptual lowering first obtains outputs from the nested calls:

```lisp
(key int ?KeyInt)
(Option text ?OptionText)
```

Then it builds the containing product:

```lisp
(*
  ((: id ?KeyInt)
   (: nickname ?OptionText))
  ?UserShape)
```

The right-hand side receives no substitution privilege. Every nested type
expression follows the same call-to-output-variable lowering used by ordinary
value expressions.

## 12. Output cardinality is the remaining application contract

Expression position expects a usable result. Relations can produce zero, one,
or many matching rows. The open `@relation-application-semantics` card must fix
the contract:

```text
zero matching output rows   no result diagnostic or empty relation result
one matching output row     scalar expression result
many matching output rows   relation-shaped result or ambiguity diagnostic
```

Functional dependencies and determinism declarations can prove the one-result
case. Mercury's mode and determinism system is relevant prior art. DL6's clock
and cardinality prover may eventually feed the same proof boundary.

Partial application also remains open. The current plan requires either an
explicit syntax and identity rule or a deterministic unsupported diagnostic.

## 13. Unification is matching tuple positions

With explicit variables:

```dl6
copy(?Value) <-
  source(?Value).
```

The variable `?Value` receives the value found in the matching `source` row.
The same variable in the head copies that binding into the derived `copy` row.

Repeated variables assert equality:

```dl6
same(?Value) <-
  pair(?Value, ?Value).
```

Only rows with equal first and second values satisfy the rule.

Nested type patterns use the same idea after lowering. A pattern such as:

```dl6
serializable(primitive(?Kind))
```

is lowered into finite graph lookups that bind `?Kind`. A constructed head term
is lowered into explicit construction or canonicalization goals after its
variables have been bound by positive body goals.

Implemented safety rules include:

- Head variables must be bound by positive body goals.
- Type construction receives ground constructors and arguments.
- Body patterns match finite compiler rows.
- Unsafe variable-bearing bare facts are rejected.
- Recursive construction is checked and bounded.

## 14. Datalog, Prolog, and the selected middle ground

The relevant difference is domain construction and search behavior.

Traditional Datalog works over a finite set of constants and derives a finite
set of relation tuples. Function symbols are commonly excluded because terms
such as:

```text
f(a)
f(f(a))
f(f(f(a)))
```

can expand the domain forever.

Prolog permits compound terms, general unification, depth-first search,
backtracking, and host predicates. Those facilities can express more programs
and can diverge through search or recursive term construction.

DL6 currently selects a bounded relational subset:

```text
range-restricted rules
set-oriented compiler closure
tabled recursive evaluation
stratified negation
aggregates after their dependencies
ground canonical type construction
bounded generated-type refreeze
```

Functional terms in rule heads are accepted as surface syntax and lower into
ordinary relational goals. This keeps the evaluator's fixpoint model visible
while retaining compact structural syntax.

## 15. The compiler has two fixpoints today

Two loops must remain conceptually distinct.

### 15.1 Inner compiler-relation fixpoint

```text
seed compiler facts
  -> match and unify rules
  -> derive new facts
  -> add previously unseen facts
  -> repeat until no new facts exist
```

This is ordinary recursive relation closure. It handles projection,
reachability, transitive `extends`, transitive `impl`, blocked serialization
reachability, stratified complements, grouped counts, and related compiler
queries.

### 15.2 Outer generated-type refreeze

```text
freeze canonical type snapshot N
  -> run compiler-relation fixpoint
  -> collect complete generated-type requests
  -> validate shape, names, order, roles, and groundness
  -> materialize newly requested declarations
  -> freeze canonical type snapshot N+1
  -> repeat until requests and canonical rows stop changing
```

The current implementation limits this loop to 16 rounds and emits a named
diagnostic when the bound is exhausted.

The outer loop exists because generated type nodes currently become canonical
declaration carriers between compiler rounds. If the type graph itself becomes
the authoritative compiler data model, part of this cycle may become ordinary
fact closure. Canonical domain construction still needs a termination policy.
The choices include demand restriction, bounded rounds, a decreasing structural
measure, or a formally terminating chase fragment.

## 16. Compile time and runtime can use the same evaluator

The semantic algorithm can be identical:

```text
COMPILE TIME                     RUNTIME

source and type facts            application facts
          \                         /
           \                       /
            same DL6 evaluator
       join + unify + negate + aggregate
                    |
                 fixpoint
            /                 \
   type and plan facts      application rows
```

The phase changes the seeded relations and retained outputs. The relational
algorithm can remain the same.

The current implementation is mechanically split:

```text
compile time   specialized tabled evaluator in Prolog
runtime        lowered target execution through SQL and Rust machinery
```

A unified executable path could compile the compiler's own DL6 libraries and
run them through the normal DL6 engine:

```text
source.dl6
  -> parse into compiler facts
  -> run compiler.dl6 with the ordinary engine
  -> derive type, integrity, flow, materialization, and target-plan facts
  -> emit the selected target program
```

Canonical type construction is the one extra pressure. General relation
construction with stable keyed outputs would make the facility available to
runtime and compile-time relations alike.

## 17. Zig correspondence

Zig treats `type` as a compile-time value:

```zig
fn Option(comptime T: type) type {
    return union(enum) {
        none,
        some: T,
    };
}

const OptionInt = Option(i32);
```

The DL6 correspondence is:

```text
Zig                                 DL6

comptime T: type                    ?T: type
function returning type             relation output typed type
@typeInfo(T)                         type.node/type.edge queries
construct struct or union            derive product or sum graph rows
compiler-owned Type object           canonical TypeId
```

Zig owns the internal identity and allocation of the returned `Type` object.
The user-level comptime function owns the shape computation. The proposed DL6
split is the same:

```text
kernel      stable TypeId and graph storage
user DL6    rules that derive and transform graph shape
```

DL6's relational representation makes compiler reflection queryable through
joins, recursion, grouping, negation, and fixpoint closure instead of Zig's
procedural `@typeInfo` inspection.

## 18. TypeScript correspondence

TypeScript writes generic type application with angle brackets:

```ts
type MaybeName = Option<string>
```

The uniform DL6 spelling is:

```dl6
Option(text)
```

Mapped types, conditional types, and recursive structural queries motivate the
same compiler-level operations:

```text
inspect fields
map each target type
construct a new product
select branches from type facts
recurse through a finite type graph
```

For example, a TypeScript-style `Partial<T>` maps each field target to an
optional target. DL6 already implements that transformation as compiler rules
over canonical type rows and generated relation requests.

Higher-kinded types would let a parameter range over constructors such as
`Option` instead of completed types such as `Option(int)`. The current
user-land operator experiments do not require that capability. Ordinary
relation application, type-valued outputs, and graph rules cover `Partial`,
`concat`, `extends`, `impl`, and recursive serializability.

## 19. Prefix notation as a semantic microscope

The prefix experiment is useful even if the final surface remains
Datalog-shaped. It exposes one general application tree.

Current-style declaration:

```dl6
User: (
  id: key(int),
  name: text
).
```

Prefix rendering:

```lisp
(: User
  (*
    (: id (key int))
    (: name text)))
```

Every construct is an operator followed by arguments:

```text
(: A B)             bind/2
(key int)           key/1
(type.edge A B C D) type.edge/4
(<- Head Body...)   rule/N
```

Using `*` for product makes the structural algebra visible:

```text
*   product
+   sum
```

Arithmetic is not the primary meaning in a type-expression context. The parser
can preserve the operator symbol and resolution can select the declared
relation by scope, arity, and argument domains.

## 20. `cons` and the list spine

A cons cell is a pair:

```text
cons(Head, Tail)
```

A linked list uses `Tail` as the next list:

```text
cons(1,
  cons(2,
    cons(3,
      nil)))
```

Lisp prints that structure as:

```lisp
(1 2 3)
```

The recursive type is:

```text
List(T) = Nil | Cons(T, List(T))
```

It parallels natural-number construction:

```text
Natural = Zero | Succ(Natural)
List(T)  = Nil  | Cons(T, List(T))
```

`Succ` adds one counting layer. `Cons` adds one element and link layer. A cons
cell can hold any tail, so `(cons 1 2)` is a dotted pair `(1 . 2)` rather than a
proper list.

The prefix call:

```lisp
(Option int)
```

has an underlying list representation approximately equivalent to:

```text
cons(Option,
  cons(int,
    nil))
```

The evaluator reads the head as the callable relation and the tail as the
ordered argument list.

## 21. Prefix grammar candidate

The smallest S-expression grammar is:

```ebnf
expression :=
    atom
  | variable
  | literal
  | "(" expression* ")"
```

Semantic cases:

```text
()               empty product or unit
(Foo)            Foo/0 application
(Foo A)          Foo/1 application
(Foo A B)        Foo/2 application
```

The operator is the first element. Arity is the number of remaining elements.

Proposed core forms:

```lisp
(: Name Target)                 bind a label in the current owner
(. Owner Name)                  project the target of a named edge
(:: Owner Name)                 refer to the keyed edge row
(* Field...)                    construct a product shape
(+ Variant...)                  construct a sum shape
(-> Inputs Outputs)             describe input and output columns
(<- Head Body...)               derive a rule-head tuple
```

These can all share the list parser. Some retain special static semantics
because they introduce bindings, scopes, or rule phases. Uniform syntax does
not require every operator to execute as an ordinary runtime relation.

## 22. Variables use a lexical `?` prefix

The selected variable direction is:

```text
?x      variable named x
x       symbol or resolved name x
'x      quoted symbol data x
```

`?` is a lexical sigil. It is not a unary runtime operator. The lexer can emit:

```text
?x -> variable(x)
x  -> symbol(x)
```

Most declarations contain no variables:

```dl6
User: (name: text).
```

Rules contain variables because they match and transfer unknown values:

```dl6
copy(?Value) <-
  source(?Value).
```

Prefix:

```lisp
(<-
  (copy ?Value)
  (source ?Value))
```

Explicit `?` frees capitalization for relation, type, constructor, and module
names. `User` can always resolve as a name. `?User` is always a variable.

Prior art includes Datomic and DataScript Datalog, SPARQL, and Notation3.
Souffle uses uppercase variables. Prolog and Mercury use uppercase or
underscore-prefixed variables. Flix uses lowercase variables and uppercase
predicate symbols.

## 23. Colon is a named-edge binder

Colon should not be named `var` in the semantic AST because its operands are
usually names and target expressions rather than logic variables.

Candidate semantic name:

```text
bind
```

or, when graph structure is the focus:

```text
label-edge
```

Surface:

```lisp
(: Name Target)
```

Explicit graph interpretation:

```lisp
(bind ?CurrentOwner Name Target)
```

The same rule handles module names and product fields:

```lisp
(: User UserShape)
(: name text)
```

```text
FileRel  --User--> UserShape
UserType --name--> text
```

## 24. Arrow as a labeled return edge

The latest prefix experiment lowered a relation declaration such as:

```dl6
User: (a: int) -> int.
```

to a product containing an explicit return edge:

```lisp
(: User
  (*
    (: a int)
    (: return int)))
```

Graph:

```text
scope --User--> UserShape
                   |
                   +--a------> int
                   +--return-> int
```

The equivalent constructor-oriented rendering is:

```text
label(
  User,
  product(
    cons(label(a, int),
      cons(label(return, int),
        nil))))
```

The Lisp reader hides the explicit cons spine. The semantic point is that the
return facade lives in the relation tuple and can be represented as a labeled
output edge. The exact handling of several output columns remains open under
`@relation-application-semantics`.

## 25. Annotations are ordinary relation applications

An annotation such as `key(int)` is a relation call in type position. The
resulting specialization remains visible as a type node so compiler rules can
recognize the annotation.

```text
User --id--> key(int) --return--> int
```

User-land integrity rules can inspect these applications and derive target
neutral facts:

```dl6
integrity.key(?Owner, ?Group, ?Edge, ?Index).
```

The target emitter then chooses how to realize those facts. SQLite may emit a
primary key clause. PostgreSQL may emit an equivalent constraint. Another
runtime may enforce the invariant through an index or specialized data
structure. The annotation library remains target neutral.

The same mechanism can express:

```text
optional
unique
reference
serializable
interned
event
history
retention
impl
extends
```

Each concept is an ordinary relation over type nodes, keyed edges, flow facts,
or materialization facts.

## 26. User-land type operators already demonstrated

The current compiler library implements these operators in DL6:

```text
Partial(Source)              map fields to option(FieldType)
concat(Left, Right)          concatenate ordered fields
extends(Child, Parent)       transitive relation
impl(Type, Interface)        transitive interface evidence
serializable(Type)           recursive closed-graph property
```

`Partial` and `concat` emit complete generated relation requests. The compiler
validates member count, contiguous positions, unique names, ground targets,
and roles before admitting the generated shape into the next refreeze round.

Recursive serializability uses a finite candidate graph. Supported leaves and
constructors are seeded. Unsupported nodes become blocked, and blocked status
propagates backward through field and application-argument edges. A final
stratified complement derives serializable nodes. Recursive strongly connected
components terminate because the algorithm computes reachability over a finite
candidate graph rather than constructing deeper type terms.

Implementation:
[`v6/dl/type/0_operators.dl6`](../../../v6/dl/type/0_operators.dl6).

## 27. Current compiler implementation

Implemented as of this document:

```text
canonical type nodes and typed edges
logical type.member/5 compatibility view
type.project/3 derived in DL6
deep dotted paths
brace nesting as name prefix
anonymous product and sum projection
functional type patterns in rule heads and bodies
compiler scalar expressions and grouped count
stratified compiler negation
tabled compiler fixpoint
bounded generated-type refreeze
Partial, concat, extends, impl, and serializable libraries
canonical storage projection
runtime rel/5 compatibility reconstruction
```

Current compatibility carriers include:

```text
semantic TypeId terms
legacy MemberId terms
application(Constructor, Arguments)
member_role(MemberId, Role)
keyed(RelationRef, Positions)
storage_relation/3
storage_column/2
storage_key/2
runtime rel/5 plans
```

These carriers are migration facts. They do not define the final public
language model.

## 28. Tests around functional type application

The current compiler test suite covers:

- Functional type terms in rule heads lower to explicit relational goals.
- Nested type expressions lower inside-out.
- Unknown constructors receive named diagnostics.
- Arity mismatches receive named diagnostics.
- Non-ground construction is refused.
- Recursive constructor cycles are refused.
- Existing applications reuse canonical identity.
- Missing closed applications create requests.
- A later refreeze exposes the generated type graph.
- Derived relation requests materialize complete ordered shapes.
- Compiler-only transport rows are erased before runtime planning.
- The 16-round construction bound receives a named exhaustion diagnostic.

Primary tests:
[`v6/prolog/compile/test/compiler_relations.test.pl`](../../../v6/prolog/compile/test/compiler_relations.test.pl)
and
[`v6/prolog/compile/test/compiler_relations/1_type_graph.test.pl`](../../../v6/prolog/compile/test/compiler_relations/1_type_graph.test.pl).

## 29. The four semantic planes

The type-kernel discussion now feeds a larger compiler separation:

```text
Type Graph
  nodes, edges, application, projection, classification
        |
        v
Integrity Graph
  keys, uniqueness, references, checks
        |
        v
Flow Graph
  occurrence, sign, delay, replacement, retention, clocks
        |
        v
Materialization Graph
  logical storage requirements and retained arrangements
        |
        v
Target Plan
  target names, capabilities, tables, code, and artifacts
```

This separation keeps SQLite, PostgreSQL, Rust, TypeScript, DBSP, memory, and
future emitters outside the target-neutral compiler relations.

The clock checker will eventually consume Flow Graph facts. CSP protocol
libraries can consume Flow and proven cardinality facts. Temporal annotations
can derive Integrity, Flow, and Materialization facts. Emitters consume those
facts after their logical contracts close.

## 30. Current task graph snapshot

The `@relational-semantic-planes` epic contains 28 child cards in the current
worktree snapshot: 12 done and 16 open.

The active dependency shape is:

```text
semantic-plane-rulings [L]
relation-application-semantics [L]
          |
          +------> type-edge-view [M]
          |               |
          +---------------+
                          |
              +-----------+-----------+
              |                       |
      integrity graph [M]       flow graph [L]
              |                       |
              |               clock projection [M]
              |                       |
              +------------> cardinality proof [L]
              |
              +----------+------------+
                         |
              materialization graph [L]
                         |
          +--------------+----------------+
          |              |                |
      temporal [M]     CSP [M]      target emitters
          |                               |
   remove old syntax [S]          retire special cases [M]
          \_______________________________/
                          |
                      goldens [S]
```

Open cards:

```text
semantic-plane-rulings
relation-application-semantics
type-edge-view
userland-integrity-graph
sqlite-integrity-emitter
userland-flow-graph
clock-flow-projection
flow-cardinality-proof
userland-materialization-graph
userland-temporal-annotations
remove-temporal-suffix
quoted-sqlite-storage-names
csp-protocol-library
retire-type-specialcases
relational-semantic-planes-golden
inferred-idb-type-reflection
```

Undeclared IDB type reflection remains a future, non-blocking card. The current
ruling requires authored declarations or compiler-generated declarations for
canonical TypeIds and type graph rows.

## 31. Settled directions

The conversation currently holds these directions:

1. Explicit `?name` variable syntax frees capitalization for ordinary names.
2. Parentheses are the uniform application syntax for values and types.
3. Type expressions use ordinary relation application and output columns.
4. The arrow marks output columns or a return facade inside the relation tuple.
5. A type is represented by a canonical TypeId.
6. Relation-shaped and primitive-shaped types share the TypeId domain.
7. A relation-shaped type owns ordered named edges.
8. Scope, module, relation namespace, and nested declaration use the same named
   edge machinery.
9. Brace nesting contributes a name prefix without implicit parent storage.
10. Dot follows a named edge to its target.
11. Double colon is the candidate edge-row reference.
12. Annotations are ordinary relation applications or ordinary facts over type
    nodes and keyed edges.
13. Compile-time rules should use the same relational semantics as runtime
    rules.
14. Compiler construction remains deterministic, ground, canonical, and
    bounded.
15. Target-specific vocabulary belongs in target plans and emitters.

## 32. Open design decisions

The following details remain open:

### 32.1 Surface commitment

The prefix forms currently act as a semantic microscope. A parser decision is
still needed between:

```text
infix declarations with prefix applications
fully prefix S-expressions
both surfaces lowering to one AST
```

### 32.2 Product and sum operators

`*` for product and `+` for sum are proposed. Their resolution against numeric
operators needs a scope, arity, and argument-domain contract.

### 32.3 Colon AST name

`bind` and `label-edge` are candidates. The operator creates a name-to-target
edge inside an implicit current owner.

### 32.4 Dot and edge reference

The target projection semantics are held. The exact parser rules for `.`, `::`,
qualified paths, edge-valued arguments, and quoted names remain to be fixed.

### 32.5 Return facade

Exactly one output maps naturally to expression position. Zero and several
outputs need a formal relation-shaped result or diagnostic rule.

### 32.6 Canonical identity

Structural terms, content-addressed text IDs, and surrogate interned IDs share
the same functional dependency. The public representation and collision
contract remain open.

### 32.7 One executable evaluator

Compiler and runtime semantics can share an evaluator. The current Prolog
compiler evaluator and target runtime remain separate implementations. A
self-hosting or shared-engine migration requires explicit bootstrapping,
artifact, and effect boundaries.

### 32.8 Outer refreeze

Generated type construction currently uses an outer bounded loop. A future
authoritative type graph may absorb more construction into ordinary closure.
Termination and domain-expansion rules still need a stable contract.

### 32.9 Node and edge classifications

Product, sum, namespace, parameter, return, annotation, variant, field, and
contains classifications can be ordinary relations. Their final public
signatures and uniqueness rules remain on `@semantic-plane-rulings`.

## 33. Resume checkpoint

When resuming this design, begin with these statements:

```text
1. F(A...) is one application syntax.
2. A relation output is another tuple column.
3. F(A...) in expression position lowers to F(A..., ?Result).
4. A type expression is an expression whose result has type `type`.
5. Generic "substitution" is ordinary variable binding and argument flow.
6. The compiler owns canonical TypeId identity.
7. A relation-shaped TypeId is an ordered named edge set.
8. Colon binds an edge, dot follows one, and double colon refers to one.
9. Compile-time and runtime relations can use the same fixpoint semantics.
10. Generated domain expansion is the bounded construction boundary.
```

Then review these three active gates in order:

```text
semantic-plane-rulings
relation-application-semantics
type-edge-view
```

Those gates determine the public signatures consumed by integrity, flow,
materialization, temporal, CSP, and emitter work.

## 34. Glossary

**Application**
: Calling a relation with arguments. In expression position, lowering adds
  fresh output variables.

**Constructor**
: A declared relation used to produce a value or type result.

**Specialization**
: The canonical result of applying a generic constructor to ground type
  arguments, such as `Option(int)`.

**Return facade**
: The output type exposed to the expression containing a relation call.

**TypeId**
: Canonical compiler identity for one type node.

**Interning**
: Ensuring the same canonical identity is returned for the same constructor and
  ordered arguments.

**Type graph**
: TypeId nodes plus named typed edges and derived indexes over them.

**Bind**
: Create a named edge from the current owner to a target expression.

**Project**
: Follow a named edge from an owner and return its target.

**Edge reference**
: Return the keyed edge row instead of following it to the target.

**Unification**
: Bind logic variables by matching relation positions and require repeated
  variables to carry equal values.

**Fixpoint**
: Repeatedly derive unseen relation rows until another round adds nothing.

**Refreeze**
: Admit complete generated type declarations, rebuild the canonical type
  snapshot, and run compiler closure again.

**Product**
: A type shape containing all of its named fields.

**Sum**
: A type shape selecting one of several named variants.

**Cons**
: A pair constructor whose tail can point to another cons cell to form a list.

## Prior-art links

- [SWI-Prolog syntax and variable prefixes](https://www.swi-prolog.org/pldoc/man?section=syntax)
- [Mercury syntax](https://mercurylang.org/information/doc-latest/mercury_reference_manual/Syntax.html)
- [Mercury predicate and function declarations](https://mercurylang.org/information/doc-release/mercury_ref/Predicate-and-function-type-declarations.html)
- [Souffle relations](https://souffle-lang.github.io/relations)
- [Flix fixpoints](https://doc.flix.dev/fixpoints.html)
- [Flix identifier rules](https://doc.flix.dev/identifiers.html)
- [Datomic query grammar](https://docs.datomic.com/query/query-data-reference.html)
- [Datomic history filters](https://docs.datomic.com/reference/filters.html)
- [SPARQL query variables](https://www.w3.org/TR/sparql11-query/#rVar)
- [Notation3 rule variables](https://www.w3.org/2000/10/swap/doc/Rules)
