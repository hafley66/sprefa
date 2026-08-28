# DL7 Phase-0 Boundary in Racket

Research date: 2026-08-28. This is a boundary description, not an implementation or execution report. All Racket-specific facts are marked **Documented** and link to [1_SOURCES.md](1_SOURCES.md). DL7-specific representation choices are **Inferred**.

## Interface

```text
read-source  : bytes + source-name -> syntax objects
expand       : syntax objects + module context -> expanded syntax objects
lower        : expanded syntax objects -> DL7 core graph
logic-plan   : DL7 core graph -> restricted Datalog theory | Racklog goals | solver request
package      : module entrypoint + static dependencies -> executable + distribution
```

All five interface signatures are **Inferred** DL7 boundaries. The linked
Racket receipts establish the facilities used on either side of each boundary,
not these exact names or data shapes.

`read-source` and `expand` are separate because Racket defines a read pass followed by an expand pass. `read-syntax` creates syntax objects with source locations and initially empty lexical information; expansion adds and consumes binding information. **Documented.** [S11, S12]

## Reader boundary

| Boundary | Input | Output | Receipt / unresolved work |
| --- | --- | --- | --- |
| Reader | source character stream | syntax object tree | **Documented:** the reader supports `read-syntax` and configurable readtables. [S12] |
| Language entry | `#lang` declaration | reader and expander starting point | **Documented:** Racket supports module languages, reader extensions, and `#lang` language definitions. [S13] |
| DL7 punctuation | DL7 source spelling | syntax object or reader-produced datum | **Inferred:** choose a readtable extension only if DL7 requires syntax outside Racket’s lexical conventions; otherwise lower ordinary syntax objects. |
| Locations | syntax object | DL7 location field | **Inferred:** preserve source name, line, column, position, and span as fields at the first lowering boundary. The source receipt establishes source locations, not a DL7 schema. [S11, S12] |

The `datalog/sexp` language provides an official example of a parenthetical language layered on the same Datalog semantics. **Documented.** It is a reference for module-language packaging, not evidence that it accepts DL7 syntax. [S03]

## Scope and macro boundary

Racket represents identifiers as syntax objects. Scope sets and phase levels determine bindings; phase 0 is module runtime and phase 1 is transformer time. **Documented.** [S11]

| Stage | Storage | Reads | Writes | Uniqueness condition |
| --- | --- | --- | --- | --- |
| Parse | syntax object | source stream | datum, source location, initial lexical information | **Documented:** reader result wraps source-located syntax. [S12] |
| Expand | syntax object plus module/phase context | binding and transformer environments | expanded syntax with binding information | **Documented:** bindings are determined by identifier symbol plus scope-set relation at a phase. [S11] |
| Lower | DL7 core graph | expanded syntax and identifier comparisons | nodes, edges, source references, binding references | **Inferred:** assign each lowered binder/reference an explicit stable ID; retain source syntax only as provenance. |
| Encode | stable DL7 graph | core graph | logic facts/rules or serialized graph | **Inferred:** logic-variable IDs and binding IDs must be represented separately. |

Macro expansion can introduce bindings and can expand subexpressions at deeper phases. **Documented.** [S11] Therefore, phase-0 lowering must select an expansion boundary before converting identifiers to DL7 symbols. The selection itself is **Inferred** and requires a DL7 test corpus.

## Logic boundary

| DL7 logic need | Racket route | Documented boundary | Unresolved before assignment |
| --- | --- | --- | --- |
| finite, function-free Horn rules | `datalog` / `datalog/sexp` | Function-free safe Horn clauses; tabling intermediate results; Datalog API returns substitution dictionaries and supports assertion/retraction. [S01, S03] | Exact term encoding, duplicate order, update-to-derived-answer behavior, and fixture result. |
| relation unification and answer enumeration | Racklog | `%=` unifies, `%which` returns a solution, `%more` requests another; occurs check is disabled by default but configurable. [S06] | Fairness, cyclic recursion, tabling, and query isolation. |
| relational package candidates | miniKanren, cKanren | Catalog receipts identify packages; cKanren’s catalog tests fail. [S07, S08] | Public API, constraints, reification, scheduling, and shared-fixture behavior. |
| solver-backed symbolic constraints | Rosette | Rosette is a solver-aided Racket language with a symbolic VM. [S09] | Mapping from DL7 constraints to solver constraints, supported domain, model extraction, and reproducibility. |

For a Datalog target, the theory boundary can be built from the documented `make-theory`/`datalog` API. **Documented.** [S03] This does not provide a receipt for general Prolog tabling, answer subsumption, well-founded negation, CHR, or incremental tabling. Those are separate unresolved rows in [2_SWI_CROSSWALK.md](2_SWI_CROSSWALK.md).

## Packaging boundary

```text
DL7 Racket module
  -> static require graph
  -> raco exe
  -> executable with embedded module bundle
  -> raco distribute
  -> same-OS distribution directory
```

`raco exe` embeds a module and statically required modules. Modules reached only by `eval`, `load`, or `dynamic-require`, and reader modules used only through `#lang`, need explicit packaging controls such as `++lib` and `++lang`. **Documented.** [S14] `raco distribute` carries needed shared libraries and run-time files declared with `define-runtime-path`; it targets machines on the same operating system. **Documented.** [S15]

## Explicit unresolved rows at the time limit

| Row | Reason |
| --- | --- |
| local examples, shared cyclic fixture, executable measurements | Racket is absent from `PATH`; installation is excluded by the brief. |
| Racklog fairness, tabling, SLG, cyclic termination, duplicate answers, isolation | No current official documentation receipt found in the scoped sources; no local probe. |
| miniKanren semantics | Catalog/repository receipt only; no current official manual receipt or local probe. |
| cKanren constraint semantics and usable version | Catalog describes the package, lists candidate modules, and reports failing tests; no local probe. |
| Rosette equivalence to SWI CLPFD, CLPQ, CLPR, CHR, attributed variables, or coroutining | Rosette’s solver-aided model is documented; no equivalence receipt exists in the reviewed sources. |
| stable DL7 AST, scope-ID, logic-variable-ID, source-location, serialization, and update policy | DL7 implementation decisions are outside the Racket documentation. |
