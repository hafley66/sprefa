# 0. Orientation

## The compact model

| Datalog | Prolog |
|---|---|
| Relations and rules | Predicates and clauses |
| Bottom-up fixed point | Goal-directed proof search |
| Flat tuples | Recursive compound terms |
| Set of query rows | Backtracking answers |
| Stratified negation | Negation as failure plus well-founded semantics under tabling |
| Engine-selected evaluation | Source order and goal order affect execution |

SWI adds tabling, constraints, mutable predicates, transactions, threads, HTTP, JSON, persistence bindings, profiling, debugging, foreign interfaces, and saved applications.

## The language pipeline

```text
schema.soup
    -> DCG parser
    -> Prolog semantic terms
    -> resolution and checking relations
    -> pattern relations and fact queries
    -> Rust, JavaScript, diagnostics, hover, completion
```

## Why terms matter

This source:

```typespec
type User {
  id: UserId;
  tags: String[];
}
```

becomes one recursive value:

```prolog
type_decl(user, model([
    field(id, user_id),
    field(tags, array(string))
])).
```

Unification decomposes the tree and binds its parts:

```prolog
?- field(tags, array(string)) = field(Name, array(Item)).
Name = tags,
Item = string.
```

## Execution caveat

Prolog searches clauses top to bottom and goals left to right. A relation can be logically sound while terminating in one argument mode and looping in another. Tabling and deliberate mode boundaries control this.
