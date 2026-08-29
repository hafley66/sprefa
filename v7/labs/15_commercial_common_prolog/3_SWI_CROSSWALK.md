# SWI-Prolog Crosswalk

Research and source-check date: 2026-08-29.

This table compares the exact SWI facilities used by the Common Lisp logic reference against the checked vendor documentation. A `covered` entry means the product documents a directly corresponding facility. `partial` means a nearby facility is documented but the SWI semantics or required compiler boundary remains unspecified. `undocumented` means the checked vendor sources do not establish the facility. No product execution was performed.

| SWI facility | Allegro Prolog | LispWorks Common Prolog | KnowledgeWorks | Remaining boundary |
| --- | --- | --- | --- | --- |
| Reader terms, symbols, and interning | `partial`: Lisp S-expression terms and `?` variables are documented; SWI reader and variable identity semantics are not | `partial`: Lisp-like terms, list/vector terms, and an Edinburgh translator are documented; SWI reader semantics are not | `partial`: uses Common Prolog terms plus CLOS objects; SWI reader semantics are not | Define a stable term ABI, variable identity, reader syntax, and source spans. |
| Macro expansion and phase-0 lowering | `partial`: `<-`, `<--`, and Lisp macros are documented; a compiler phase contract is not | `partial`: `defrelmacro` and `defrel-special-form-macro` are documented; syntax-object expansion is not | `partial`: rule macros and the Meta Rule Protocol are documented; a phase-0 lowering contract is not | Preserve source locations, binding identity, polarity, and rule ownership. |
| First-order nested unification | `covered`: `=/2`, `arg/3`, `functor/3`, `copy-term/2`, and nested examples | `covered`: `=`, `arg`, `functor`, `=..`, vectors, lists, and `defdetunipred` | `covered` through the Common Prolog backward chainer and KnowledgeWorks object matching | Occurs-check behavior, cyclic terms, and canonical substitution output remain undocumented. |
| Occurs check | `undocumented` | `undocumented` | `undocumented` | Verify policy with a product probe before using cyclic-term claims. |
| Backtracking and multiple answers | `covered`: `?-`, `prolog`, `first`, `bagof`, and `setof` | `covered`: `rqp`, `logic :all`, `any`, `findall`, and `findallset` | `covered` for backward goals through Common Prolog | Fairness, starvation, duplicate policy, cancellation, and exact answer order need receipts. |
| Fair search | `undocumented` | `undocumented` | `undocumented` | A finite relational fairness probe is required. |
| Function-free Horn rules | `partial`: facts and rules are documented, with general Prolog terms and CL calls | `partial`: `defrel` rules are documented, with general Prolog terms and Lisp goals | `partial`: forward and backward rules are documented, with CLOS object conditions | Datalog safety, bottom-up semantics, and rule stratification are not established. |
| Recursive least fixpoint | `undocumented` | `undocumented` | `partial`: RETE forward inference reaches a rule agenda, but Datalog least-fixpoint equivalence is undocumented | A cyclic closure receipt with explicit semantics is required. |
| General Prolog tabling and SLG completion | `undocumented` | `undocumented` | `undocumented` | No table directive, variant table, suspended consumer, or completion API is documented. |
| Variant or subsumptive call tables | `undocumented` | `undocumented` | `undocumented` | Requires a table key, answer store, consumer suspension, and completion receipt. |
| Answer subsumption and lattice tabling | `undocumented` | `undocumented` | `undocumented` | No lattice answer policy or table update API is documented. |
| Well-founded negation and delayed negative goals | `partial`: `not/1` is listed; WFS is undocumented | `partial`: `not` is listed; WFS is undocumented | `partial`: forward `not` conditions exist; WFS is undocumented | Negation-as-failure, stratification, undefined truth, and delay behavior need separate definitions. |
| Dynamic facts, assertions, and retractions | `covered` for vendor-specific compiled and recorded databases; semantics differ from SWI | `covered` for `asserta`, `assertz`, `retract`, `erase`, `recorda`, `recordz`, and `recorded` | `partial`: object-base `assert` and `erase`, truth maintenance, and backward clause updates are documented | Incremental dependency repair and SWI update atomicity are undocumented. |
| Incremental tabling after updates | `undocumented` | `undocumented` | `undocumented` | Mutable clauses, RETE working memory, and truth maintenance do not establish SWI incremental table reuse. |
| Constraint logic over finite domains | `undocumented` | `undocumented` | `undocumented` | No CLP(FD) store, propagation API, or finite-domain semantics are documented. |
| Rational and real arithmetic constraints | `undocumented`: arithmetic can call Lisp, but a constraint store is not documented | `undocumented`: arithmetic Lisp goals and `is` are documented, CLP(Q/R) is not | `undocumented` | Solver domains, propagation, residual constraints, and unknown handling remain absent from the checked sources. |
| Constraint Handling Rules | `undocumented` | `undocumented` | `undocumented` | No CHR syntax, token store, propagation history, or confluence behavior is documented. |
| Attributed variables | `undocumented` | `undocumented` | `undocumented` | CLOS objects and KnowledgeWorks dependencies are separate mechanisms. |
| Coroutining, freeze, and delayed wakeup | `undocumented` | `undocumented` | `undocumented` | No attributed-variable wake queue or suspension primitive is documented. |
| DCG lowering | `undocumented` | `covered`: `defgrammar`, sentence tails, extra arguments, Lisp clauses, calls, and cut | `covered` for KnowledgeWorks backward rules when using the inherited Common Prolog DCG facility | Common Prolog DCGs require an adapter for SWI grammar syntax and exact expansion/source mapping. |
| Term inspection and construction | `covered`: `arg`, `functor`, `copy-term`, `ground`, and list terms | `covered`: `arg`, `functor`, `=..`, vectors, lists, and `read-term` | `covered` through inherited Common Prolog plus CLOS object predicates | Define constructor identity, cyclic-term policy, source metadata, and host-value escape rules. |
| Module and namespace system | `partial`: CL packages exist, and the manual explicitly says there is no Prolog module system | `partial`: CL packages and systems exist; a Prolog module system is undocumented | `partial`: KnowledgeWorks contexts and rule names provide organization, not SWI modules | Predicate visibility, import/export, module generation, and cross-module symbol keys require a compiler policy. |
| Source locations and hygienic binding | `undocumented` | `undocumented` | `undocumented` | Common Lisp macro objects and Prolog `?` symbols do not establish SWI source and scope metadata. |
| Graph SCCs, topological order, and worklists | `undocumented` | `undocumented` | `partial`: RETE networks and agendas are documented, but SCC and compiler worklist APIs are not | DL7 relation polarity, SCC strata, deterministic ordering, and invalidation ownership remain to be implemented. |
| Foreign C API | `partial`: Allegro CL has host FFI documentation, but no Allegro Prolog foreign API receipt | `partial`: LispWorks FLI is documented, but no Common Prolog foreign API receipt | `partial`: LispWorks FLI and Lisp integration are documented, but no KnowledgeWorks foreign rule ABI receipt | Define ABI ownership, term lifetimes, exception conversion, and thread boundaries. |
| Standalone executable | `partial`: Allegro CL runtime delivery is documented; Prolog-specific packaging is undocumented | `partial`: LispWorks image saving and delivery are documented; Prolog artifact closure is unmeasured | `partial`: KnowledgeWorks delivery is documented for Enterprise and mobile runtime products; artifact closure is unmeasured | Product-specific executable bytes, dynamic dependencies, startup, RSS, and delivery contents are unavailable. |
| Embedding SWI | `undocumented` | `undocumented` | `undocumented` | The checked commercial product documents Lisp integration, not an SWI engine, `libswipl`, or SWI query lifecycle. |
| Concurrent isolated query engines | `undocumented` | `undocumented` | `partial`: independent inferencing states are documented; thread-safe SWI-style engine isolation is not | Define ownership for variables, clause stores, tables, CLOS objects, updates, and cancellation. |
| Saved compiler state | `partial`: Allegro CL images and runtime delivery are host facilities; Prolog state persistence is undocumented | `partial`: preloaded LispWorks images are documented; Prolog table/database persistence is undocumented | `partial`: independent inferencing state objects and delivered images are documented; SWI saved-state semantics are not | Define versioned IR, resource paths, state generations, migrations, and cache validation. |

