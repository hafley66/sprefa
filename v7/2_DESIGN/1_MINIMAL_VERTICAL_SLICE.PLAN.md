# DL7 minimal programmable kernel

Date: 2026-08-28

## Goal

Build the smallest `.dl7` kernel that proves one relational evaluator can run
both compiler rules and runtime rules.

```text
.dl7 terms
    -> bind and application lowering
    -> one normalized fact/rule IR
    -> one SWI-Prolog evaluator
         | compiler seeds -> type graph
         | runtime seeds  -> runtime rows
    -> later backend adapter
    -> existing v6/sprefa-engine-rs
```

The Rust engine remains in place. V7 adds no Rust runtime and copies no engine
source. ProgramJson adaptation follows the kernel proof.

Userland helper types are proof goals in dependency order:

```text
Partial
    -> Pick
    -> Exclude
```

They must be authored as `.dl7` rules. Their names and behavior cannot appear
inside the reader, graph kernel, or evaluator.

## Receipts from saved design discussions

The plan incorporates Boop favorites 26 through 37, especially:

- favorite 37: reader, binder resolution, core lowering, and execution are
  distinct compiler phases;
- favorite 36: `?x` is a variable, `x` is a reference, `'x` is symbol data;
- favorite 34: a type expression is an ordinary expression returning `type`;
- favorite 33: prefix lists expose one application tree;
- favorite 31: return values remain relation tuple columns;
- favorite 29: value and type calls share application lowering;
- favorite 28: canonical specialization identity is kernel interning;
- favorite 27: compile time and runtime use one relational fixpoint algorithm.

## Overnight code ceiling

- Four production modules maximum: reader, graph/lowering, evaluator, driver.
- One standard-library `.dl7` file.
- One fixture.
- One end-to-end SWI test file containing one test.
- One optional engine smoke command after the kernel is green.
- No V6 full suite, generated corpus, TS V2 command, benchmark, or formatter.
- No imports, effects, ticks, retention, history, macros, body reopening,
  recursive types, partial application, higher-kinded types, or emitter rewrite.

An out-of-scope dependency stops its card and produces a Boop hail with the
missing signature or ruling.

## Syntax kernel

The reader accepts one prefix tree:

```ebnf
term := atom | variable | literal | "(" term* ")"
```

Kernel spellings:

```lisp
(: Name Target)                 named edge in the current owner
(* Binding...)                  product
(+ Binding...)                  sum
(-> Inputs Output)              callable signature
(<- Head Body...)               rule
(F Argument...)                 application in expression position
?x                              logic variable
x                               reference
'x                              symbol data
```

If the Sol contract review finds that the compact spelling
`Name: *(...)` can be added without a second AST path, it may be accepted as
reader sugar. The canonical reader output remains the prefix tree above.

## Type and edge graph

All type-like values occupy one semantic identity domain. Product, sum,
primitive, namespace, and specialization are ordinary classifications of
those identities.

The public edge fact follows the settled order:

```prolog
':'(+Owner, +Name, +Target, +Index).
```

Its logical key is `(Owner, Name)`. `Index` preserves authored order. The
complete ground colon term can be passed as the edge node when an annotation
or compiler rule refers to the edge itself. No synthetic public edge ID and no
`member` vocabulary enter V7.

The file module path supplies the implicit outer owner:

```lisp
(: User
  (*
    (: id int)
    (: name text)))
```

```prolog
':'(module(Path), 'User', UserType, 0).
':'(UserType, id, primitive(int), 0).
':'(UserType, name, primitive(text), 1).
```

The Sol contract review pins the exact ground term used for `UserType`.

## Bind and scope

A scope is a node whose outgoing colon edges form its symbol table. Name
resolution reads those edges. A product is also a node whose outgoing colon
edges form its ordered fields. The same relation supports both uses.

```prolog
resolve(+Owner, +Name, -Target).

% Read ':'(Owner, Name, Target, Index).
% Follow the module-owner chain defined by lowering.
% Return one target or one deterministic diagnostic.
```

The first slice has a file module owner and nested product owners. Imports,
reopening, and lexical closures remain outside the slice.

## Relation application

A declared callable is a product containing input edges and one `return` edge.
The return remains a column in the underlying relation tuple.

```lisp
(: Identity
  (->
    (* (: value any))
    any))
```

Application in expression position:

```lisp
(Identity 1)
```

lowers to the saturated relational goal:

```prolog
'Identity'(1, Result).
```

The surrounding form receives `Result`. The same lowering applies when the
return column has type `type`.

Required application laws:

- the callable must be declared;
- one output column is supported in this slice;
- zero rows means no result;
- one row means one result;
- several distinct rows violate deterministic expression application;
- unsaturated calls are refused;
- ordinary body goals remain relational and may produce several bindings.

## One evaluator

```prolog
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

% Validate rule safety and functional dependencies.
% Compute strata.
% Run positive recursive groups with SWI tabling.
% Read negation from completed lower strata.
% Return a canonical set of ground rows.
% Clear tables and temporary facts before returning.
```

The evaluator contains no compile-time or runtime branch.

```prolog
evaluate(CompilerRules, TypeSeeds, TypeClosure, TypeDiagnostics).
evaluate(RuntimeRules, RuntimeSeeds, RuntimeClosure, RuntimeDiagnostics).
```

