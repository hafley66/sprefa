# 1. Runtime Toolbox

## Core execution

| Tool | Syntax | Use in a compiler or LSP |
|---|---|---|
| Unification | `A = model(Fields)` | AST matching and decomposition |
| Backtracking | multiple clauses or `member/2` | Enumerate declarations, paths, and candidates |
| Meta-calls | `call(Goal)` | Rule dispatch and configurable checks |
| Modules | `:- module(name, [...]).` | Compiler phases and library boundaries |
| DCGs | `rule(AST) --> ...` | DSL, templates, JSON-like syntax, protocol framing |
| Term expansion | `term_expansion/2` | Compile surface declarations during load |
| Goal expansion | `goal_expansion/2` | Specialize domain queries during load |
| Attributed variables | library interface | Attach constraints to logic variables |
| Coroutining | `dif/2`, `freeze/2`, `when/2` | Delay checks until arguments become known |

## Dynamic database

```prolog
:- dynamic document_text/3.

open_document(Uri, Version, Text) :-
    assertz(document_text(Uri, Version, Text)).

close_document(Uri) :-
    retractall(document_text(Uri, _, _)).
```

Dynamic predicates provide indexed in-memory relations. Use a document URI or revision as the first argument when it is the common lookup key.

## Transactions and snapshots

```prolog
transaction(update_document(Uri, Version, Text)).
```

SWI transactions isolate dynamic-predicate updates and roll them back on failure or exception. The official database documentation calls the transaction basics and API stable, while interactions with shared tables remain qualified.

## Tries

Tries store terms by structural prefix and back SWI's tabling machinery. They are useful for explicit intern tables, memoized semantic keys, and large sets of compound terms.

## Threads and queues

```prolog
message_queue_create(Queue),
thread_create(worker(Queue), Thread, []),
thread_send_message(Queue, parse(Uri, Version, Text)).
```

Message queues block without polling. Terms are copied when crossing thread boundaries, so variable identity and later bindings do not cross with them.

## Foreign interfaces

SWI provides C and C++ interfaces for embedding the runtime or implementing predicates in native code. A Rust host can call the C API through bindings, though the Prolog runtime remains SWI's C implementation.

## Development tools

| Tool | Purpose |
|---|---|
| Four-port tracer | Call, exit, redo, and fail inspection |
| Graphical debugger | Source-level tracing |
| `profile/1` | Time and call profiling |
| `time/1` | Inferences, CPU time, and throughput snapshot |
| `statistics/2` | Stacks, atoms, clauses, tables, and runtime counters |
| `make/0` | Reload modified source files |
| PlUnit | Unit and property-shaped tests |
| PlDoc | Documentation from source comments |
| `library(prolog_source)` | Source-file and term-position analysis |

## Resource controls

Tabling supports tripwires and restraints on subgoal size, answer size, and answer count. These can turn accidental infinite semantic expansion into diagnostics instead of unbounded growth.
