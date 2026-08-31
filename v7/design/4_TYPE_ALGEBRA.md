# DL7 userland type algebra

## Type graph

A product type is a node plus ordered `:/4` edges:

```lisp
(: User
   (* (: id int)
      (: name text)))
```

The compiler graph contains the equivalent shape:

```text
node(User)
product(User)
:(User, id, int, 0)
:(User, name, text, 1)
```

Contracts use the same representation. There is no separate interface node
kind in this slice.

## Structural conformance

```text
Conforms(Source: type, Contract: type, return: type)
```

For every contract edge, `Source` must contain an edge with the same label and
target. Source ordinals may differ and extra source edges are accepted.

An expression:

```lisp
(: UserProof (Conforms User UserContract))
```

lowers to the ordinary relation goal:

```text
Conforms(User, UserContract, UserProof)
```

The relation interns `(Source, Contract)` before validation. The request is
frozen as `intern_snapshot/3` in the next compiler round. Contract comparison
then derives either the proof or failure rows:

```text
matching_contract_edge(Source, Contract, Name, Target, ContractIndex)
missing_contract_edge(Source, Contract, Name, Target, ContractIndex)
```

Failure remains absence of `Conforms/3`. The explicit missing-edge rows make
the reason queryable by other compiler rules.

## Contract conjunction

```text
ConformsAll(Source: type, Contracts: list(type), return: type)
```

`Contracts` is an ordinary closed `cons/3` list. Positive recursion walks the
list and calls `Conforms/3` for each element. The empty list succeeds. This is
the initial interface-intersection operation when no materialized merged type
is needed.

## Generic constraints

A generic constraint is a body goal:

```lisp
(<- (NamedBox ?Source ?Result)
    (Conforms ?Source Named ?Proof)
    (nil ?Empty)
    (cons ?Source ?Empty ?Arguments)
    (intern NamedBox ?Arguments ?Result))
```

`NamedBox(User, Result)` succeeds only after `Conforms(User, Named, Proof)`.
The generic, constraint, proof, and result application all use ordinary DL7
relations and the shared compiler fixpoint.

## Materialized intersection

```text
Intersect(Left: type, Right: type, return: type)
```

The result identity is canonical for the ordered pair `(Left, Right)`. Its
edge order is:

```text
all Left edges in Left order
then Right edges whose labels are absent from Left, in Right order
```

An equal label and equal target is deduplicated. An equal label and unequal
target derives:

```text
intersection_conflict(Left, Right, Name)
```

and no `Intersect/3` result. Dense result ordinals are computed by counting
the strict predecessor rows for each retained candidate edge.

`Extend/3` demonstrates composition in userland:

```lisp
(<- (Extend ?Base ?Extension ?Result)
    (Intersect ?Base ?Extension ?Result))
```

It returns the same canonical intersection identity and adds no compiler
built-in.

## Relation-valued edges and impl evidence

An edge target may be an ordinary relation type:

```lisp
(: HashFunction
   (* (: input type)
      (: return int)))

(: Hashable
   (* (: hash HashFunction)))
```

Conformance compares `HashFunction` by the same type identity comparison used
for primitive and product targets.

Impl evidence is authored data:

```text
implements(Contract, Source, Witness)
```

The prelude derives:

```text
valid_impl(Contract, Source, Witness)
invalid_impl_edge(Contract, Source, Witness, Name, Target, ContractIndex)
```

The witness is an ordinary product containing the required implementation
edges. This slice adds no impl declaration syntax.

## HistoryV1

History options are typed data with two edges:

```lisp
(: HistoryOptions
   (* (: mode "copy")
      (: contract UserContract)))
```

`HistoryV1(Source, Options, Result)` reads the contract edge and requires a
conformance proof before interning the history specialization. Existing rules
then copy source edges and emit `def/head/head_arg/body/body_arg` rows. The
generated relation and generated rule enter the same checked runtime program
as before.

## Compiler rounds

```text
round 1   relation call reaches intern/3 and records an application
round 2   intern_snapshot/3 exposes the canonical candidate
round 2+  edge comparison derives matches, failures, conflicts, or validity
round 3+  successful relation results derive product edges or generated rules
stable    intern rows are removed from the published compiler closure
```

The bound remains 16 rounds. The consolidated fixture reaches a stable
closure within that bound.

## Files

```text
v7/prelude/0_constructors.dl7
v7/prelude/1_declarations.dl7
v7/prelude/2_constructor_rules.dl7
v7/prelude/3_derived_rules.dl7
v7/prelude/4_type_algebra.dl7
v7/test/fixtures/3_type_algebra.dl7
v7/test/1_entrypoints.test.pl
```

The separate `dl7-module-system` epic owns multi-file module semantics. Its
source inventory begins at the DL6 `use_resolve.pl`, `0_dot_expand.pl`, and
`executor_modules.pl` implementations.

