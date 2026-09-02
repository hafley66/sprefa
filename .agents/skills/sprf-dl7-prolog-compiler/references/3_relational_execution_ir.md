# Dual-time extraction and relational execution IR

## Contents

1. Recovered V6 shape
2. Two clocks
3. Two IR levels
4. Artifact and runtime flow
5. Decisions still open

## 1. Recovered V6 shape

V6 already contains three generations of the same program.

```text
plan/9
  program, types, relation plans, arrivals, rule order, edge rules

lowered/8
  DDL, arrival statements, edge statements, level statements,
  delta statements, relation plans, arrival targets

IGenProgram / GenProgram
  emitted target module or ProgramJson carrying DDL, relation metadata,
  boot statements, incremental relation plans, edges, levels, retention,
  host plans, queries, and SQLite statement text
```

`v6/prolog/lower.pl` documents `lowered/8` as plain Prolog structures plus SQL
text. `emit_ts.pl` and `emit_rust.pl` render the same lowering. The TypeScript
`IGenProgram` and Rust `GenProgram` are homologous runtime contracts guarded by
`IR_VERSION = 1`.

The valuable SQLite lowering includes the researched DBSP mechanisms:

- arrival add and retract statements;
- current, next, delta, and boundary storage;
- keyed replacement and edge-triggered writes;
- support counts and refcounts;
- recursive ping/pong expansion;
- DRed cone construction, trimming, rederivation, and revival;
- scoped aggregate recomputation;
- retention and history cleanup;
- structured-value and string interning;
- final relation reads and tick delta publication.

These are generated algorithms encoded as SQL templates, rather than ordinary
handwritten application queries.

## 2. Two clocks

`sprefa-extract` can execute under either clock while retaining the same pure
per-input extraction implementation.

```text
COMPILE CLOCK

source/schema bytes
      |
      v
sprefa-extract(content identity, grammar, query)
      |
      v
static facts
      |
      v
DL7 comptime fixpoint -> type graph and checked logical program


RUNTIME CLOCK

file/API/process event at tick N
      |
      v
sprefa-engine-rs host invokes sprefa-extract
      |
      v
response facts become arrivals
      |
      v
relational tick N+1 -> deltas, state, and further effects
```

Compile-time extraction is content-addressed compiler input. Runtime extraction
is an effect whose response crosses a tick boundary and can be persisted or
replayed as ordinary arrivals.

## 3. Two IR levels

Keep the semantic graph separate from its storage algorithm.

```text
Logical relational IR
  relations, columns, keys, rules, polarity, aggregates,
  dependencies, strata, effects, input and output relations
                  |
                  v
Relational execution IR
  storage slots, reads, writes, arrivals, departures, frontiers,
  boundaries, support, recursion, retention, transaction order
                  |
          +-------+----------------+
          |                        |
          v                        v
SQLite physicalizer         another physicalizer
  DDL and SQL templates       target-native operators
```

The execution IR is the common DBSP/SQL model. A read or write remains an
operation with relation identity, row shape, phase, and dependency metadata.
The SQLite physicalizer renders those operations into the proven statement
templates. Target runtimes orchestrate the emitted operations; they do not
rediscover dependency or DBSP algorithms from SQL text.

V6 partially merges the second and third boxes because `lowered/8` already
contains SQL. V7's reified logical-program rows currently provide the first
box. Recovering V6 utility requires an explicit execution-IR box between them.

## 4. Artifact and runtime flow

```text
                    COMPILER PROCESS

 DL7 + external facts
          |
          v
 type/comptime closure
          |
          v
 checked logical IR
          |
          v
 relational execution IR
          |
          +----------------------+----------------------+
          |                      |                      |
          v                      v                      v
 SQLite physicalizer      Rust app emitter       TS app emitter
          |                      |                      |
          +----------------------+----------------------+
                                 |
                                 v
                       generated app bundle
                   schema SQL + tick SQL + plan
                    host/effect adapters + code


                         RUNTIME PROCESS

 external events -> target app/runtime -> arrival batch
                                      |
                                      v
                             sprefa-engine-rs or TSV2
                                      |
                          transaction/tick interpretation
                                      |
                                      v
                                  SQLite state
                                      |
                         deltas + effects + subscriptions
                                      |
                     sprefa-extract may run as one host effect
```

`sprefa-engine-rs` is the primary runtime. TSV2 and generated target-language
applications are alternate consumers or conformance doors over the same
execution contract. The SQLite templates remain shared emitted data instead
of being independently recreated in every target language.

## 5. Decisions still open

The recovered direction leaves three boundaries requiring explicit choice.

1. Execution IR may carry abstract read/write operations until the SQLite
   physicalizer, or it may retain SQL-bearing statement records matching
   V6. The former exposes more backend portability; the latter reaches V6
   parity sooner.
2. A target artifact may embed one serialized execution plan interpreted by
   the runtime, or emit monomorphic target source containing that plan and
   direct orchestration calls. V6 Rust currently embeds ProgramJson in source.
3. Compile-time extraction may run in-process through Rust FFI, through a
   compiler effect protocol, or as a content-addressed subprocess. Runtime
   extraction already belongs naturally inside `sprefa-engine-rs`.

Preserve the V6 SQL templates as the oracle while separating these boundaries.
Conformance compares tick logs, SQLite state, and emitted deltas for identical
logical programs and schedules.
