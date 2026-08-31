# Commercial Common Prolog Capabilities

Research and source-check date: 2026-08-29. This report uses vendor documentation only. It contains no product execution receipt.

Status meanings:

- `documented`: the checked vendor source describes the facility or gives an API/example.
- `partial`: a related facility is documented, with a semantic or packaging boundary against the requested capability.
- `undocumented`: the checked vendor sources do not establish the feature.
- `unavailable`: a checked source explicitly excludes the facility for the product or edition.

## Allegro Prolog inside Allegro CL

The product page describes Allegro Prolog as an integrated Common Lisp extension. The Prolog manual identifies Allegro Prolog 1.1.2 for Allegro CL 10.1. It states that predicates compile to Common Lisp functions, with one function combining the rules for each functor/arity, and that predicates may be loaded interpreted or compiled with automatic compilation on first use.

| Requested capability | Status | Documentation evidence and boundary |
| --- | --- | --- |
| Syntax and terms | `documented` | Lisp S-expression syntax is used. Variables are symbols whose names begin with `?`; the anonymous variable is `?`. Operator syntax and Edinburgh syntax are not provided by the checked manual. |
| Nested-term unification | `documented` | `=/2`, `copy-term/2`, `arg/3`, `functor/3`, `ground/1`, `var/1`, and examples using nested list structures are documented. The exact internal term representation is implementation-owned. |
| Occurs check | `undocumented` | The checked manual contains no occurs-check policy or API. Cyclic unification behavior is therefore unavailable as a verified claim. |
| Compilation model | `documented` | Rules for a functor/arity are combined into a compiled Lisp function. Facts with ground heads are captured as data. A predicate is compiled on first use, and `prolog-compile-symbols` can compile predicates needing recompilation. |
| Backtracking and multiple answers | `documented` | `?-` displays solutions and requests more answers through the Lisp listener. `prolog` invokes a conjunction programmatically. `first/1`, `last/1`, `bagof/3`, and `setof/3` are listed built-ins. The manual examples show depth-first clause order. |
| Cut | `documented` | `!` is the Prolog cut and may be written as `!` or `(!)`. |
| DCG | `undocumented` | The checked Allegro Prolog manual has no DCG or `defgrammar` facility. |
| Debugging | `documented` | `leash` traces call, exit, redo, and fail ports. `unleash`, `leash-1`, `*leash-limit*`, and trace examples are documented. The manual records limitations and pending extensions for the leash interface. |
| Lisp interoperation | `documented` | `lisp`, `lisp*`, `lispp`, `is`, `lisp!`, `let`, `let*`, `unwind-protect`, `slot=`, `slot=*`, and `slot-value` bridge between Prolog and Lisp. `prolog` is the Lisp-to-Prolog macro. Dynamic-extent copying rules are documented for data escaping a Prolog continuation. |
| Constraints | `undocumented` | No CLP, finite-domain, rational, real, or CHR facility is established by the checked Prolog manual. Standard arithmetic and predicate operators that are omitted can be reached through Lisp calls where applicable. |
| General tabling | `undocumented` | The checked manual has no table directive, table API, SLG completion description, call table, or answer table. Cycle termination through tabling is unavailable as a verified claim. |
| Dynamic updates | `documented` | `assert/1`, `asserta/1`, `assertz/1`, `abolish/2`, `recorda/1`, `recordz/1`, `recorded/2`, and `retract/1` are documented. The manual separates the compiled predicate database from the recorded hash-table database. Updates to a compiled functor/arity invalidate it and cause recompilation on the next call. |
| Executable delivery | `partial` | Allegro CL 11.0 documents saved images and runtime application creation through the host delivery system. The checked Prolog manual does not document a Prolog-specific standalone artifact or its inclusion rules. Runtime rights depend on the Allegro CL license and edition. |
| Embedding | `partial` | Prolog is directly embedded in Allegro CL and has a bidirectional Lisp/Prolog calling interface. Product-specific embedding of Allegro Prolog into an external C, Rust, or other host is undocumented in the checked sources. |

### Allegro Prolog source examples

The vendor manual supplies a finite `append/3` query with four answers, recursive `member` and `rev-member` definitions, a `leash` trace, a package-inheritance recorded database, generator closures, CLOS slot access, and the zebra problem with a benchmark. These examples establish the documented surface. They do not establish occurs-check policy, tabling, constraint semantics, or cycle termination.

## LispWorks Common Prolog

The LispWorks 8.1 guide describes Common Prolog as a logic system within Common Lisp. It uses simple vectors for Prolog structured terms, compiles predicates to Lisp functions, and bases the implementation loosely on the WAM. `defrel` accepts mode declarations that control indexing and generated code. The Common Prolog query interface is available through `rqp`, and the Lisp interface is centered on `logic`.

