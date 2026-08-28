# Lab 15 SWI Crosswalk: Allegro Prolog and LispWorks Common Prolog vs SWI-Prolog

Question: which SWI-Prolog facilities are covered by the two commercial
Common Lisp Prologs, which need adapters, and which are absent from their
documentation? Status vocabulary follows the report contract: **documented**
(vendor manual states it), **absent from documentation**, **unavailable for
local verification** (no local install; nothing observed).

## Coverage matrix

| SWI facility | Allegro Prolog | LispWorks Common Prolog / KnowledgeWorks |
| --- | --- | --- |
| S-expression term syntax | documented; native representation (Lisp lists, `?`vars) | documented; lists and simple vectors |
| Edinburgh syntax reader | absent from documentation (explicitly no operator syntax; Edinburgh "may be provided in the future") | documented; `consult`/`reconsult`/`compile_and_reconsult` on `.pl`, `(erqp)` loop, compat predicate list |
| First-order unification, `=`, `==`, `=..`, `functor/3`, `arg/3` | documented for `=`, `==`; `copy-term`, `functor`, `arg` in the built-in list | documented for `=`, `==`, `=..`, `functor`, `arg` over vectors |
| Occurs check / cyclic terms | absent from documentation | absent from documentation |
| Depth-first backtracking, multiple answers | documented (`?-` loop, `first/1`, `or/*`, `repeat`, `if/2`, `if/3`, `memberp`) | documented (`rqp`, `;` retry, `once/1`) |
| Cut | documented (`!`, `(!)`; used in the zebra benchmark) | documented (`(cut)`; `!` via Edinburgh layer) |
| Arithmetic (`is/2` and friends) | documented as `is`/`lisp` delegation to Lisp arithmetic (more general: unifies any returned value) | documented `is/2` plus goal-position Lisp evaluation `((floor 3 4) ?x ?y)` |
| All-solutions: `findall`, `bagof`, `setof` | documented `bagof`, `setof`; no `findall` in the built-in list | documented `findall`, `findallset`, `bagof`, `setof` (explicit-existential syntax); documented hang on infinite solution sets |
| Dynamic facts: assert/asserta/assertz/abolish/retract | documented; two databases (compiled predicates; recorded hashtable) with an explicit semantic split between them | documented `asserta/assertz/retract/erase`; `recorda/recordz/recorded` |
| Fact indexing control | absent from documentation (named only as a known-issue wishlist, "index/1") | documented `(mode ...)` declarations with `?`, `?*`, `+`, `-` specs |
| Tabling, SLG/well-founded evaluation | absent from documentation | absent from documentation |
| Incremental table maintenance after updates | absent from documentation (assert invalidates the compiled function, full recompile next call) | absent from documentation (same shape: redefinition recompiles) |
| Datalog fixpoint / function-free safety analysis | absent from documentation | absent from documentation |
| Negation (`not/1`, `\+`) | documented `not/1` (no well-founded/delays statement) | documented `not/1` and Edinburgh `\+` |
| DCGs | absent from documentation | documented `defgrammar`, `phrase`, pushback lists |
| CHR, finite-domain constraints, attributed variables, coroutining/`freeze` | absent from documentation | absent from documentation |
| Exceptions (`catch/throw`), warnings | absent from documentation (Lisp conditions are the documented error channel; `prolog-stack-overflow` condition documented) | absent from documentation (Lisp conditions implied; not stated for Prolog goals) |
| Modules / predicate namespaces | documented absence ("no implementation of the Prolog module system") with an inherited-symbol collision warning | absent from documentation (Lisp packages apply; no Prolog module system named) |
| 4-port debugging, spy, trace | documented `leash`/`unleash` 4-port printing (no spy points) | documented `(trace)`, `(spy ...)`, `(leash ...)`, interactive creep/skip/leap/break/retry/fail loop; KW IDE rule stepping |
| Lisp interop (call Lisp from Prolog) | documented and extensive: `lisp`/`lisp*`/`lisp!`/`lispp`/`lispp*`/`is`, `let`/`let*`, `unwind-protect`, CLOS `slot=`/`slot-value`, `generator` closures; documented lexical-scope restriction and stack-consing dynamic-extent hazard | documented: goal-position Lisp forms unifying all values; `?.var` references in `with-prolog` |
| Call Prolog from Lisp | documented: `prolog` macro, `?->`, block-return protocol | documented: `logic` with `:return-type`/`:all`, `any`, `findall`, `findallset`, `deflogfun`, `with-prolog` |
| Graph SCC, worklists, compiler-graph utilities | absent from documentation (host Lisp provides it) | absent from documentation (host Lisp provides it) |
| Foreign C API of the engine | absent from documentation | absent from documentation |
| Standalone executable / library delivery | documented at ACL product level ("Application delivery as a DLL or stand alone image"); Prolog-module-specific delivery statement absent from documentation | documented: `deliver` generates executables and libraries; edition-gated (Professional/Enterprise/HobbyistDV); KnowledgeWorks page claims multi-platform delivery |
| Embedded runtime hosting | documented: this is the design point of Allegro Prolog (Lisp-integrated extension, no separate listener) | documented: `(require "prolog")` inside the Lisp image; save an image with it preloaded |
| Concurrent isolated query engines | absent from documentation (Prolog stack/choice points are per-computation; thread semantics unstated) | absent from documentation (KW mentions "multiple independent inferencing states" for forward chaining; Prolog-goal concurrency unstated) |
| License / procurement gate for the DL7 path | Allegro CL 11.0 Free Express Edition exists; Prolog's presence in Express and the delivery terms are unavailable for local verification; commercial licensing by quote | KnowledgeWorks/Common Prolog is Enterprise Edition only, $4,500/user (64-bit, 2025-03-03 USD); 1-month evaluation on request |

## Reading for the DL7 compiler question

- Both engines cover the SWI core that DL7 rules need at execution time:
  Horn rules, unification, backtracking, cut, dynamic facts, all-solutions
  predicates, compiled predicates, Lisp data interchange.
- Both lack, at the documentation level, exactly the facilities labs 2-12
  found missing in open-source CL Prologs: tabling, fixpoint semantics,
  fairness, constraints, module systems, and incremental update maintenance.
  The commercial engines add Edinburgh compatibility, debugging, and delivery
  maturity, and in LispWorks' case mode/indexing declarations and DCGs.
- Termination on the shared cyclic `path` fixture would therefore require the
  same depth-bound, visited-state, or bottom-up adapter as the open-source
  engines; nothing in either manual documents an engine-side mechanism.
- Procurement is the real boundary: Allegro by quote with a Free Express
  tier of undocumented Prolog scope; LispWorks Enterprise at a published
  per-user price with a documented one-month evaluation route.
