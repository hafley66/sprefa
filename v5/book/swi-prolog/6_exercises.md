# 6. Exercises Against the Lab

## 1. Query the parsed database

```sh
cd labs/swi-typespec-lab
swipl
```

```prolog
?- ['0_schema.pl'].
?- type_decl(Name, Type).
?- consumer(Protocol, Action, Pattern, Result).
```

Observe semicolon-driven answer enumeration.

## 2. Add a nested type

Add to `schema.soup`:

```typespec
type Team {
  id: String;
  members: User[];
}
```

Query:

```prolog
?- ['1_types.pl'].
?- setof(Path, type_path(team, Path), Paths).
```

Expected members include:

```text
members[*].id
members[*].profile.name
members[*].tags[*]
members[*].metadata{key}
```

## 3. Add an invalid union value

```prolog
?- ['2_patterns.pl'].
?- parse_pattern("users/:id/events/{kind: EventKind}", Pattern),
   pattern_value(Pattern, Bindings, "users/alice/events/renamed").
false.
```

Trace it:

```prolog
?- trace, pattern_value(Pattern, Bindings, Text).
```

Watch the union variants backtrack.

## 4. Table recursive aliases

Add a new relation rather than changing `canonical_type/2` immediately:

```prolog
:- table alias_reaches/2.

alias_reaches(A, B) :- type_decl(A, alias(B)).
alias_reaches(A, C) :- type_decl(A, alias(B)), alias_reaches(B, C).
```

Compare behavior with and without tabling after introducing an alias cycle.

## 5. Inspect runtime statistics

```prolog
?- time(setof(Path, type_path(user, Path), Paths)).
?- statistics.
```

Record inference count, table space, local stack, global stack, and atom count before adding a large generated schema.

## 6. First LSP slice

Implement only these messages:

```text
initialize
textDocument/didOpen
textDocument/hover
shutdown
exit
```

Use whole-document synchronization and full reparsing. This isolates framing, position conversion, span lookup, and response encoding before incremental maintenance.
