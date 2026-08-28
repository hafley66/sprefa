# SWI to Racket Crosswalk

Research date: 2026-08-28. Labels: **Documented** cites a receipt in [1_SOURCES.md](1_SOURCES.md); **Inferred** is a bounded implementation conclusion; **Unresolved** has no supporting receipt or local probe. “Missing machinery” is scoped to the named SWI facility.

| SWI facility useful to DL7 | Shortest Racket route | Semantics receipt | Missing machinery / status |
| --- | --- | --- | --- |
| Reader terms, symbols, interning | Racket reader plus `read-syntax` | Syntax object, source location, reader configuration, default-reader interning, and fresh uninterned symbols are documented. [S11, S12, S16] | **Documented**. DL7 token and AST policy remains an adapter. |
| Macro expansion and phase-0 lowering | syntax objects, expansion, a module language or `#lang` reader | Read then expand; scopes and phase levels drive binding. [S11, S13] | **Documented**. DL7 core-form schema and lowering rules are **Inferred** work. |
| First-order unification | Racklog `%=` | Recursive structural comparison; occurs check is configurable and disabled by default. [S06] | **Documented**. Compound-term interchange and selected occurs policy must be explicit. |
| Backtracking and multiple answers | Racklog `%which` then `%more` | `%more` retries the preceding `%which` query for another solution. [S06] | **Documented**. Fairness and duplicate policy are **Unresolved** without a probe. |
| Function-free Horn rules | `#lang datalog` or `#lang datalog/sexp` | Datalog is function-free Horn with safe clause heads. [S01, S03] | **Documented**. DL7 terms outside that domain require another representation. |
| Recursive least fixpoint | Datalog | Manual states tabling of intermediate results and termination for its function-free safe-Horn language. [S01] | **Inferred route**: the scoped receipt does not state least-model/fixpoint equivalence or name a bottom-up or seminaive algorithm. |
| General Prolog tabling and SLG completion | No documented route in the researched facilities | Datalog documents termination in its restricted language; Racklog documentation contains no tabling or SLG claim. [S01, S04] | **Unresolved**: call tables, consumer suspension, SCC completion, and a general-Prolog domain. |
| Variant or subsumptive call tables | No documented route | Datalog source catalog lists a `variant` module, but this does not document call-table semantics. [S02] | **Unresolved**. |
| Answer subsumption and lattice tabling | No documented route | Rosette compiles symbolic execution to logical constraints; that is a different documented model. [S09] | **Unresolved**: table answer order, lattice, and aggregation semantics. |
| Well-founded negation and delayed goals | No documented route | Racklog documents negation as failure; Datalog documentation reviewed here does not establish well-founded or delayed-goal semantics. [S04] | **Unresolved**. |
| Dynamic facts, assertions, and retractions | Datalog theory API | `!` asserts, `~` retracts a literal, and a theory is mutated by statements. [S03] | **Documented**. Incremental-table behavior after updates is separate. |
| Dynamic Racklog facts | Racklog `%assert!` or `%assert-after!` | Clauses can be added after `%rel`; manual explicitly says Racklog has no retraction predicate. [S06] | **Documented**. Retraction requires a host-level versioning adapter. |
| Incremental tabling after updates | No documented route | Datalog documents assertions/retractions and tabling but does not document incremental dependency maintenance. [S01, S03] | **Unresolved**. |
| Finite-domain constraints | cKanren catalog candidate | Catalog describes cKanren as constraint programming and lists `unstable/fd`; catalog tests fail. [S08] | **Unresolved**: public API, propagation, finite-domain semantics, and current runnable status. |
| Rational and real arithmetic constraints | Rosette candidate | Rosette exposes a solver-aided language and symbolic VM, without a documented SWI `clpq`/`clpr` equivalence in reviewed sources. [S09] | **Unresolved**: domain, solver mapping, and relational mode. |
| Constraint Handling Rules | No documented route | No researched receipt documents CHR. | **Unresolved**: rule store, propagation history, wakeup, and confluence policy. |
| Attributed variables | cKanren-family candidate | Catalog lists cKanren attributes and constraint-store modules, but gives no semantics; tests fail. [S08] | **Unresolved**. |
| Coroutining, `freeze`, delayed wakeup | No documented route | Racklog continuation-based backtracking is documented; it does not document variable wakeup or `freeze`. [S04] | **Unresolved**. |
| DCG lowering | Racket macro or custom `#lang` | Racket documents macro and reader/language extension layers. [S11, S13] | **Inferred**: a hidden-state-argument lowering can be written. No DCG library receipt was researched. |
| Term inspection and construction | Racket data and syntax objects | Reader and syntax objects are documented; Racklog accepts Racket values in unification examples. [S06, S12] | **Documented** host substrate. DL7 term tags and traversal are **Inferred** adapters. |
| Module and namespace system | Racket modules, phases, `#lang` | Modules, phases, scope sets, and language readers are documented. [S11, S13] | **Documented**. Predicate import/export policy is **Inferred** compiler work. |
| Source locations and hygienic binding | `read-syntax`, syntax objects, scope sets | Syntax objects carry source locations and lexical information; binding is determined through scope sets. [S11, S12] | **Documented**. Stable DL7 binding IDs and serialized location schema are **Inferred** work. |
| Graph SCC, topological order, worklists | Host-level implementation | No graph package or base-API receipt was researched for this crosswalk. | **Unresolved**. |
| Foreign C API | No documented route in this receipt set | No Racket FFI source receipt was researched. | **Unresolved**. |
| Standalone executable | `raco exe`, then `raco distribute` | `raco exe` embeds module code; `raco distribute` carries declared run-time files and needed shared libraries. [S14, S15] | **Documented** build route. Bytes, dependencies, startup, and platform output are **Unresolved** without Racket. |
| Embedding SWI | No documented route in this receipt set | No `libswipl` integration source receipt was researched. | **Unresolved**: Racket FFI shape, term lifetime, engine attachment, exception conversion, and thread ownership. |
| Concurrent isolated query engines | No documented logic-engine route | No Racklog, Datalog, miniKanren, cKanren, or Rosette receipt reviewed here defines query/table ownership or isolation. | **Unresolved**. |
| Saved compiler state | module data plus executable/distribution packaging | `raco exe` embeds module bundles; Datalog can read/write theories, losing source locations on write. [S03, S14] | **Documented** mechanisms. Compiler graph persistence, identity preservation, and schema are **Inferred** work. |

## Corrections to the supplied hypothesis

1. The Datalog manual documents **tabling intermediate results** and termination, rather than naming a bottom-up or seminaive evaluator. [S01]
2. The Datalog interoperability API documents both assertion and retraction. [S03]
3. Racklog documents `%assert!`, but explicitly has **no retraction predicate**. [S06]
4. Racklog’s occurs check is configurable and disabled by default. [S06]
5. The reviewed Racklog documentation does not provide a tabling, SLG, fairness, or query-isolation receipt. Those rows are unresolved rather than runtime claims. [S04, S06]
6. cKanren is a catalog candidate only: its catalog build compiled while its catalog tests failed. [S08]
