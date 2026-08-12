# Type IR polyglot door: visual price map

## TOC

1. [The door](#the-door)
2. [What fits](#what-fits)
3. [Where meaning leaks](#where-meaning-leaks)
4. [Candidates](#candidates)
5. [Two jobs](#two-jobs)
6. [Input choices](#input-choices)
7. [Forks for a human ruling](#forks-for-a-human-ruling)

## The door

```text
OpenAPI / JSON Schema ──> .dl6 type plane ──> JSON Schema / OpenAPI
                                  |
                                  +──> TS, Rust, Go, Python: not built yet

TS-shaped types   ─┐
Rust-shaped types ─┼──> .dl6 type plane: not built yet
other languages  ──┘
```

The target shape has one description of each source and target type system. Pairwise converters would grow as N times M.

## What fits

| `.dl6` idea | TypeScript | Rust | Schema languages |
|---|---|---|---|
| integer | `number`, with i64 precision gap | `i64` | integer plus bounds |
| float | `number` | `f64` | number/double |
| text | `string` | `String` | string |
| boolean | `boolean` | `bool` | boolean |
| arbitrary JSON | recursive JSON value or `unknown` | JSON value enum | unconstrained schema |
| named record | interface/type | struct | object/model |
| payload enum | discriminated union | enum with payloads | union plus a tag convention |
| optional value | `T | null` | `Option<T>` | union with null |
| inline JSON list | `T[]` | `Vec<T>` | array |

## Where meaning leaks

```text
list(T)
  value order                    TS array / Rust Vec can carry this
  list entity identity           lost
  member relation identity       lost

list_entity_dense_sequence(T)
  list + owner + reference count no direct language counterpart

list_interned_set(T)
  uniqueness                     Set / uniqueItems can carry this
  interned value identity        lost

list_entity_linked_sequence(T)
  values                         arrays can carry these
  member ids + explicit links    lost
```

```text
option(list(T))  = the whole list may be absent/null
list(option(T))  = the list exists; each item may be absent/null
```

TypeScript `number` cannot exactly carry every signed 64-bit integer. Optional object fields and nullable object fields are also separate concepts in TypeScript, JSON Schema, OpenAPI, and TypeSpec. The current plane has one `none` value that becomes JSON null.

## Candidates

```text
fixed neutral model
  TypeSpec        -> emitters
  Protocol Buffers -> compiler plugins
  Cap'n Proto     -> compiler plugins
  Smithy          -> generators

JSON interchange already beside the type plane
  JSON Schema -> quicktype -> TS / Rust / Go / Python / ...
              -> Typify -> Rust
              -> schema-to-TS -> TypeScript
  OpenAPI     -> OpenAPI Generator -> clients / servers / models

Rust as the starting point
  Rust -> ts-rs / Specta -> TypeScript
       -> Schemars -> JSON Schema
       -> Typeshare -> Kotlin / Swift / Scala / TS / Go
       -> Serde registry -> generated languages
```

Every candidate has one fixed middle model and code for each input or output. None takes a declarative description of Rust, TypeScript, Go, or Python and derives both parsing and printing from it.

The shortest already-connected experiment is:

```text
.dl6 registry -> existing JSON Schema emitter -> quicktype -> language types
```

Before that path preserves current meaning, the schema emitter needs rows for payload enums, signed-64 bounds, a null-versus-missing rule, and an explicit representation for each relational list constructor.

## Two jobs

Population pricing arrives in deliverable 3.

## Input choices

Input-side pricing arrives in deliverable 4.

## Forks for a human ruling

Forks arrive in deliverable 5. No fork is selected in this document.
