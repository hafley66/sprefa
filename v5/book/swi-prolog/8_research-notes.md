# 8. Research Notes and Sources

## Official sources

| Area | Source |
|---|---|
| SWI 10 stable manual | [SWI-Prolog 10.0.1 reference manual](https://www.swi-prolog.org/download/stable/doc/SWI-Prolog-10.0.1.pdf) |
| Tabling | [Tabled execution](https://www.swi-prolog.org/pldoc/man?section=tabling) |
| Incremental tabling | [Incremental tabling](https://www.swi-prolog.org/pldoc/man?section=tabling-incremental) |
| Tabling and constraints | [Tabling and constraints](https://www.swi-prolog.org/pldoc/man?section=tabling-constraints) |
| Resource restraints | [Tabling restraints](https://ww1.swi-prolog.org/pldoc/man?section=tabling-restraints) |
| Dynamic database and transactions | [Database](https://www.swi-prolog.org/pldoc/man?section=db) |
| Thread queues | [Message queues](https://www.swi-prolog.org/pldoc/man?section=msgqueue) |
| Source positions | [Term reading and writing](https://ww1.swi-prolog.org/pldoc/man?section=termrw) |
| Source inspection | [`library(prolog_source)`](https://www.swi-prolog.org/pldoc/doc/_SWI_/library/prolog_source.pl?public_only=false) |
| Pack format | [Creating extension packages](https://www.swi-prolog.org/howto/Pack.html) |
| Saved application setup | [Project special files](https://www.swi-prolog.org/pldoc/man?section=project-special-files) |
| Saved executable behavior | [Running an application](https://www.swi-prolog.org/pldoc/man?section=runcomp) |
| Unix executable packaging | [Creating executables on Unix](https://www.swi-prolog.org/FAQ/UnixExe.md) |
| RocksDB binding | [SWI RocksDB pack](https://www.swi-prolog.org/pack/list?p=rocksdb) |
| LSP server | [`lsp_server`](https://www.swi-prolog.org/pack/file_details/lsp_server/prolog/lsp_server.pl) |
| Alternate Prolog LSP | [`prolog_lsp`](https://us.swi-prolog.org/pack/list?p=prolog_lsp) |
| Debug adapter | [`debug_adapter`](https://www.swi-prolog.org/pack/file_details/debug_adapter/README.md) |

## Scryer sources

| Area | Source |
|---|---|
| Repository and README | [Scryer Prolog](https://github.com/mthom/scryer-prolog) |
| Crate 0.10.0 | [docs.rs package](https://docs.rs/crate/scryer-prolog/0.10.0) |
| Current library summary | [docs.rs latest](https://docs.rs/crate/scryer-prolog/latest) |

## Capability inventory

The SWI manual inventory relevant to this project includes language syntax, built-ins, modules, tabling, constraints, attributed variables, dynamic databases, transactions, threads, message queues, engines, foreign interfaces, saved states, HTTP, JSON, RDF, XML/HTML, ODBC, debugging, profiling, testing, documentation, and package management.

The exact pack ecosystem is larger and changes independently of the core runtime. Pack behavior should be verified at the pinned revision used by a build.

## Recent timeline

| Date | Event |
|---|---|
| 2026-02 | SWI-Prolog 10.0.1 stable manual published |
| 2026-07-20 | Homebrew installed SWI-Prolog 10.0.2 for the local lab |
| 2025-10-31 | Scryer Prolog 0.10.0 entered the FreeBSD ports timeline |
| 2026-05-27 | docs.rs release listing recorded Scryer 0.10.0 |

## Limits and warnings

- SWI extensions reduce portability to ISO-focused runtimes.
- Incremental tabling tracks dynamic-predicate dependencies, while a text editor still needs document identity, source spans, and version ownership.
- Shared tables and transactions have documented interaction limits.
- Tabled answers consume memory until reclaimed or abolished.
- Tabled constraints are supported, with documented representation caveats for attributed variables.
- `prolog_lsp` labels itself immature.
- Scryer's README describes ongoing implementation goals. Advanced SWI features require feature-by-feature verification before porting.
- Old Prolog tutorials often omit modules, tabling, strings, dicts, transactions, and modern constraint usage.

## Generation notes for humans and language models

- State predicate modes in comments: `+` input, `-` output, `?` either.
- State determinism: `det`, `semidet`, `nondet`, or `multi`.
- Keep pure semantic relations separate from dynamic-database updates and I/O.
- Put source spans in semantic terms before implementing diagnostics.
- Test every intended predicate direction independently.
- Inspect choicepoints in code generation. Multiple proofs can multiply emitted artifacts.
- Add cuts only at deliberate committed-choice boundaries.
- Table recursive semantic relations when cycles are valid domain data.
- Ground inputs before negation unless well-founded tabled semantics are intended.

## Research gaps

- No benchmark compares SWI 10.0.2 and Scryer 0.10.0 on the Soup parser, path enumeration, or LSP workload.
- Scryer's exact support for incremental, monotonic, subsumptive, and shared tabling was not established from current primary documentation.
- No current issue census was completed for SWI LSP packs or Scryer embedding APIs.
- The saved macOS executable measured 290 KiB before code signing; portability to other systems was not tested.
- Static SWI linking and distributable macOS runtime packaging were not tested. The current saved executable dynamically loads `libswipl.10.dylib`.