## Covered SWI facilities

The checked documentation directly covers these facility families:

- Allegro Prolog: first-order term unification and inspection, depth-first backtracking with multiple answers, cut, Lisp interoperation, vendor-specific assertion and record databases, and four-port leash tracing.
- LispWorks Common Prolog: all of the Allegro-covered families except the exact Allegro database split, plus DCG definition, Edinburgh syntax translation, mode declarations and indexing, `logic` result collection modes, `findallset`, and interactive four-port debugging.
- KnowledgeWorks: the inherited Common Prolog backward-chaining facilities, forward rule execution over CLOS objects, RETE working-memory matching, contexts and conflict resolution, logical dependency truth maintenance, and independent inferencing state objects.

## Missing or undocumented SWI facilities

The checked documentation does not establish an implementation for either commercial system for occurs-check policy, fair search, general tabling, SLG completion, variant or subsumptive tables, answer subsumption, incremental tabling, CLP(FD), CLP(Q/R), CHR, attributed variables, coroutining, source-located hygienic binding, SWI embedding, or a measured standalone artifact.

KnowledgeWorks has forward-chaining and truth-maintenance mechanisms, but the sources do not establish SWI least-fixpoint Datalog semantics, incremental tabling, well-founded negation, or SWI dynamic-update behavior for those mechanisms. Common Prolog DCGs and query collection APIs require an adapter for SWI-compatible syntax, source mapping, and exact answer semantics.

## SWI reference receipts

- [SWI tabling manual](https://www.swi-prolog.org/pldoc/man?section=tabling)
- [SWI constraint logic programming manual](https://www.swi-prolog.org/pldoc/man?section=clp)
- [SWI CHR manual](https://www.swi-prolog.org/pldoc/man?section=chr)
- [SWI saved states manual](https://www.swi-prolog.org/pldoc/man?section=saved-states)
- [SWI foreign threads and engines manual](https://www.swi-prolog.org/pldoc/man?section=foreignthread)
- [SWI embedding manual](https://www.swi-prolog.org/pldoc/man?section=embedded)
