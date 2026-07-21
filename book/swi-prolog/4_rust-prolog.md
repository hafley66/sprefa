# 4. Rust Implementations

## Scryer Prolog

[Scryer Prolog](https://github.com/mthom/scryer-prolog) is the primary modern Prolog implementation written mostly in Rust. Release 0.10.0 is current in the package sources checked on 2026-07-20.

Its implemented surface includes:

- Warren Abstract Machine execution
- ISO-oriented syntax and control
- Modules, operators, term and goal expansion
- DCGs and partial-string tooling
- Attributed variables
- `dif/2` and `freeze/2`
- CLP(B) and CLP(Z)
- SLG resolution
- JSON, sockets, TLS, crypto, processes, UUIDs, graphs, XML/HTML, and simplex libraries
- Prebuilt binaries and a Rust crate

## SWI and Scryer matrix

| Area | SWI-Prolog | Scryer Prolog |
|---|---|---|
| Implementation | Mature C runtime and native libraries | Mostly Rust |
| Version observed | 10.0.2 locally | 0.10.0 |
| ISO focus | Broad compatibility plus SWI extensions | Strong ISO-conformance goal |
| DCGs | Yes | Yes, with compact string and partial-string focus |
| Tabling | Variant, subsumptive, incremental, monotonic, shared, WFS | SLG support; verify exact advanced modes before depending on parity |
| Constraints | CLP(FD/B/Q/R), CHR, attributed variables | CLP(B/Z), attributed variables, user constraints |
| HTTP/server ecosystem | Extensive | Sockets and TLS; smaller server ecosystem |
| Persistence/database ecosystem | Persistency, RDF, ODBC, RocksDB pack, others | Smaller library surface |
| Profiling/debugging | Mature tracer, debugger, profiler, statistics | Toplevel and development tooling; smaller surface |
| Saved application deployment | `qsave_program/2` | Native Rust-built executable and script support |
| Rust embedding | Through SWI C API bindings | Published Rust crate |

## Rust Datalog relatives

These solve a narrower relational problem than Prolog:

| Project | Model |
|---|---|
| Datafrog | Lightweight fixed-point relations embedded in Rust |
| Ascent | Datalog-like Rust macro language with lattices |
| Crepe | Compiled Datalog-style Rust macros |
| Differential Dataflow | Incremental iterative dataflow with additions and removals |

They do not supply Prolog's general term language, bidirectional DCGs, goal-directed search, attributed variables, or broad interactive runtime.

## Portability boundary for this lab

The current lab uses several SWI-specific details:

```text
string objects
dicts
read_file_to_string/3
PlUnit behavior
dynamic database conventions
library(dcg/basics)
```

The semantic term shapes and basic DCGs are portable in concept. A Scryer run requires a compatibility pass rather than a binary substitution.
