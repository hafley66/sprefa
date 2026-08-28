# Lab 15 Capabilities: Allegro Prolog vs LispWorks Common Prolog

Legend: **documented** (vendor manual states it), **absent from documentation**
(the manual does not provide it, and where stated explicitly the product says
so), **unavailable for local verification** (no local install; no receipt is
possible). Nothing in this file is locally observed.

## Allegro Prolog (Franz Inc., Allegro CL 10.1-era doc [B])

### Syntax

- Documented: S-expression syntax, not Edinburgh. Prolog variables are Lisp
  symbols whose names begin with `?`; anonymous variable is the symbol `?`.
  Example session (from the manual, verbatim):
  ```lisp
  (require :prolog) (use-package :prolog)
  (?- (append ?x ?y (1 2 3)))
  ;; ?x = () ?y = (1 2 3) <ENTER> ... No.
  ```
  Rules via macros `<-` (assert clause) and `<--` (retract all clauses of the
  functor/arity first, then assert), e.g.
  ```lisp
  (<-- (member ?item (?item . ?)))
  (<-  (member ?item (? . ?rest)) (member ?item ?rest))
  ```
- Documented absence: "There is also no support for operator syntax (e.g. infix
  notation)"; Edinburgh input "may be provided in the future" but is not present
  in this manual. No ISO-compliance claim: "does not intend to be an
  ISO-compliant Prolog".

### Unification

- Documented: standard first-order unification over Lisp conses, `=`/2 and
  `==/2` built-ins; nested terms unify through cons recursion (the `is`/`lisp`
  docs show destructuring unify of `(multiple-value-list (truncate 11 3))`
  against `(?div ?rem)`). Structured terms are ordinary Lisp lists.
- Occurs check: absent from documentation. The manual never mentions occurs
  check, cyclic terms, or rational-tree behavior. Unavailable for local
  verification.

### Search and backtracking

- Documented: depth-first chronological backtracking with choice points
  ("control may return to any previous choice point, undoing any intervening
  unifications"); interactive `?-` loop backtracks on newline/semicolon;
  `first/1` (commit to first solution), `or/*` (variadic disjunction),
  `and/*`, `repeat/0`, `fail/0`, `if/2`, `if/3`, `memberp/2` (member without
  backtracking). Cut is `!` (atom or 1-element list `(!)`); the zebra example
  uses `!` inside the `prolog` macro to stop after one answer.
- Fairness, tabling, fixed-point search: absent from documentation. No
  tabling, memoization, or answer-subsumption mechanism is named anywhere in
  the chapter; recursive cyclic programs are not discussed.

### Compilation model

- Documented: "Prolog predicates are translated to compiled Common Lisp
  functions. A single Lisp function combines all rules for each distinct
  functor/arity." Predicates compile automatically on first use; assert/retract
  invalidate the compiled function for recompilation on next call;
  `prolog-compile-symbols` forces compilation. Special treatment captures
  variable-free, body-less facts as data inside the function. Files may be
  loaded compiled or interpreted. Tail-call elimination: "functors usually do
  not perform tail-call elimination"; the experimental `declare :tail-calls`
  facility was removed from the release (leftover text explicitly crossed out
  in the manual).

### Debugging

- Documented: `leash {functor arity}*` macro prints the four WAM ports
  (call, exit, redo, fail) to `*trace-output*`; `leash-1`, `unleash`,
  `unleash-1`, `*leash-limit*` (enter debugger above a depth), indentation
  wrap variable; leashing toggles automatic recompilation with/without TCO.
  Known issues section admits leash "deserve[s] extension better to support
  debugging". No spy-point or source-stepper integration for Prolog.

### Lisp interop

- Documented and deep: `lisp`/`lisp*`/`lisp!` variants run Lisp forms from
  Prolog with unification of results; `lispp`/`lispp*` run as predicates
  (fail on nil); `is` is an alias for 2-argument `lisp`; the Lisp function
  `fail` forces clause failure; the `prolog` macro calls Prolog from Lisp with
  lexical/dynamic access via a `block prolog`; `let`/`let*`/
  `unwind-protect` predicates; CLOS slot access predicates `slot=`, `slot=*`,
  `slot-value`, `slot-value!` (backtrackable vs persistent slot binding);
  `generator`/`generator*`/`generating`/`generating*` predicates iterate Lisp
  closures as Prolog data sources. Constraint: "a Lisp form inside a prolog
  rule executed by the lisp/2 and similar predicates may not refer to the Lisp
  lexical environment outside the rule definition" (late compilation of
  functor/arity bodies).
