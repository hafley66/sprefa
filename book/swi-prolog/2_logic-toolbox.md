# 2. Logic and Constraint Toolbox

## Tabling

```prolog
:- table reachable/2.

reachable(A, B) :- edge(A, B).
reachable(A, C) :- edge(A, B), reachable(B, C).
```

Tabling memoizes answers and suspends a recursive call when it encounters an equivalent call already under evaluation. It prevents ordinary left-recursive looping and computes only answers demanded by a query.

## Incremental tabling

```prolog
:- dynamic edge/2 as incremental.
:- table reachable/2 as incremental.
```

When an incremental dynamic fact changes, SWI invalidates dependent tables through an incremental dependency graph. Re-evaluation occurs on demand and can stop propagating when a recomputed answer set is unchanged.

This maps directly to an editor document graph:

```text
document text -> declarations -> references -> diagnostics -> hover
```

## Well-founded negation

Ordinary `\+ Goal` means failure to prove `Goal`. Tabled well-founded semantics can represent undefined results caused by recursion through negation. This matters for mutually recursive visibility, trait, or authorization rules.

## Answer subsumption

Mode-directed tabling can retain an aggregate answer such as minimum cost or maximum score rather than every proof. Candidate ranking and shortest semantic paths are common uses.

## Constraint libraries

| Library | Domain | Example use |
|---|---|---|
| CLP(FD) | Finite-domain integers | Lengths, arity, protocol bounds, layout |
| CLP(B) | Boolean constraints | Feature conditions and capability sets |
| CLP(Q) | Rational numbers | Exact numeric relations |
| CLP(R) | Real-number constraints | Continuous constraints |
| CHR | Constraint Handling Rules | User-defined type constraints and normalization |
| `dif/2` | Disequality | Sound symbolic inequality |

CLP(FD) example:

```prolog
valid_tuple_arity(Fields, Min, Max) :-
    length(Fields, Count),
    Count #>= Min,
    Count #=< Max.
```

## CHR as a type-checking substrate

CHR rules rewrite a constraint store:

```prolog
:- chr_constraint subtype/2, equal_type/2.

equal_type(A, B) ==> subtype(A, B), subtype(B, A).
subtype(A, B), subtype(B, C) ==> subtype(A, C).
```

For a language experiment, CHR can express propagation and simplification. Source spans and proof provenance still need explicit fields.

## Aggregation

```prolog
setof(Path, type_path(user, Path), Paths).
findall(Location, reference(Name, Location), Locations).
aggregate_all(count, reference(Name, _), Count).
```

`setof/3` sorts and deduplicates. `findall/3` preserves every generated answer. Aggregation decides when relational enumeration becomes one result value.