Phase selection, effects, persistence, and backend emission live outside the
evaluator.

## Canonical construction

Plain Datalog cannot create a new domain value. The kernel therefore exposes
one deterministic construction relation:

```prolog
intern(+Constructor, +Arguments, -Result).
```

Functional dependency:

```text
(Constructor, ordered Arguments) -> Result
```

The Sol contract review pins the Result representation. Authored `.dl7` uses
ordinary calls such as `(Option int)` and `(Partial User)`. Application lowering
and the driver invoke interning when a declared type-returning call requires a
canonical result identity.

The driver may run an outer request loop:

```text
evaluate
    -> collect unseen ground constructions
    -> intern their identities
    -> add resulting facts
    -> evaluate again
    -> stop when no facts or requests change
```

Recursive construction and arbitrary fresh values are refused.

## Userland proof goals

### Partial

```prolog
':'(Output, Name, OptionalType, Index) <-
    specialization(Output, partial, [Input]),
    ':'(Input, Name, MemberType, Index),
    option(MemberType, OptionalType).
```

### Pick

```prolog
':'(Output, Name, MemberType, OutputIndex) <-
    specialization(Output, pick, [Input, Names]),
    ':'(Input, Name, MemberType, InputIndex),
    contains(Names, Name),
    selected_rank(Input, Names, InputIndex, OutputIndex).
```

### Exclude

```prolog
':'(Output, Name, MemberType, OutputIndex) <-
    specialization(Output, exclude, [Input, Names]),
    ':'(Input, Name, MemberType, InputIndex),
    not contains(Names, Name),
    remaining_rank(Input, Names, InputIndex, OutputIndex).
```

`Names` is a closed symbol collection before Exclude's stratum runs. Pick and
Exclude preserve relative order and produce dense output indices.

The overnight kernel may stop after Partial. Pick and Exclude remain separate
open cards over the proven evaluator.

## Type signatures by module

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).

lower_dl7(+ModulePath, +Forms, -Rules, -Seeds, -Requests, -Diagnostics).

evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).
```

Pseudocode body order for `compile_dl7/4`:

```text
read source and prelude
resolve module and bind edges
lower every relation output into tuple position
partition compiler rules from runtime rules using declarations
run compiler rules through evaluate/4
return normalized runtime rules without executing them
```

The reference proof then calls `evaluate/4` on the returned runtime program.
The later ProgramJson adapter consumes that same normalized runtime program.

## Instance timeline

```text
source bytes
    -> reader forms and source map
    -> resolved names and callable signatures
    -> normalized facts and rules
    -> compiler evaluate/4 call
    -> intern/refreeze until compiler closure
    -> normalized runtime program
    -> runtime evaluate/4 call for reference proof
    -> later ProgramJson adapter
    -> existing Rust engine
```

Reader identities last one compile. Semantic type identities last through the
artifact. Evaluator tables and request rows last one call. Runtime rows last
one reference evaluation. The Rust engine continues owning durable runtime
state after backend integration.

## Storage and uniqueness

| Row family | Key | Lifetime |
|---|---|---|
| reader form | reader node | one compile |
| source span | reader node | one compile |
| colon edge | owner plus name | semantic artifact |
| callable column | callable plus index | semantic artifact |
| specialization | constructor plus ordered arguments | semantic artifact |
| fixpoint row | complete row or declared functional key | one evaluation |
| construction request | constructor plus ordered arguments | one compile |
| runtime row | declared relation key | runtime evaluation |

## Donor policy

DL6 contributes audited predicates and laws:

- reader atoms, literals, escapes, comments, spans, and diagnostics;
- name collision and path resolution algorithms;
- semantic identity and canonical encoding laws;
- authored-order rule safety;
- strata, tabling, anti-join, aggregate separation, and functional-key checks;
- ProgramJson inventory and engine phase laws.

DL6 declaration terms, parser dispatch, `rel`, braces, dotted syntax, `plan/9`,
`lowered/8`, TS V2, and runtime source stay outside the kernel.

## Test ceiling

One focused test compiles one fixture and snapshots:

```text
reader forms
compiler type closure
normalized runtime program
runtime reference closure
second compile equality
```

This is one test invocation and one exact expected term. Any additional test
requires a concrete defect that cannot be represented in the same snapshot.

## Task DAG and model routing

```text
kernel-contract [Sol, heavy]
        |
        v
contract-critique [Opus 5, review]
        |
        +-----------------------------+
        v                             v
prefix-reader [GLM53F xhigh, medium]  shared-evaluator [Sol, heavy]
        |                             |
        v                             |
symbol-graph [GLM53F xhigh, medium]   |
        |                             |
        +-------------+---------------+
                      v
              partial-goal [GLM53F xhigh, medium]
                      |
                      v
              kernel-oracle [Flash 4, small]
                      |
                      v
              luna-review [Luna, low]
                      |
          +-----------+------------+
          v                        v
pick-exclude [GLM53F, goal]   engine-seam [Terra, medium]
                                   |
                                   v
                           engine-smoke [Flash 4, small]
```

Each lane begins from main after all blocker commits land. No lane pushes.
Issue updates and commit receipts are written from main by the coordinator.