- Documented hazard: dynamic extent. Prolog variables, unification conses, and
  continuation closures are stack allocated ("essentially zero consing");
  data returned into Lisp must be heap-copied (the manual's `lisp!` family or
  explicit `copy-tree`); `bagof`/`setof` results are automatically heap
  consed.

### Dynamic facts, retraction, databases

- Documented: compiled-predicate database via `assert/1` (list of clauses!),
  `asserta/1`, `assertz/1`, `abolish/2`; separate recorded-fact hashtable
  database (`equal`-key hashtable) via `recorda/1`, `recordz/1`, `recorded/2`,
  `retract/1`, `retractall`, `erase/1`; the hashtable database is consulted
  only through `recorded/2`, and is not automatically visible to compiled
  predicates. `consult` clears a functor/arity on first redefinition;
  `<--` redefines interactively. High-bandwidth assert/retract interleaved
  with calls is called out as a performance hazard; the manual recommends the
  recorded interface or generators. AllegroCache integration via `pcache`'s
  `db` predicate reasons over persistent CLOS objects with first-slot index
  optimization.

### Constraints, tabling, DCG/grammar

- Constraints: absent from documentation. No finite domains, disequality, or
  constraint store is mentioned anywhere in the chapter.
- Tabling: absent from documentation (no call/answer tables, no subsumption).
- DCG: absent from documentation. No grammar-rule mechanism is described.

### Executable delivery and embedding

- Documented (product level [A]): "Application delivery as a DLL or stand alone
  image" is an Allegro CL runtime feature. The Prolog chapter itself says
  nothing about delivery; whether the `prolog` module survives image delivery
  is unavailable for local verification.
- Embedding: documented, this is the product's design point ("Prolog logic
  programming as an integrated extension to Common Lisp for use in Lisp
  programs, not as a separate language"). No Prolog top-level listener besides
  the Lisp listener plus the `?-` macro.

### Prolog stack

- Documented: a separate Prolog stack of unifications doubles on overflow;
  `prolog-stack-overflow` condition (a `cl:condition`, not `error`) with
  `*prolog-stack-limit*` default 4096.

## LispWorks Common Prolog / KnowledgeWorks (LispWorks 8.1, doc 18 Feb 2025 [E])

### Syntax

- Documented: Lisp-like syntax with `?` variables (`??foo` escapes to the
  symbol `?foo`); goals are lists or simple vectors, e.g.
  `(reverse (1 2 3) ?x)` or `#(member ?x (1 2 3))`. Structured terms are
  simple vectors; `functor/3`, `arg/3`, `=../2` behave "in a standard fashion"
  on both lists and vectors. Definitions:
  ```lisp
  (defrel append
    ((append () ?x ?x))
    ((append (?u . ?x) ?y (?u . ?z)) (append ?x ?y ?z)))
  ```
- Documented: an Edinburgh syntax translator exists. `consult` on `.pl` files;
  `reconsult` also loads `.lisp` and compiled `.?fasl` files;
  `compile_and_reconsult`; a separate Edinburgh read-query-print loop `(erqp)`
  alongside the Lisp-syntax `(rqp)`. The compatibility predicate list
  (`-->`, `->`, `:-`, `\+`, `^`, `is`, `name`, `see/tell` family, etc.) is
  enumerated in Appendix A.14.

