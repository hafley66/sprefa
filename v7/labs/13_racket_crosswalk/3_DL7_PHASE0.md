# DL7 Phase-0 Boundaries

Research and source-check date: 2026-08-29.

The phase-0 boundary keeps source syntax, lexical binding, logic variables, and evaluated facts as separate representations. The dataflow is:

```text
source bytes
  -> read-syntax
  -> source-located syntax objects
  -> phase-1 macro expansion
  -> resolved DL7 core forms
  -> logic-program IR with generated variable IDs
  -> restricted Datalog evaluation or an explicitly selected external runtime
  -> canonical rows and diagnostics
```

The [Racket syntax model](https://docs.racket-lang.org/reference/syntax-model.html) defines the read pass as producing syntax objects and the expand pass as producing a fully parsed form. The [Datalog manual](https://docs.racket-lang.org/datalog/) and the [Racklog manual](https://docs.racket-lang.org/racklog/) define the separate logic routes used after the front-end boundary.

## Reader boundary

Use a DL7 reader module with `read-syntax` as the source entry point. `syntax/module-reader` supplies the standard `#lang` reader protocol and supports custom `#:read` and `#:read-syntax` procedures. The reader output should contain:

| Reader output | Required information |
| --- | --- |
| Syntax node | Node kind, ordered children, original datum or token text |
| Source span | File identity, line, column, byte start, byte end |
| Identifier | Printed symbol plus syntax-object lexical information |
| Literal | Typed constant category and source span |
| DL7 rule form | Head, ordered body, polarity, requested outputs, and source span |

`#lang datalog/sexp` is a usable Datalog source language for a separate experiment. Its current manual says that its semantics match the normal Datalog language, its syntax is parenthetical, `require` forms are permitted, and top-level identifiers and datums are otherwise restricted. DL7 should use a dedicated reader so its module forms, declarations, syntax errors, and source metadata remain under DL7 ownership.

Reader receipts:

- [Racket reader reference](https://docs.racket-lang.org/reference/reader.html): read-syntax mode wraps read data in syntax objects and carries source-location information.
- [Racket `#lang` syntax](https://docs.racket-lang.org/guide/hash-lang_syntax.html): a language resolves to a reader module in a collection, with `#lang reader` as the general module-path escape.
- [Reader helpers](https://docs.racket-lang.org/syntax/reader-helpers.html): `syntax/module-reader` defines the `read` and `read-syntax` protocol.

## Scope boundary

Keep identifiers as syntax objects through binding resolution. Racket represents binding relationships with scope sets. `free-identifier=?` compares references to the same binding, while `bound-identifier=?` compares binding identifiers. Printed symbol equality is insufficient for macro-introduced bindings and shadowing.

The phase-0 normalized form should retain a stable binding record alongside the syntax object:

```text
binding-id = module identity + phase + resolved binding identity
source-id  = file identity + byte span
logic-id   = compiler-request identity + clause/query-local integer
```

`binding-id` identifies source-level names. `logic-id` identifies variables in a rule or query. A source identifier named `X` and a logic variable printed as `X` must remain distinct values until the lowering step explicitly maps one to the other. This prevents a macro-introduced binding, a module import, and a Datalog variable from colliding because they share a spelling.

Scope receipts:

- [Syntax model, identifiers and scopes](https://docs.racket-lang.org/reference/syntax-model.html): scope sets determine binding references and shadowing.
- [Syntax objects](https://docs.racket-lang.org/reference/stx-patterns.html): syntax data and lexical context can be inspected and transformed through the syntax-object API.
- [Racket phases](https://docs.racket-lang.org/guide/module-basics.html): module bindings are organized by phase, with phase 0 as runtime and phase 1 as expansion time.

## Macro boundary

Run macro expansion before logic lowering. A DL7 language can provide a module-begin transformer that expands declarations into explicit core forms. `syntax-parse` supplies pattern matching and error reporting for macro inputs. `local-expand` can be used where the compiler needs a controlled expansion point.

The macro contract should be:

```text
input:  syntax object for a DL7 surface form
output: syntax object for a DL7 core form with source metadata
```

Macro output should make the following distinctions explicit:

- value bindings versus relation declarations;
- relation variables versus host-language identifiers;
- positive versus negative body literals;
- constructor application versus host-language computation;
- requested result fields versus anonymous intermediate variables;
- compile-time diagnostics versus runtime query failures.

The [Racket syntax model](https://docs.racket-lang.org/reference/syntax-model.html) states that expansion recursively processes syntax objects and uses their binding information. The macro layer therefore owns hygienic name resolution. The logic layer receives resolved identities and does not reconstruct binding identity from printed names.

## Logic boundary

The phase-0 logic IR should be a versioned, ground description of a rule request:

```text
logic-program(
  relations: relation(name, arity, source-span),
  seeds:     call(relation, ground-terms),
  rules:     rule(head, ordered-body, source-span),
  outputs:   ordered-variable-or-term-requests,
  variables: clause-local generated IDs
)
```

The initial native route is Racket `datalog`:

- function-free Horn clauses;
- every rule-head variable appears in a body literal;
- finite ground constants for the compiler request;
- variant-keyed subgoal tables and propagated facts for recursion;
- query and theory state owned by one compiler request.

The [Datalog module-language documentation](https://docs.racket-lang.org/datalog/datalog.html) specifies safe rules, assertions, retractions, queries, equality, inequality, and external queries. It warns that external queries can break the termination guarantee. Phase 0 should keep host calls outside recursive Datalog strata or place them behind an explicit bounded adapter.

The local Datalog source shows the storage sequence used by the installed package:

1. `make-theory` creates a mutable theory keyed by predicate and arity.
2. Assertions and retractions update clause lists in that theory.
3. Each `prove` call creates a fresh variant-keyed subgoal table.
4. A subgoal stores discovered facts and waiting clauses.
5. New facts resolve waiting clauses and propagate additional facts.
6. The query returns the facts collected for the requested subgoal.

The local retraction probe changed `path(a,Y)` from `Y=b,c` to `Y=b` after removing `edge(b,c)`. That receipt establishes current-query correctness for the finite probe. It does not establish incremental reuse of completed recursive tables, because the installed `prove` implementation allocates its subgoal table per call. Incremental dependency maintenance remains a separate DL7 component.

The alternative routes have separate ownership:

| Need | Racket route | Phase-0 boundary |
| --- | --- | --- |
| Prolog-style unification and answer streams | Racklog `%=` plus `%which` and `%more` | Adapter. Record occurs-check policy, answer order, duplicate behavior, and query lifetime. |
| Relational constraints | cKanren package family | Package boundary. Pin a source commit and run the finite-domain, disequality, type, and absento probes before selection. |
| Optimized relational compilation | hosted miniKanren | Package boundary. Treat catalog status and dependency behavior as inputs to a separate probe. |
| SMT-backed symbolic constraints | Rosette | Solver boundary. Convert solver results and unknown states into DL7 diagnostics and preserve symbolic-term ownership. |
| General SWI tabling, CHR, CLP(Q/R), and attributed variables | retain or embed SWI | External-runtime boundary. Define engine attachment, foreign frames, table lifecycle, exceptions, and term serialization. |

## Result and storage boundary

Every evaluator call should receive a request-owned theory or an immutable snapshot plus a generation identifier. The result should be canonicalized before it crosses the runtime boundary:

```text
result = {
  request-id,
  relation-name,
  ordered-columns,
  sorted-ground-rows,
  diagnostics,
  evaluator-route,
  source-spans
}
```

Storage rules for phase 0:

- relation identity uses resolved module and binding identity, predicate name, and arity;
- clause-local logic variables use generated IDs rather than reader symbols;
- constructor identity and argument order are serialized explicitly;
- source spans remain attached to clauses, terms, and diagnostics;
- recursive table state is request-owned and generation-scoped;
- updates carry fact ownership so a retraction can identify affected derived rows;
- external host calls are bounded and carry an explicit conversion boundary;
- sets of rows are sorted by a documented total order before emission.

The supplied [runtime shootout README](../18_runtime_shootout/0_README.md) separates setup from closure evaluation and validates materialized counts. Its [results](../18_runtime_shootout/5_RESULTS.md) record correct Racket 9.3 counts at `N=48`, with a median process startup of 340.000 ms, chain closure of 495.746 ms, and ring closure of 1966.985 ms. These values belong to the selected `datalog` route and should remain separate from a future phase-0 benchmark for reader, expansion, lowering, and serialization costs.

## Packaging boundary

Use a module-based entry point with `#lang racket/base` or the selected DL7 language. The packaging sequence is:

```text
raco make source modules
  -> raco exe entry-module
  -> inspect executable and reachable runtime files
  -> raco distribute distribution-dir executable
```

The current [`raco exe` manual](https://docs.racket-lang.org/raco/exe.html) states that required modules are embedded through the module dependency graph. Modules reached only through `eval`, `load`, or `dynamic-require` need `++lib` or another explicit inclusion mechanism. Language readers used only through dynamic `#lang` loading need `++lang`. Runtime files can use `define-runtime-path`.

The current [`raco distribute` manual](https://docs.racket-lang.org/raco/exe-dist.html) states that the command collects shared libraries and runtime files for machines running the same operating system. On macOS, a non-GUI executable is placed under `bin` and frameworks under `lib`.

Local packaging receipt:

- `raco exe` built the existing Racket 9.3 Datalog arm in a temporary directory. The executable was a 11,774,987-byte arm64 Mach-O file and returned the expected chain count `6` at `N=4`.
- The first `raco distribute` attempt failed while patching a copied read-only Mach-O executable. After owner write permission was added to that temporary file, distribution completed; the packaged executable returned the expected ring count `16` at `N=4`.
- The successful packaging receipt covers the installed Racket arm and `datalog` dependency. It does not cover dynamic DL7 language loading, SWI embedding, or multi-platform artifact production.

## Open phase-0 blockers

The following receipts remain required before choosing a broader logic route:

- cKanren, baseline miniKanren, hosted miniKanren, and Rosette have current catalog records but no local execution in this lab because package installation was intentionally avoided.
- No local probe covers SWI embedding through Racket FFI, foreign-thread engine attachment, or exception and term conversion.
- The package catalog command could not resolve `download.racket-lang.org` from the sandbox; official current catalog and documentation pages were used for source receipts.
- The original brief’s Racket-absent statement is stale. Local `command -v racket` resolves `/opt/homebrew/bin/racket`, and `racket --version` reports `Welcome to Racket v9.3 [cs]`.