| Requested capability | Status | Documentation evidence and boundary |
| --- | --- | --- |
| Syntax and terms | `documented` | Lisp-like prefix syntax uses `?` variables. The guide documents list and vector goals, simple vectors for structured terms, destructuring, `defrel`, and an Edinburgh syntax translator. |
| Nested-term unification | `documented` | `=`, `arg`, `functor`, and `=..` are documented with list and vector examples. `defdetunipred` is the extension point for Lisp-defined predicates that need unification. |
| Occurs check | `undocumented` | The checked 8.1 guide contains no explicit occurs-check policy or configuration. |
| Compilation model | `documented` | Common Prolog predicates compile into Lisp functions. `defrel` supports mode declarations `?`, `?*`, `+`, and `-`, which control clause indexing and assumptions about argument binding. The guide documents compilation in the listener and `compile_and_reconsult` for Edinburgh files. |
| Backtracking and multiple answers | `documented` | `rqp` retrieves more solutions with semicolon. `logic` supports first result, multiple values, lists, bags, and alists. `any`, `findall`, and `findallset` are documented. |
| Cut | `documented` | `cut` is a Common Prolog built-in. KnowledgeWorks backward rules also expose cut through the shared backward-chaining system. |
| DCG | `documented` | `defgrammar` defines a DCG relation with sentence, sentence tail, optional extra arguments, Lisp clauses, calls, and cut. The guide includes grammar examples. |
| Debugging | `documented` | Common Prolog supplies a four-port call/exit/redo/fail model, exhaustive tracing, spy points, leashing, interactive debugger commands, and a graphic logic environment with source-level stepping and call trees. |
| Lisp interoperation | `documented` | A goal whose first element is a list evaluates Lisp and unifies returned values. `logic`, `any`, `findall`, `findallset`, and `with-prolog` call Prolog from Lisp. `defdetpred` and `defdetunipred` add Lisp-defined predicates. |
| Constraints | `undocumented` | The checked 8.1 Common Prolog guide has no CLP(FD), CLP(Q), CLP(R), or CHR facility. Lisp arithmetic calls are interoperation, not a documented constraint store. |
| General tabling | `undocumented` | The checked guide has no table directives, SLG completion, variant/subsumptive table API, answer subsumption, or completed call-table description. Cycle termination through tabling is unavailable as a verified claim. |
| Dynamic updates | `documented` | `asserta`, `assertz`, `retract`, `erase`, `recorda`, `recordz`, and `recorded` are listed. `findallset` removes duplicates at collection time. This documents mutable clause and record stores, with no incremental table semantics established. |
| Executable delivery | `partial` | The guide documents saving an image with Common Prolog loaded. LispWorks 8.1 documents optimized application delivery, and Enterprise delivery includes KnowledgeWorks. A Prolog-specific artifact inventory, size, startup, and dependency receipt is unavailable. |
| Embedding | `partial` | Common Prolog is embedded in LispWorks and has documented calls in both directions. An external-host embedding API for Common Prolog is undocumented in the checked sources. |

### Common Prolog source examples

The 8.1 guide includes the translation of standard `append/3`, recursive `reverse/2`, factorial through Lisp arithmetic goals, `logic` return modes, a `with-prolog` palindrome function, custom logic macros, DCG grammars with extra arguments, Edinburgh file translation, and debugger transcripts.

## KnowledgeWorks

KnowledgeWorks contains Common Prolog as its backward chainer and adds a separate forward-chaining system. The product page describes forward chaining over a CLOS object base, backward chaining over goals, contexts and conflict resolution, multiple independent inferencing states, and truth maintenance. The implementation appendix describes a RETE forward chainer and a WAM-based backward chainer. These facilities are recorded under KnowledgeWorks rather than attributed to Common Prolog alone.

| KnowledgeWorks facility | Status | Documentation evidence and boundary |
| --- | --- | --- |
| Backward Prolog rules | `documented` | `defrule` supports `:backward`; the backward engine extends Common Prolog and can match KnowledgeWorks CLOS objects. |
| Forward rules | `documented` | `defrule` supports `:forward`; conditions match the object base and actions can assert, erase, call Lisp, invoke backward goals, and change contexts. |
| RETE rule evaluation | `documented` | The implementation notes describe a RETE network of shared conditions and tracked instantiations. |
| Truth maintenance | `documented` | A `logical` forward condition records dependencies so objects created by a rule can be erased when those conditions cease to hold. |
| Rule contexts and conflict resolution | `documented` | Contexts organize agendas and meta-interpretation. Priority, recency, specificity, and user-defined tactics are documented. |
| Multiple inferencing states | `documented` | KnowledgeWorks documents independent inferencing state objects. Thread isolation and SWI engine equivalence are undocumented. |
| Lisp and CLOS integration | `documented` | KnowledgeWorks uses CLOS classes and objects, and both rule directions call Lisp. |
| SWI-compatible tabling, CLP, CHR, attributed variables | `undocumented` | The checked KnowledgeWorks and Prolog sources do not establish these SWI mechanisms. RETE working memory and truth maintenance do not supply a verified replacement for them. |

## Capability boundary for this lab

The documented direct Prolog surfaces are compiled first-order rules, unification, depth-first multiple-answer search, cut, Lisp interoperation, dynamic clause or record updates, and vendor-specific debugging. LispWorks Common Prolog additionally documents DCGs, Edinburgh syntax translation, structured-term inspection, and richer query return modes. KnowledgeWorks adds RETE forward chaining, CLOS pattern matching, contexts, inferencing states, and truth maintenance.

The following remain `undocumented` in the checked product sources: occurs-check policy for both implementations, general tabling and SLG completion, variant or subsumptive tables, answer subsumption, well-founded negation, CLP(FD), CLP(Q/R), CHR, attributed variables, coroutining, source-located hygienic binding, SWI embedding, and measured standalone artifacts.
