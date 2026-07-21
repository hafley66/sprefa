# 0a. From SQL and Datalog to Prolog

## One relation, three syntaxes

Data:

```text
consumer(http, get, user_path, user)
consumer(channel, subscribe, user_event, user)
```

SQL:

```sql
select action, pattern, result
from consumer
where protocol = 'http';
```

Datalog:

```datalog
http_consumer(Action, Pattern, Result) :-
    consumer("http", Action, Pattern, Result).
```

Prolog:

```prolog
http_consumer(Action, Pattern, Result) :-
    consumer(http, Action, Pattern, Result).
```

The Datalog and Prolog clauses look similar. Evaluation strategy and available term shapes separate them.

## Select and where

SQL:

```sql
select name from type_decl where kind = 'model';
```

Datalog:

```datalog
model_name(Name) :- type_decl(Name, "model").
```

Prolog uses structural matching:

```prolog
?- type_decl(Name, model(Fields)).
```

`model(Fields)` filters the row and extracts its nested field list in one operation.

## Join

SQL:

```sql
select c.protocol, c.action, p.source
from consumer c
join pattern_decl p on p.name = c.pattern;
```

Datalog:

```datalog
consumer_source(Protocol, Action, Source) :-
    consumer(Protocol, Action, PatternName, _),
    pattern_decl(PatternName, Source).
```

Prolog:

```prolog
?- consumer(Protocol, Action, PatternName, _),
   pattern_decl(PatternName, Source).
```

The repeated variable `PatternName` is the join condition.

## Derived relation as a view

SQL:

```sql
create view consumer_source as
select c.protocol, c.action, p.source
from consumer c
join pattern_decl p on p.name = c.pattern;
```

Datalog:

```datalog
consumer_source(Protocol, Action, Source) :-
    consumer(Protocol, Action, PatternName, _),
    pattern_decl(PatternName, Source).
```

Prolog:

```prolog
consumer_source(Protocol, Action, Source) :-
    consumer(Protocol, Action, PatternName, _),
    pattern_decl(PatternName, Source).
```

The rule resembles a SQL view and remains executable without materializing a table.

## Not exists

SQL:

```sql
select *
from type_decl t
where not exists (
    select 1 from consumer c where c.result = t.name
);
```

Datalog with stratified negation:

```datalog
unused_result_type(Name, Type) :-
    type_decl(Name, Type),
    not consumer(_, _, _, Name).
```

Prolog:

```prolog
unused_result_type(Name, Type) :-
    type_decl(Name, Type),
    \+ consumer(_, _, _, Name).
```

`Name` is bound before negation. Goal order carries operational meaning.

## Recursive CTE

SQL:

```sql
with recursive reachable(source, target) as (
    select source, target from edge
    union
    select r.source, e.target
    from reachable r
    join edge e on e.source = r.target
)
select * from reachable;
```

Datalog:

```datalog
reachable(Source, Target) :- edge(Source, Target).
reachable(Source, Target) :-
    reachable(Source, Middle),
    edge(Middle, Target).
```

Tabled Prolog:

```prolog
:- table reachable/2.

reachable(Source, Target) :- edge(Source, Target).
reachable(Source, Target) :-
    reachable(Source, Middle),
    edge(Middle, Target).
```

Tabling supplies answer memoization and cycle handling.

## Group and collect

SQL:

```sql
select owner, count(*) as path_count
from type_path
group by owner;
```

Datalog with an aggregate extension:

```datalog
path_count(Owner, count<Path>) :-
    type_path(Owner, Path).
```

Aggregate syntax varies between Datalog implementations. The logical input remains the grouped relation `type_path(Owner, Path)`.

Prolog:

```prolog
?- findall(Path, type_path(user, Path), Paths).
?- setof(Path, type_path(user, Path), UniqueSortedPaths).
?- aggregate_all(count, reference(user_id, _), Count).
```

`findall/3` preserves generated answers. `setof/3` sorts and deduplicates. Aggregation converts answer enumeration into one result value.

## Departure 1: recursive terms

SQL commonly normalizes the tree into related tables:

```sql
select f.model_name, f.field_name, f.type_constructor, f.type_argument
from model_field f
where f.type_constructor = 'array';
```

Datalog commonly uses flattened relations for portability:

```datalog
array_field(Model, Field, Item) :-
    model_field(Model, Field, TypeId),
    array_type(TypeId, Item).
```

Prolog relations accept tree-shaped values directly:

```prolog
type_decl(user, model([
    field(id, user_id),
    field(tags, array(string)),
    field(metadata, map(string, string))
])).
```

Query every directly declared array field:

```prolog
?- type_decl(Model, model(Fields)),
   member(field(Field, array(Item)), Fields).
```

Variables serve as columns and holes inside trees at the same time.

## Departure 2: argument modes

SQL gives each statement an explicit result projection:

```sql
select binding_value
from pattern_match
where pattern = :pattern and text = 'users/alice';
```

Datalog queries can bind different columns of a finite relation:

```datalog
?- pattern_value(Pattern, Binding, "users/alice").
?- pattern_value(Pattern, "alice", Text).
```

The relation must still be enumerable over the chosen domain. Unrestricted string construction generally requires engine extensions or a separate function.

This relation connects a pattern, bindings, and text:

```prolog
pattern_value(Pattern, Bindings, Text).
```

Parse direction:

```prolog
?- pattern_value(Pattern, Bindings, "users/alice").
Bindings = [id-"alice"].
```

Render direction:

```prolog
?- pattern_value(Pattern, [id-"alice"], Text).
Text = "users/alice".
```

Unification does not assign a privileged input or output position. Predicate implementation still determines which modes terminate.

## Departure 3: programs are terms

SQL exposes its parsed program through vendor-specific syntax trees, query plans, or catalog views rather than ordinary SQL values.

Datalog usually treats rules as the program that defines relations. Meta-Datalog systems can reify rules into relations, but this is outside plain Datalog's common core.

Prolog uses compound terms as its source representation. This source clause:

```prolog
Head :- Body.
```

parses into a compound term shaped like:

```prolog
:-(Head, Body)
```

Operators control parsing and display. DCGs, term expansion, goal expansion, and meta-calls transform language fragments using the same term machinery used for domain data.

## Syntax rhythm

| Mark | Reading |
|---|---|
| `.` | End clause or query |
| `,` | And, then solve next goal |
| `;` | Or, or request another top-level answer |
| `:-` | Head holds if body holds |
| `?-` | Ask this goal |
| Uppercase name | Logic variable |
| Lowercase name | Atom |
| `name(...)` | Compound term |
| `[A, B]` | List |
| `[Head\|Tail]` | List decomposition |
| `_` | Anonymous variable |
| `\+ Goal` | Goal cannot be proven |
| `!` | Commit to choices made since entering this predicate |
| `-->` | DCG rule with hidden sequence arguments |

## Concept bridge

```text
SQL: relations queried by a planner
  -> Datalog: relations plus recursive derivation
  -> Prolog: recursive terms plus goal-directed proof search
  -> DCGs: proof relations over sequences
  -> meta-programming: terms representing and transforming programs
```