### Unification

- Documented: standard Prolog unification (`=`, `==`, `\==`, `=..`), vector
  structured terms. Occurs check: absent from documentation (never mentioned;
  cyclic-term behavior unavailable for local verification).

### Search and backtracking

- Documented: Prolog-style depth-first search with backtracking through
  compiled continuation-passing functions; multiple solutions via the `rqp`
  loop (`;` for more solutions, `NO.` at exhaustion); `once/1` makes a goal
  deterministic and enables last-call optimization; `findall/3`,
  `findallset/3` (deduplicating), `bagof/2`-style `bagof ?exp ((goal . ex-vars) ?bag)`
  with explicit existentially quantified variable syntax
  `(setof ?x ((foo ?x ?y) ?y) ?z)`; `repeat/0`, `fail/0`, `not/1`, `sort`,
  `keysort`. `findall` and `findallset` "will hang if a goal expression
  generates an infinite solution set" (documented).
- Cut: documented as the `(cut)` goal (KnowledgeWorks `defrule` examples show
  `(cut)` in backward rules); Edinburgh `!` available through the translator.
- KnowledgeWorks backward chaining is the same engine surfaced as
  `(defrule name :backward ...)` over CLOS objects (pattern matching on
  object base, slot terms). Forward chaining is a separate RETE-based
  OPS5-style engine (`defrule name :forward`), with contexts, conflict
  resolution tactics, metarule protocol, truth maintenance ("logical
  dependencies"), and multiple independent inferencing states [E][F].
- Tabling/fairness: absent from documentation. No tabling or fair-merge
  mechanism is named.

### Compilation

- Documented: "Common Prolog predicates are compiled into Lisp functions which
  may then be compiled by a standard Lisp compiler"; WAM-derived design
  "modified to take advantage of a Lisp environment's built in support for
  control flow and memory allocation"; "each Prolog clause compiles into a
  function and handling Prolog control flow by continuation passing" [E][F].
  `defdetrel`/`deterministic` declare deterministic relations (allowing last
  call optimization); `defrel` accepts `(mode ...)` declarations controlling
  clause indexing (`?`, `?*`, `+`, `-` per argument; default `?*` first arg,
  `?` rest); `defrelmacro` defines logic macros expanded before variable
  translation; `output-defrels` recovers `defrel` forms from dynamic clauses.

### Debugging

- Documented: exhaustive `(trace)`, spy points `(spy foo)`, `(spy (foo 3))`,
  `(spy (foo bar))`, `nospy`, `leash` with port subsets, 4-port model, and an
  interactive command loop with creep/skip/leap/break/display/quit/retry/
  fail/abort; `debug`/`debugging`/`nodebug`/`notrace`; `listing`.
  KnowledgeWorks adds spy windows, rule monitors, forward-chaining history,
  single-step buttons in the IDE [E].

### Lisp interop

- Documented: a Lisp form in goal position (`((floor 3 4) ?x ?y)`) evaluates
  and unifies all returned values, multiple-value aware; variables must be
  bound at evaluation. From Lisp: `logic` with `:return-type`
  `:display/:fill/:bag/:alist` and `:all nil/:values/:list`; `any`,
  `findall`, `findallset`; `deflogfun` generates Lisp functions over
  precompiled goals with `:all` keyword; `with-prolog` embeds Prolog in Lisp
  functions with `?.name` Lisp-variable references. No dynamic-extent hazard is
  documented for Common Prolog (unlike Allegro Prolog's stack-consing model);
  the manual does not discuss data extent at all.

### Dynamic facts and retraction

- Documented: `asserta`, `assertz`, `retract`, `erase` (database reference),
  `recorda`, `recordz`, `recorded`, `clause`, plus KnowledgeWorks object-base
  mutation goals `assert`/`retract` over CLOS instances (`(assert (truck
  ?truck driver ?driver))`), with RETE propagation on object change. Dynamic
  schema evolution for the object base is a KnowledgeWorks/CLOS property [E][F].

### Constraints, tabling, DCG

- Constraints: absent from documentation. No CLP domains, disequality, or
  constraint store in the KnowledgeWorks/Prolog manual. (Truth maintenance in
  forward chaining is dependency recording, not a constraint store.)
- Tabling: absent from documentation.
- DCG: documented. `defgrammar` defines definite clause grammars; grammar
  bodies support atoms, variables, sub-grammar calls with extra arguments,
  embedded Lisp clauses, `(call term)`, `(cut)`, and pushback lists
  (right-hand context); invoked with `phrase` (2- and 3-argument forms)
  [E, kw-prolog-9].

### Embedding and executable delivery

- Documented: Common Prolog is a library inside the Lisp image
  (`(require "prolog")`), designed for mixed Lisp/Prolog source files; images
  may be saved with it preloaded ("it may be worthwhile to save an image with
  it pre-loaded" [E]).
- Documented [E]: `deliver` generates runtime executables and libraries;
  included in Professional, Enterprise, HobbyistDV; absent from Personal and
  Hobbyist; no runtime license fees for Professional/Enterprise-developed
  applications. Delivery of a KnowledgeWorks/Prolog application specifically is
  described as "Multi-platform delivery" on the KnowledgeWorks product page
  [F].

## Side-by-side summary

| Capability | Allegro Prolog | LispWorks Common Prolog |
| --- | --- | --- |
| Syntax | S-expressions, `?`vars; no infix, no Edinburgh | S-expressions + vectors; Edinburgh translator and `erqp` |
| Unification | documented, cons terms | documented, cons and vector terms |
| Occurs check | absent from documentation | absent from documentation |
| Cut | documented `!` / `(!)` | documented `(cut)` and `!` via translator |
| DCG | absent from documentation | documented `defgrammar` + `phrase` |
| Compilation | predicates to Lisp functions, auto-recompile on assert | clauses to functions, continuation passing, mode/indexing declarations |
| Tabling | absent from documentation | absent from documentation |
| Constraints | absent from documentation | absent from documentation |
| Dynamic facts | assert/asserta/assertz/abolish + recorded hashtable db + AllegroCache `db` | asserta/assertz/retract/erase/recorded + CLOS object base with RETE propagation |
| Debugging | `leash` 4-port, recompile-based | 4-port trace/spy/leash with interactive loop; KW IDE windows |
| Lisp interop | `lisp`/`lispp`/`is` family with stack-consing extent hazard; `prolog` macro | goal-position Lisp forms; `logic`, `any`, `findall(set)`, `deflogfun`, `with-prolog` |
| Standalone delivery | documented at product level (DLL or stand-alone image) | documented (`deliver`, edition-gated) |
| Where it lives | `require :prolog` in ACL (Enterprise/Express tiers; pricing by quote) | `(require "prolog")`, Enterprise Edition only; $4,500/user 64-bit (2025-03-03 USD) |
| Local verification | unavailable | unavailable |

## Evidence gaps

1. ACL 11.0-era Allegro Prolog chapter: franz.com was unreachable on
   2026-08-28 (HTTP 522); the newest archived snapshot of the Prolog chapter
   documents ACL 10.1 / Prolog 1.1.2. Any ACL 11 changes to Prolog are
   undocumented in the obtainable material.
2. Occurs-check and cyclic-term policy for both engines: no manual statement;
   needs a local trial build to observe.
3. Whether Allegro Prolog survives Allegro CL image delivery: stated only as a
   generic product capability; no Prolog-specific delivery statement found.
4. KnowledgeWorks SQL-interface details and Common Prolog under delivery were
   read from the product page and manual TOC; per-platform delivery caveats
   live in the LispWorks Delivery manual, which was not mirrored in full.
