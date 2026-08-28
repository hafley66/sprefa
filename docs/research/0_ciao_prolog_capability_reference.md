# Ciao Prolog, reconstructed from the compiler outward

Research date: 2026-08-27

Pinned revisions:

- Ciao: [`v1.25.0-m1`](https://github.com/ciao-lang/ciao/releases/tag/v1.25.0-m1), commit [`fdff410`](https://github.com/ciao-lang/ciao/commit/fdff410cf2b7f2b85baff97485a2db5522d785f3), published 2025-06-21
- CiaoPP: version `1.8.0`, commit [`241dd12`](https://github.com/ciao-lang/ciaopp/commit/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928)
- Main Ciao manual: version 1.25, dated 2025-06-10
- CiaoPP reference manual: version 1.8, dated 2024-10-13

Local execution status: `ciao` and `ciaoc` were absent from the workstation. Syntax and phase claims below were checked against the current manuals, tagged source, compiler implementation, package implementations, release metadata, issue tracker, and maintainer discussions. No Ciao installation was performed.

## Reading map

1. [The one-screen model](#the-one-screen-model)
2. [The first program](#the-first-program)
3. [Packages make each module select a language](#packages-make-each-module-select-a-language)
4. [The exact compiler phase order](#the-exact-compiler-phase-order)
5. [A concrete package lowering](#a-concrete-package-lowering)
6. [Functional syntax and the extra result argument](#functional-syntax-and-the-extra-result-argument)
7. [Assertions, properties, modes, and regular types](#assertions-properties-modes-and-regular-types)
8. [CiaoPP and abstract interpretation](#ciaopp-and-abstract-interpretation)
9. [Compiler data structures](#compiler-data-structures)
10. [Compilation products and deployment](#compilation-products-and-deployment)
11. [Capability inventory](#capability-inventory)
12. [Extension surfaces](#extension-surfaces)
13. [Observed limits and maintenance friction](#observed-limits-and-maintenance-friction)
14. [Prior art and neighboring systems](#prior-art-and-neighboring-systems)
15. [Correspondence with the DL6 work](#correspondence-with-the-dl6-work)
16. [Documentation inventory](#documentation-inventory)
17. [Release timeline](#release-timeline)
18. [Advanced recipes](#advanced-recipes)
19. [LLM and automation notes](#llm-and-automation-notes)
20. [Verification record](#verification-record)
21. [Primary sources](#primary-sources)

## The one-screen model

```text
                         authored module
                               │
              module(..., ..., [package list])
                               │
             packages install syntax and translators
                               │
                               ▼
       read terms ──► translate terms ──► record program facts
                                               │
                     ┌─────────────────────────┴──────────────────────┐
                     │                                                │
                     ▼                                                ▼
              normal compiler                                  CiaoPP
                     │                                                │
           module expansion                                  abstract fixpoint
           clause/goal lowering                                      │
           WAM and bytecode                                  inferred properties
                     │                                        checked assertions
                     ▼                                        transformed source
                  engine
```

Ciao has one small Prolog-family kernel and several layers around it:

| Layer | Stored object | Main operation |
|---|---|---|
| Reader | Prolog terms | Parse with module-local operators |
| Package layer | Ordered translation hooks | Rewrite source terms and goals |
| Compiler frontend | Clauses, declarations, imports, assertions | Resolve modules and compile clauses |
| Assertion layer | Logical properties attached to calls and computations | Document, check, and infer behavior |
| CiaoPP | A preprocessing-unit database plus abstract-domain facts | Compute abstract fixpoints and transform programs |
| Runtime | WAM-style bytecode plus the Ciao engine | Execute predicates, constraints, tabling, and effects |

The language of a file is selected by its module declaration. Two files in one application may activate different package lists and therefore accept different syntax or semantics.

## The first program

```prolog
:- module(colors, [opposite/2], [assertions]).

:- pred opposite(Color, Opposite)
   : color(Color)
   => color(Opposite)
   + is_det.

:- regtype color/1.
color(red).
color(green).

opposite(red, green).
opposite(green, red).
```

Read each line mechanically:

```text
module(colors, [opposite/2], [assertions])
       │              │                │
       │              │                └─ language packages for this module
       │              └─ exported predicate name and arity
       └─ module name
```

The assertion says:

```text
opposite(Color, Opposite)
  call condition:    color(Color)
  success condition: color(Opposite)
  computation fact:  is_det
```

The clauses remain ordinary relations. Either argument can participate in unification. The assertion supplies an intended calling pattern and a property of that use.

## Packages make each module select a language

The third argument of `module/3` is a package list:

```prolog
:- module(example, [main/0], [assertions, regtypes, fsyntax, clpfd]).
```

A module imported with `use_module/1` contributes predicates. A package contributes language behavior. The module manual states that modules do not alter another module's syntax; packages perform that job.

```text
module import                         package activation
─────────────                         ──────────────────
exports predicates                    defines operators
provides runtime code                 defines declarations
keeps caller syntax unchanged         installs source translators
                                      may import runtime support
```

`module/2` implicitly selects the `classic` package. `module/3` makes the package set explicit. `pure` and `noprelude` suppress the usual prelude.

### Package source shape

The current DCG package is a complete small example:

```prolog
:- package(dcg).

:- load_compilation_module(library(dcg/dcg_tr)).
:- add_sentence_trans(dcg_tr:dcg_translation/3, 310).
:- add_goal_trans(dcg_tr:dcg_translation_goal/3, 310).

:- include(library(dcg/dcg_ops)).
```

This means:

```text
package(dcg)
    │
    ├─ load translator code into the compiler
    ├─ run dcg_translation over source sentences at priority 310
    ├─ run dcg_translation_goal over goals at priority 310
    └─ include the operators required to parse DCG notation
```

Current source: [`core/lib/dcg/dcg.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/dcg/dcg.pl)

### Translation predicate signatures

The public package API describes translators with arity 2 or 3:

```prolog
translate(Old, New).
translate(Old, New, Module).
```

The compiler binds `Old` to the source item. A successful translator binds `New` to the replacement. Failure preserves the original item. A sentence translator may return a list, allowing one source sentence to produce several sentences.

Priorities are integers. Lower priorities execute first.

```text
Old term
   │
   ▼
translator at priority 100
   │
   ▼
translator at priority 200
   │
   ▼
translator at priority 300
   │
   ▼
final term
```

## The exact compiler phase order

The public package manual gives the relative order. The current frontend source provides the full bootstrap sequence.

### Phase 0: read the first sentence

The compiler reads the module declaration first because it needs the package list before it can correctly parse and translate the remainder of the file.

```text
open source
    │
    ▼
read first sentence
    │
    ▼
normalize module declaration
    │
    ├─ determine module name
    ├─ determine exports
    └─ determine package list
```

### Phase 1: activate the package environment

```text
activate prelude unless pure/noprelude
    │
    ▼
include each package file
    │
    ├─ operators affect later parsing
    ├─ new declarations become recognized
    ├─ compilation modules are loaded
    └─ sentence and term hooks become active immediately
```

Package activation is source ordered. A later `use_package/1` directive can affect the remaining text.

### Phase 2: read and normalize source sentences

For each source sentence:

```text
raw parsed sentence
    │
    ▼
conditional-compilation filtering
    │
    ▼
sentence translations, ordered by priority
    │
    ▼
recursive term translations, ordered by priority
    │
    ▼
classify result as declaration, clause, fact, or compile-time query
    │
    ▼
record frontend facts
```

The source confirms this call order:

```prolog
expand_term(X0, M, Dict, X2) :-
    sentence_translation(X0, M, Dict, X1),
    term_translation(X1, M, Dict, X2).
```

Current source: [`core/lib/compiler/translation.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/compiler/translation.pl)

### Phase 3: generate module and interface information

The reader records declarations, imports, exports, predicate definitions, assertions, source locations, and variable dictionaries. Dependencies are processed incrementally. Interface data goes to `.itf` cache files.

### Phase 4: compile clauses

Clause compilation has a second translation sequence:

```text
recorded clause
    │
    ▼
clause translations with priority <= 100000
    │
    ▼
module expansion and meta-predicate expansion
    │
    ├─ resolve local/imported/multifile predicate identity
    ├─ qualify callable goals
    └─ expand higher-order arguments
    │
    ▼
recursive goal translations
    │
    ▼
clause translations with priority > 100000
    │
    ▼
WAM/bytecode compilation or interpreted clause installation
```

The compiler calls the two clause-translation phases `before_mexp` and `after_mexp`. The boundary at priority `100000` is implemented in `add_clause_trans/3`.

Current frontend sources:

- [`core/lib/compiler/c_itf.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/compiler/c_itf.pl)
- [`core/lib/compiler/frontend_core.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/compiler/frontend_core.pl)
- [`core/lib/compiler/mexpand.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/compiler/mexpand.pl)

### Translator lifecycle

Sentence translators receive `0` once for module initialization. Clause translators receive `clause(0,0)` once. Goal translators receive `end_of_file` during shutdown.

```text
register hook
    │
    ▼
initialization sentinel
    │
    ▼
zero or more source items
    │
    ▼
end-of-module sentinel where applicable
    │
    ▼
remove module-local hook state
```

## A concrete package lowering

### Regular type declaration

Authored source:

```prolog
:- regtype tree(T).
```

The `regtypes` package defines `regtype` as a new declaration and installs one sentence translator:

```prolog
:- package(regtypes).

:- load_compilation_module(library(regtypes/regtypes_tr)).
:- add_sentence_trans(regtypes_tr:expand_regtypes/2, 210).

:- new_declaration(regtype/1).
:- new_declaration(regtype/2).
```

Its complete essential lowering is:

```prolog
expand_regtypes((:- regtype(T)), (:- prop(T + regtype))).
```

So the compiler sees:

```prolog
:- prop tree(T) + regtype.
```

Current sources:

- [`core/lib/regtypes/regtypes.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/regtypes/regtypes.pl)
- [`core/lib/regtypes/regtypes_tr.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/regtypes/regtypes_tr.pl)

### DCG rule

Authored source:

```prolog
sequence --> item, sequence.
sequence --> [].
```

Conceptual lowering:

```prolog
sequence(S0, S) :-
    item(S0, S1),
    sequence(S1, S).

sequence(S, S).
```

The package adds two state arguments and threads the intermediate state. The current translator constructs a new head with arity `A + 2`, places `S0` and `S` in the added positions, and recursively connects body items.

Current source: [`core/lib/dcg/dcg_tr.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/lib/dcg/dcg_tr.pl)

This is the concrete connection between a high-level syntax and ordinary relational clauses:

```text
special surface term
      │
      ▼
ordinary Prolog term with extra arguments
      │
      ▼
normal module expansion and compilation
```

## Functional syntax and the extra result argument

The `fsyntax` package provides function-shaped notation over relations.

```prolog
opposite(red) := green.
```

lowers to:

```prolog
opposite(red, green).
```

Another example:

```prolog
addlast(X, L) := ~append(L, [X]).
```

lowers to:

```prolog
addlast(X, L, Result) :-
    append(L, [X], Result).
```

### The designated result position

`fsyntax` treats one predicate argument as the result connection. The default is the last argument. `fun_return/1` can select another position.

```text
function-shaped view                  relation-shaped storage
────────────────────                  ───────────────────────
append(L, [X]) returns R              append(L, [X], R)
fact(N) returns R                     fact(N, R)
arg(1, T) returns A                   arg(1, T, A)
```

The extra argument remains a logic variable. Calling direction comes from instantiation and constraints. The manual demonstrates reverse use of functional notation under CLP(Q): `24 = ~fact(X)` can solve for `X`.

### Nested functional expressions

```prolog
der(A + B) := der(A) + der(B).
```

lowers conceptually to:

```prolog
der(A + B, X + Y) :-
    der(A, X),
    der(B, Y).
```

The translator creates intermediate variables and emits relational calls. Ciao preserves tail recursion for recursive functional definitions according to the package manual.

### Evaluation marks

| Syntax | Meaning |
|---|---|
| `~f(X)` | Evaluate `f/2` using its designated result argument |
| `f(X) := R` | Define a function-shaped clause that lowers to a predicate with one extra argument |
| `^Term` | Quote the principal functor so functional evaluation does not consume it |
| `^^Goal` | Move functional evaluation into a meta-goal's inner scope |
| `:- fun_eval f/1.` | Treat `f/1` applications as evaluable without `~` |
| `:- fun_return f(_,~,_)` | Select the argument used as the result connection |

Current manual: [Functional notation](https://ciao-lang.org/ciao/build/doc/ciao.html/fsyntax_doc.html)

Current package source: [`core/library/fsyntax/fsyntax.pl`](https://github.com/ciao-lang/ciao/blob/fdff410cf2b7f2b85baff97485a2db5522d785f3/core/library/fsyntax/fsyntax.pl)

The optimized compiler recognizes `functional_expand` as a compiler pragma. The package remains the module-level activation point while the optimized implementation may perform the lowering in compiler code.

## Assertions, properties, modes, and regular types

### Assertion grammar

The common shape is:

```prolog
:- Status pred Head : Calls => Success + Computation # Comment.
```

Every field is optional except the head.

```text
Status       what confidence/source the assertion has
Head         predicate and variables being described
Calls        properties required at call time
Success      properties guaranteed on successful answers
Computation  whole-run properties such as determinism, failure, or cost
Comment      documentation data consumed by LPdoc
```

Example:

```prolog
:- check pred append(Xs, Ys, Zs)
   : (list(Xs), list(Ys), var(Zs))
   => (list(Xs), list(Ys), list(Zs))
   + (is_det, not_fails).
```

### Status lattice used by tools

| Status | Mechanical meaning |
|---|---|
| `check` | A claim awaiting proof, refutation, or runtime enforcement |
| `checked` | A tool has proved the claim for the analyzed scope |
| `false` | A tool has shown the claim can be violated |
| `trust` | The analyzer accepts the claim as supplied information |
| `true` | Information inferred or known to hold |

An unqualified `pred` assertion defaults to `check`.

The public `pred` declaration is normalized into a `calls` assertion and a `success` assertion. Computation properties are represented through `comp` assertions.

### Assertion kinds

| Kind | Describes |
|---|---|
| `pred` | Combined call, success, computation, and documentation contract |
| `calls` | Admissible calls |
| `success` | Answers under an optional call condition |
| `comp` | Properties of the whole computation |
| `entry` | Calls arriving from outside the analyzed module |
| `exit` | Answers exposed outside the module |
| `prop` | A predicate safe for use as a property/check |
| `regtype` | A property with regular-type structure |
| `test` | A concrete assertion-backed test case |
| `texec` | Test input and execution instructions |
| `decl` | A declaration contract |
| `modedef` | A reusable call/success property macro |

### Properties are executable predicates

Ciao defines a property as a predicate satisfying operational restrictions required for safe checking. The documented restrictions include termination, absence of interfering side effects, and no further instantiation or added constraints.

```prolog
:- prop even(X).

even(X) :-
    integer(X),
    0 is X mod 2.
```

The same `even/1` predicate may appear in a goal or inside an assertion.

### Regular types are properties

```prolog
:- regtype color/1.

color(red).
color(green).
color(blue).
```

Recursive example:

```prolog
:- regtype tree/1.

tree(empty).
tree(node(Value, Left, Right)) :-
    term(Value),
    tree(Left),
    tree(Right).
```

The type is represented by clauses for a unary predicate. Parametric regular types use higher-order property arguments. The functional-syntax manual gives this example:

```prolog
:- regtype list_of/2.
list_of(T) := [] | [~T | list_of(T)].
```

which lowers to:

```prolog
list_of(_, []).
list_of(T, [X|Xs]) :-
    T(X),
    list_of(T, Xs).
```

### Modes are assertion sugar

```prolog
:- pred is(-num, +arithexpression).
```

expands into call and success properties. The mode markers do not create a separate type checker. They abbreviate assertion fields.

### Type information has several origins

```text
authored regtype clauses ──────────────┐
authored assertions ──────────────────┤
library assertions ───────────────────┼─► assertion/property database
CiaoPP inferred abstract values ──────┤
runtime assertion failures ───────────┘
```

Official manuals:

- [Assertion language](https://ciao-lang.org/ciao/build/doc/ciao.html/assertions_doc.html)
- [Regular types](https://ciao-lang.org/ciao/build/doc/ciao.html/regtypes_doc.html)
- [Basic properties](https://ciao-lang.org/ciao/build/doc/ciao.html/basic_props.html)
- [Classical modes](https://ciao-lang.org/ciao/build/doc/ciao.html/modes_doc.html)
- [Runtime checks](https://ciao-lang.org/ciao/build/doc/ciao.html/rtchecks_doc.html)

## CiaoPP and abstract interpretation

CiaoPP operates after source loading and package expansion. Its internal unit includes the module source, dependencies, declarations, assertions, imported predicate assertions, and transitively needed property definitions.

```text
expanded module and dependencies
              │
              ▼
       preprocessing unit
              │
              ├─ clauses
              ├─ variable-name dictionaries
              ├─ declarations
              ├─ authored assertions
              ├─ imported assertions
              └─ property definitions required to interpret assertions
              │
              ▼
      selected abstract domains
              │
              ▼
        abstract fixpoint
              │
              ├─ call abstractions
              ├─ success abstractions
              ├─ determinism/failure facts
              ├─ size/cost bounds
              └─ program-point facts
              │
              ▼
       check authored assertions
              │
              ├─ checked
              ├─ false
              └─ residual check
              │
              ▼
   optional transformation and output
```

### High-level lifecycle

The high-level interface exposes three entry points:

```prolog
auto_analyze(File).
auto_check_assert(File).
auto_optimize(File).
```

Their source-level call path is:

```text
module(File, Info)
    │
    ▼
analyze(AbstractDomains)
    │
    ▼
acheck / acheck_summary
    │
    ▼
transform(Transformation)
    │
    ▼
output(File)
```

Current sources:

- [`src/auto_interface.pl`](https://github.com/ciao-lang/ciaopp/blob/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928/src/auto_interface.pl)
- [`src/frontend_driver.pl`](https://github.com/ciao-lang/ciaopp/blob/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928/src/frontend_driver.pl)
- [`src/analyze_driver.pl`](https://github.com/ciao-lang/ciaopp/blob/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928/src/analyze_driver.pl)
- [`src/transform_driver.pl`](https://github.com/ciao-lang/ciaopp/blob/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928/src/transform_driver.pl)

### Command-line lifecycle

```bash
ciaopp -A module.pl
ciaopp -V module.pl
ciaopp -O module.pl
ciaopp -o module_checked.pl -V module.pl
ciaopp -T
```

| Flag | Operation |
|---|---|
| `-A` | Analyze |
| `-V` | Verify assertions |
| `-O` | Optimize |
| `-Q` | Open text configuration menu |
| `-T` | Start CiaoPP top level |
| `-fFlag=Value` | Set a public preprocessing flag |

### Abstract domains

CiaoPP has separate analyses because one finite abstraction rarely tracks every useful property efficiently.

| Family | Examples of inferred information |
|---|---|
| Groundness and sharing | Ground variables, alias groups, freeness, linearity |
| Term shape and types | Bounded-depth term structure, regular types, generated types |
| Determinacy | At most one solution, mutual exclusion, nondeterminism |
| Non-failure | Guaranteed success, definite failure, possible failure |
| Effects | Side-effect classification |
| Sizes | Upper/lower bounds for term size, depth, list length, integer value |
| Cost | Resolution-step bounds and asymptotic orders |
| Numeric constraints | Signs, intervals, polyhedra |
| Partial evaluation | Abstract unfolding, specialized call patterns |

The `fixpoint` option selects implementations such as `plai`, `dd`, and `di`. The analyzer can run several domains and preserve each domain's facts.

### Widening and finite convergence

Recursive programs may generate an infinite ascending sequence of increasingly precise shapes. Abstract domains define finite representations and widening operations so analysis reaches a fixpoint.

```text
concrete executions: potentially unbounded terms and states
                         │ abstraction
                         ▼
abstract state: finite property representation
                         │ transfer through clauses
                         ▼
new abstract state
                         │ join and widening
                         └─────────────── repeat until stable
```

Type analyses include structural widening, shortening, bounded term depth, and domains restricted to user/library type definitions.

Official manuals:

- [CiaoPP reference](https://ciao-lang.org/ciao/build/doc/ciaopp.html/)
- [CiaoPP high-level interface](https://ciao-lang.org/ciao/build/doc/ciaopp.html/auto_interface.html)
- [CiaoPP low-level interface](https://ciao-lang.org/ciao/build/doc/ciaopp.html/ciaopp.html)
- [Available abstract domains](https://ciao-lang.org/ciao/build/doc/ciaopp.html/part_domains.html)
- [CiaoPP tutorials](https://ciao-lang.org/ciao/build/doc/ciaopp_tutorials.html/)

## Compiler data structures

### Frontend records

The compiler frontend stores source in dynamic relations. Representative signatures from `c_itf.pl`:

```prolog
clause_of(Base, Head, Body, VarNames, Source, Line0, Line1).
defines_module(Base, Module).
direct_export(Base, Functor, Arity, DefinitionType, MetaSpec).
imports_pred(Base, ImportedFile, Functor, Arity, DefinitionType, MetaSpec, EndFile).
uses(Base, File).
loads(Base, CompilationModule).
decl(Base, Declaration).
```

Assertions are normalized into:

```prolog
assertion_read(Predicate, Module, Status, Type, Body,
               VarDictionary, Source, LineBegin, LineEnd).
```

This is compiler state represented as queryable relations. The frontend mutates these relations while reading and compiling a module.

### Interface files

`.itf` files cache enough facts for dependency checks, imports, exports, meta-predicate information, declarations, and incremental compilation. Package declarations created with `new_declaration/2` can opt into the interface so dependent modules see them.

### CiaoPP preprocessing units

The preprocessing-unit library exposes relational operations including:

```prolog
preprocessing_unit(Files, Modules, Error, Options).
program(Clauses, Dictionaries).
replace_program(Clauses, Dictionaries).
get_assertion(Key, Assertion).
add_assertion(Assertion).
add_directive(Directive).
```

`.ast` files cache preprocessing-unit information. The unit includes transitive property definitions needed to interpret exported assertions.

Official manual: [Preprocessing Unit Handling Library](https://ciao-lang.org/ciao/build/doc/ciaopp.html/p_unit.html)

### Storage and lifecycle

```text
source text
   │ read
   ▼
temporary frontend facts
   │
   ├─► .itf dependency/interface cache
   ├─► clause compiler
   └─► CiaoPP preprocessing unit
             │
             ├─► .ast cache
             ├─► inferred assertion facts
             └─► rewritten source
```

Uniqueness is primarily compound and structural: module identity plus predicate functor/arity, source base plus declaration/import target, or predicate/module/status/type/body for assertions. These are Prolog relations rather than objects with one universal object identifier.

## Compilation products and deployment

`ciaoc` performs separate and incremental compilation. It follows dependencies and reuses object code for unchanged modules.

| Product or mode | Description |
|---|---|
| `.itf` | Interface and dependency information |
| `.po` | Portable compiled object containing bytecode |
| `.wam` | WAM representation requested with `ciaoc -w` |
| Dynamic executable | User modules embedded; library modules loaded at startup |
| Static executable | User modules and libraries embedded; engine still supplied separately |
| Lazy dynamic executable | Libraries loaded on first call |
| Self-contained executable | Engine plus bytecode in one platform-specific executable |
| Generated C | Compiler can emit C per Prolog file for lower-level native linking; current manual says complete standalone linking is not automated through the usual path |

Common commands:

```bash
ciaoc program.pl
ciaoc -c module.pl
ciaoc -w module.pl
ciaoc -s program.pl
ciaoc -S program.pl
```

The default Unix executable is a launcher containing bytecode and invoking the Ciao engine. `-S` includes an engine and produces a platform-specific self-contained executable.

Official manual: [The standalone command-line compiler](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaoc.html)

## Capability inventory

### Language and execution

| Capability | Activation or surface | Implementation position | Documented status |
|---|---|---|---|
| ISO/classic Prolog | `classic`, `iso_strict` packages | Core compiler and libraries | Current, with open ISO-compatibility issues |
| Per-module language selection | `module/3` package list | Compiler frontend | Core |
| User syntax and semantic extensions | package operators and translation hooks | Compiler frontend | Core |
| Higher-order calls | `hiord` | Package, module expansion, runtime | Current |
| Predicate abstractions | `hiord`, `{...}` forms | Static meta-expansion plus runtime support | Current |
| Functional syntax | `fsyntax`, `functional` | Package/compiler expansion | Current |
| Lazy evaluation | `lazy` | Function lowering plus `freeze` transformation | Current package |
| DCGs | `dcg` | Sentence and goal translations | Current |
| Traits/interfaces | `traits` | Sentence and goal translations | Experimental in changelog |
| Named arguments/feature terms | package | Translation and runtime support | Current manual |
| CLP(FD), CLP(Q), CLP(R) | constraint packages | Attributed variables plus solvers | Current |
| Tabling | `table/1`, tabling package | Source transformation plus tabling runtime | Beta |
| Tabled constraints | `t_clpq`, `t_clpr`, TCLP API | Tabling plus solver entailment hooks | Current research feature |
| Alternative search | breadth-first, iterative deepening, Andorra | Packages and runtime support | Current manual |
| Dynamic and concurrent facts | data/dynamic/concurrency packages | Runtime databases | Current |
| Persistent predicates | `persdb` | Package plus persistent storage runtime | Current |
| Low-level threads/concurrency | concurrency primitives | Runtime engine | Current |
| Active modules | `actmod` | Package, mailbox loop, process/network runtime | Devel |
| C FFI | `foreign_interface` | Assertions drive glue generation and shared-object compilation | Current |

### Toolchain and libraries

| Capability | Tool or subsystem |
|---|---|
| Interactive shell | Ciao top level |
| Source debugger | Debugger and Emacs integration |
| Profiling | Profiler package/build grade |
| Testing | Assertion-backed unit-test framework |
| Documentation | LPdoc reads assertions and machine-readable comments |
| Static analysis | CiaoPP abstract domains and fixpoint engines |
| Verification | CiaoPP checks `check` assertions and emits statuses |
| Runtime contracts | Runtime-check transformation for residual assertions |
| Specialization | CiaoPP partial evaluation and code generation |
| Slicing | CiaoPP transformation |
| Parallelization | CiaoPP and optional bundles |
| Incremental compilation | Compiler dependency graph plus `.itf`/`.po` caches |
| Incremental analysis | CiaoPP incremental-analysis subsystem |
| Build/project management | Bundles, workspaces, manifests, `ciao` command |
| Editor integration | Emacs environment and VS Code extension |
| Networking | Sockets, HTTP client/server, active modules |
| Web data | HTML/XML, JSON, templates, PiLLoW libraries |

The complete current manual table of contents is [here](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaofulltoc.html).

## Extension surfaces

### 1. New local syntax or declarations

Use a package:

```prolog
:- package(example_language).

:- op(700, xfx, means).
:- new_declaration(example_decl/1).
:- load_compilation_module(library(example_language_tr)).
:- add_sentence_trans(example_language_tr:sentence/3, 500).
:- add_goal_trans(example_language_tr:goal/3, 500).
```

Data flow:

```text
source term + module
        │ translator predicate
        ▼
replacement term
```

### 2. New CiaoPP transformation

`transform_driver.pl` documents two mechanisms:

- Add clauses for `transform/2` and `transformation/1`.
- Extend the multifile predicates `transformation/4` and `transformation/1` from another module.

The transformation receives program clauses and variable dictionaries and can replace the preprocessing-unit program.

```prolog
transformation(my_pass).

transformation(my_pass, Clauses0, Dicts0, Info) :-
    ...,
    Clauses = ...,
    Dicts = ...,
    replace_program(Clauses, Dicts),
    Info = ... .
```

### 3. New abstract domain

An abstract domain supplies operations used by the PLAI fixpoint engine, such as abstract call transfer, projection, extension, join, widening, builtin treatment, and conversion between domain values and assertion properties.

The lifecycle is:

```text
entry assertion/property
        │ convert to abstract call state
        ▼
clause transfer functions
        │
        ▼
join/widen at recursive call patterns
        │
        ▼
abstract success state
        │ convert back to properties
        ▼
inferred assertion output
```

A 2024 maintainer discussion confirms that an analysis-only property can be declared in source as a native property and interpreted by a domain. Integration with assertion checking, abstract execution, runtime checks, and output touches additional CiaoPP or Ciao library surfaces.

Discussion: [New domains for CiaoPP and how to add new properties](https://github.com/orgs/ciao-lang/discussions/93)

### 4. New constraint solver

Attributed variables customize unification. A tabling-aware solver implements the Mod TCLP projection and entailment interface:

```prolog
call_domain_projection/2
answer_domain_projection/2
call_store_projection/3
answer_store_projection/3
call_entail/2
answer_check_entail/3
apply_answer/2
```

Official manuals:

- [Attributed variables](https://ciao-lang.org/ciao/build/doc/ciao.html/attr_doc.html)
- [Tabling and TCLP](https://ciao-lang.org/ciao/build/doc/ciao.html/tabling_doc.html)

## Observed limits and maintenance friction

### Documentation shape

The main manuals are generated reference inventories. Their pages expose declarations and imports precisely while providing few end-to-end phase diagrams. The Ciao source itself contains a TODO requesting an explanatory figure for package passes.

CiaoPP documentation has separate reference and tutorial manuals. The reference manual explicitly labels its abstract-domain list incomplete and its public flag list outdated. The tutorials provide the shorter entry path.

### Ordered rewrites

Package translations compose by numeric priority and source order. The package manual warns that recursive term and goal translations can loop. It also states that goal translations noticeably slow compilation.

The translation contract is directional:

```text
OldTerm -> NewTerm
```

The predicate implementing the translation can use unification internally, while the compiler schedules it as an ordered rewrite pass.

### Analysis extension crosses subsystem boundaries

Adding an abstract domain involves fewer surfaces when the output stays inside analysis. Assertion verification, runtime checks, abstract executability, property printing, and library integration require mappings outside the domain. Maintainers described this split in the 2024 domain-extension discussion.

### Release and source state

The latest GitHub release is `v1.25.0-m1`, published 2025-06-21. The official 1.25 changelog describes the release as in progress. The tag and `master` point to the same commit. CiaoPP reports version `1.8.0` in its manifest and its current `master` commit is dated 2025-06-10.

### Public issue themes as of 2026-08-27

| Theme | Current evidence |
|---|---|
| Installation portability | [Ubuntu 25.10 install failure #114](https://github.com/ciao-lang/ciao/issues/114), [dangling tarball symlinks #122](https://github.com/ciao-lang/ciao/issues/122) |
| ISO compatibility | [evaluable `(^)/2` #125](https://github.com/ciao-lang/ciao/issues/125), [`clause/2` facts #107](https://github.com/ciao-lang/ciao/issues/107), several older arithmetic and reader issues |
| WASM completeness | [WASM limitations state #123](https://github.com/ciao-lang/ciao/issues/123) |
| Testing and docs | [variable-order unit tests #118](https://github.com/ciao-lang/ciao/issues/118), [tutorial HTML rendering #117](https://github.com/ciao-lang/ciao/issues/117) |
| CiaoPP soundness | [linearity result issue #3](https://github.com/ciao-lang/ciaopp/issues/3) |
| Advanced assertion semantics | Current discussions ask about `trust`, entry assertions, termination compositionality, and higher-order arguments |

The Ciao repository reported 49 open issues during this research pass. The CiaoPP repository reported 3.

### Feature maturity markers

- Active modules are marked `devel` and document deadlock and query-protocol limitations.
- Tabling is marked `beta`.
- Traits are described as experimental in the changelog.
- The command-line compiler manual describes ordinary portable bytecode executables and self-contained engine bundles. Its lower-level generated-C route is documented as incompletely automated.

## Prior art and neighboring systems

The systems below separate the same compiler concerns at different boundaries.

| System | Source extension boundary | Semantic checking boundary | Executable model |
|---|---|---|---|
| Ciao | A module selects packages. Packages register ordered sentence, term, clause, and goal translators. | Assertions become compiler data. CiaoPP interprets them with selectable abstract domains and fixpoints. | Portable bytecode plus a Ciao engine, engine bundles, and a lower-level generated-C route |
| SWI-Prolog | `term_expansion/2` and `goal_expansion/2` hooks run through module, user, and system scopes. | The compiler handles declarations; optional libraries supply contracts and analysis. | Native runtime with compiled virtual-machine code |
| Logtalk | Hook objects define explicit source-expansion workflows. Objects, categories, and protocols structure programs. | Protocols describe public predicate interfaces. Reflection queries object and protocol relations. | Source is translated for a selected backend Prolog system |
| Racket | `#lang` selects a reader and expander for the module. Macros execute at explicit phase levels. | Macro expansion and module phases govern binding; contracts and typed languages are separate language layers. | Racket bytecode or machine code through the selected runtime toolchain |
| Mercury | Language syntax and declarations are fixed compiler inputs. | Types, modes, and mode-specific determinism are inferred or checked by the compiler. | Native and managed-code backends |
| GNU Prolog | ISO Prolog plus built-in compiler extensions. | Static compiler checks plus finite-domain constraints at runtime. | Interpreted code, bytecode, or a machine-dependent standalone native executable |

### Source transformation topology

```text
SWI-Prolog
source term -> module hook -> user hook -> system hook -> compiler

Ciao
source term -> packages sorted by priority -> module expansion -> compiler

Logtalk
source term -> local hook object -> default hook object -> backend compiler

Racket
source bytes -> #lang reader -> hygienic expander at phase N -> module compiler
```

SWI's own `term_expansion/2` manual records composition and tooling problems caused by hooks that can be globally visible and difficult to reuse independently. Ciao narrows language selection to each module's package list and orders translators numerically. Logtalk makes the expansion workflow an explicit object. Racket gives the whole module a reader and expander and tracks compile-time bindings by phase.

Sources:

- [SWI-Prolog `term_expansion/2`](https://www.swi-prolog.org/pldoc/man?predicate=term_expansion%2F2)
- [SWI-Prolog `goal_expansion/2`](https://www.swi-prolog.org/pldoc/man?predicate=goal_expansion%2F2)
- [SWI-Prolog `expand_goal/2`](https://www.swi-prolog.org/pldoc/man?predicate=expand_goal%2F2)
- [Logtalk term and goal expansion](https://logtalk.org/handbook/userman/expansion.html)
- [Racket creating languages](https://docs.racket-lang.org/guide/languages.html)
- [Racket module phases](https://docs.racket-lang.org/reference/module.html)

### Interface and semantic-property topology

```text
Mercury
predicate declaration
    + type
    + mode
    + determinism for that mode
            -> checked directly by the compiler

Ciao
predicate assertion
    + call properties
    + success properties
    + computation properties
            -> stored as assertion rows
            -> interpreted by CiaoPP domains
            -> proved, disproved, retained as a check, or trusted

Logtalk
protocol
    + public predicate declarations
            -> implemented by objects or categories
            -> inspectable through reflection predicates
```

Mercury's determinism category belongs to a particular mode of a predicate. `det`, `semidet`, `multi`, and `nondet` combine solution-count and failure information. Ciao expresses determinism, modes, types, non-failure, resource bounds, and related claims through one assertion language. Each CiaoPP domain supplies the abstract operations needed to infer or verify its property family.

Sources:

- [Mercury determinism](https://mercurylang.org/information/doc-latest/mercury_reference_manual/Determinism.html)
- [Mercury reference manual](https://mercurylang.org/information/doc-latest/mercury_reference_manual/index.html)
- [Logtalk protocols](https://logtalk.org/handbook/userman/protocols.html)
- [Logtalk protocol reflection](https://logtalk.org/manuals/refman/predicates/implements_protocol_2_3.html)

### Native executable topology

GNU Prolog documents three execution routes:

```text
source clause -> interpreter
source clause -> bytecode compiler -> bytecode interpreter
source clause -> native compiler -> machine-dependent executable
```

Ciao's ordinary compiler route produces portable bytecode and links or locates a Ciao engine. An engine bundle packages the engine and bytecode into a distributable executable layout. The compiler manual also documents a lower-level route through generated C, with automation limitations recorded there.

Sources:

- [GNU Prolog native-code compiler](https://www.gprolog.org/manual/html_node/gprolog006.html)
- [GNU Prolog manual](https://www.gprolog.org/manual/gprolog.html)
- [Ciao compiler interface](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaoc.html)

### Placement of Ciao

```text
syntax ownership       module chooses packages
compiler implementation translators are Ciao predicates
semantic facts         assertions live in queryable compiler databases
recursive reasoning    CiaoPP runs abstract fixpoints over program units
runtime execution      Prolog engine, constraints, tabling, concurrency
generated artifacts    bytecode, interfaces, transformed source, engine bundles
```

The package system supplies Racket-like per-module language selection through Prolog predicates. The assertion and CiaoPP system supplies Mercury-like semantic checking through extensible properties and abstract domains. The runtime retains ordinary Prolog unification, search, constraints, and effects.

## Correspondence with the DL6 work

This table records mechanical correspondences. It does not prescribe DL6 syntax or implementation.

| DL6 subject from the current design work | Ciao mechanism | Boundary exposed by Ciao |
|---|---|---|
| `$type` compiler relations | Assertion and preprocessing-unit relations | Ciao stores compiler meaning in queryable predicates, with several mutable databases |
| Userland type operators | Properties, regular types, `modedef`, analyzer-native properties | Property execution, abstract interpretation, runtime checking, and printing have separate adapters |
| Head functor lowering | Functional syntax and DCG sentence translation | Surface terms lower to ordinary predicates with extra arguments |
| `type_apply(Constructor, Args, TypeId)` | Parametric regular-type predicate plus abstract type representation | Ciao usually keeps a predicate/property representation instead of exposing one universal interned type ID |
| Compiler fixpoint over type facts | CiaoPP abstract fixpoint | Domain transfer, join, projection, extension, and widening are explicit interfaces |
| Relation annotations | Assertions and declarations | Authored claims, trusted claims, inferred facts, refutations, and residual checks carry statuses |
| Programmable compiler in the language | Package translators and CiaoPP transformations written in Ciao | Package scheduling is an ordered pass chain; CiaoPP analysis is a fixpoint engine |
| One runtime for compile time and runtime | Compiler loads Ciao compilation modules into the running compiler | Translation predicates execute as Ciao code, while compiler scheduling and storage remain specialized |
| Emitter written in DL6 | CiaoPP transformation plus pretty-printer | Program clauses can be read from and replaced in the preprocessing unit, then emitted as source |
| Clock, determinism, failure, cost checking | CiaoPP computation-property domains | Separate domains infer determinism, non-failure, effects, size, cost, and numeric constraints |
| Recursive type construction termination | Structural widening, shortening, depth-k domains | Finite convergence is supplied by each abstract domain rather than ordinary recursion syntax alone |

### The closest Ciao shape to a relational compiler pass

```prolog
generated_edge(Owner, Name, Target) :-
    source_edge(Owner, Name, SourceTarget),
    transform_target(SourceTarget, Target).
```

This ordinary relation can compute compiler data inside a Ciao module. Registering it as a package translator gives the compiler a directional `OldTerm -> NewTerm` call. Registering an abstract domain gives the fixpoint engine lattice operations around relational transfer predicates. The two extension systems solve different scheduling problems.

```text
package translation                  abstract interpretation
───────────────────                  ───────────────────────
ordered source rewrite               recursive semantic propagation
one item enters                       abstract states revisit call patterns
one item or list leaves               join/widen decides convergence
priority controls order               dependency graph controls revisits
```

### Why `Application` keeps appearing in type discussions

In Ciao functional syntax:

```prolog
fact(N) := ResultExpression.
```

becomes:

```prolog
fact(N, Result).
```

For an interned structural type system, an application is data describing that a constructor was supplied arguments:

```text
application(option, [int])
```

Those are separate uses of the word application:

| Use | Stored shape |
|---|---|
| Predicate/function call | A goal such as `fact(N, Result)` |
| Structural type application | A term or interned node such as `application(option, [int])` |

Ciao regular types commonly represent the latter through predicate heads and clauses:

```prolog
list_of(TypePredicate, Value).
```

DL6's proposed `type_apply/3` materializes an identity for the structural constructor application. Ciao's package system demonstrates the lowering technique. CiaoPP demonstrates the recursive semantic analysis around the resulting program.

## Documentation inventory

### Official entry points

| Resource | Scope | Version/date |
|---|---|---|
| [Documentation portal](https://ciao-lang.org/documentation.html) | Index of manuals, tutorials, discussions, publications | Current portal |
| [Ciao system manual](https://ciao-lang.org/ciao/build/doc/ciao.html/) | Compiler, language, runtime, libraries, tools | 1.25, 2025-06-10 |
| [Ciao full table of contents](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaofulltoc.html) | Complete core manual inventory | 1.25 |
| [CiaoPP reference](https://ciao-lang.org/ciao/build/doc/ciaopp.html/) | Analysis, verification, transformations, abstract domains | 1.8, 2024-10-13 |
| [CiaoPP tutorials](https://ciao-lang.org/ciao/build/doc/ciaopp_tutorials.html/) | Guided static-analysis examples | Generated 2026-03-05 |
| [LPdoc manual](https://ciao-lang.org/ciao/build/doc/lpdoc.html/) | Assertion-aware documentation generation | 3.9, 2024-10-13 |
| [Builder manual](https://ciao-lang.org/ciao/build/doc/ciao_builder.html/) | Bundles, workspaces, builds, installation | Current site |
| [Installation](https://github.com/ciao-lang/ciao/blob/master/INSTALLATION.md) | Platforms, bootstrap, dependencies | Current repository |
| [Bundle catalog](https://ciao-lang.org/bundles.html) | Separately distributed capabilities | Dynamic catalog |
| [GitHub discussions](https://github.com/orgs/ciao-lang/discussions) | Maintainer answers and advanced usage | Current |

### Main Ciao manual inventory

| Part | Contents |
|---|---|
| Getting started | Installation, command line, Emacs, troubleshooting |
| Development environment | Top level, debugger, profiler, builder, compiler, scripts, utilities, editor support |
| Basic language | Modules, bundles, packages, conditional compilation, control, exceptions, terms, arithmetic |
| Assertions | Assertions, regular types, analyzer properties, modes, runtime checking, testing, preprocessing |
| Language extensions | Higher order, traits, named arguments, functions, DCGs, state, delays, search rules, dynamic/persistent facts, concurrency, constraints, tabling, attributed variables, C FFI |
| Compatibility | Classic and strict ISO packages plus compatibility libraries |
| Data structures and algorithms | Lists, strings, maps, arrays, graphs, queues, sets |
| Standard libraries | Streams, term IO, runtime control, compiler loading, OS, paths, processes |
| Additional libraries | Term tools, source tools, sockets, HTTP, HTML/XML, JSON, regex, templates, CLI parsing |

### Source repositories

| Repository | Contents |
|---|---|
| [`ciao-lang/ciao`](https://github.com/ciao-lang/ciao) | Compiler, engine, standard libraries, build bootstrap |
| [`ciao-lang/ciaopp`](https://github.com/ciao-lang/ciaopp) | Abstract interpretation, verification, transformations |
| [`ciao-lang/lpdoc`](https://github.com/ciao-lang/lpdoc) | Documentation generator |
| [`ciao-lang/ciao_vsc`](https://github.com/ciao-lang/ciao_vsc) | VS Code integration |
| [`ciao-lang`](https://github.com/ciao-lang) | Organization and separately distributed bundles |

## Release timeline

| Release | Date | Relevant changes |
|---|---|---|
| 1.25.0-m1 | 2025-06-21 | Current published milestone and documentation baseline |
| 1.24.0-m1 | 2024-10-15 | Baseline corresponding to CiaoPP 1.8 documentation date |
| 1.23.0-m1 | 2024-03-05 | Experimental clause translation after module expansion, explicit meta expansion work, compiler hooks, incremental loading changes |
| 1.22 milestones | 2022-09 to 2023-07 | Builder, workspace, platform, and core iterations |
| 1.20.0 | 2021-04-04 | Editor checking integration, active-module and library work |
| 1.19.0 | 2020-03-20 | TCLP, assertion/runtime-check, build, and documentation work |
| 1.16 | 2016-12-31 | 64-bit engine, emulator generation, engine and compiler refactoring |
| 0.5 | 1998-03-23 | DCGs as expansions and ISO-oriented defaults |
| 0.4 | 1998-02-24 | `new_declaration/1` and modular syntax extensions |
| 0.3 | 1997-08-20 | Assertions, regular types, standalone compiler, modularized builtins |

Official changelog: [Ciao changes](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaochanges.html)

## Advanced recipes

### Inspect a package lowering

1. Open the package file and list `op/3`, `new_declaration`, and `add_*_trans` directives.
2. Open each compilation module named by `load_compilation_module/1`.
3. Follow the translator predicate from its `Old` pattern to its `New` term.
4. Expand the output through module expansion if predicate qualification matters.
5. Inspect any runtime module imported by the package.

### Inspect an assertion failure

1. Identify `calls`, `success`, or `comp` in the failing message.
2. Read the inferred state printed for each abstract domain.
3. Compare it with the property's call-time requirement.
4. Enable program-point information when the mismatch occurs inside a predicate.
5. Add or correct entry information only when it represents the actual external call set.

### Supply an external implementation contract

Maintainer guidance for an implementation known outside Ciao source:

```prolog
:- trust pred external_sum(Xs, Sum)
   : list(int, Xs)
   => int(Sum).

:- impl_defined(external_sum/2).
```

`trust` supplies semantic information. `impl_defined/1` tells the compiler that the missing source definition is intentional.

Discussion: [CiaoPP `trust` qualifier and predicate definition](https://github.com/orgs/ciao-lang/discussions/113)

### Verify and retain annotated source

```bash
ciaopp -o module_checked.pl -V module.pl
```

The output source carries `checked`, `false`, and remaining `check` assertions according to the verification result.

## LLM and automation notes

### Stable retrieval order

1. Read the module's `module/3` package list.
2. Read package files before interpreting unfamiliar syntax.
3. Read translation modules to recover lowering.
4. Read assertions as structured call/success/computation facts.
5. Distinguish authored `check` and `trust` assertions from inferred `true` and verified `checked` assertions.
6. For CiaoPP output, identify the abstract domains and flags used before comparing results.

### Common retrieval errors

| Error | Corrective lookup |
|---|---|
| Treating every Ciao file as one fixed dialect | Inspect `module/3` and `use_package/1` |
| Treating `:=` as a runtime function primitive | Inspect `fsyntax` lowering to an extra predicate argument |
| Treating a regtype as a closed nominal declaration | Inspect the generated `prop` assertion and predicate clauses |
| Treating `trust` as proof | Read assertion status semantics |
| Treating package hooks as a semantic fixpoint | Read ordered translation priorities and compiler phase order |
| Treating CiaoPP type analysis as one algorithm | Record the selected abstract domain, widening, depth, and intermodule flags |
| Treating generated manuals as a tutorial sequence | Start from CiaoPP tutorials, then use reference pages for exact declarations |

## Verification record

| Claim | Verification method | Result |
|---|---|---|
| Current Ciao release | GitHub releases and tags API | `v1.25.0-m1`, 2025-06-21 |
| Current Ciao source revision | GitHub commits API | `fdff410`, same commit as release tag |
| Current CiaoPP version | `Manifest/Manifest.pl` and reference manual | `1.8.0` |
| Package hook API | Public manual plus `core/engine/packages.pl` | Matched |
| Translation ordering | Manual plus `translation.pl`, `c_itf.pl`, `mexpand.pl` | Matched |
| Module/package bootstrap | `c_itf.pl` and `frontend_core.pl` | Matched |
| DCG lowering | Package and translator source | Matched arity `A + 2` state threading |
| Regtype lowering | Package and translator source | Matched `regtype` to `prop + regtype` |
| Functional result argument | Manual examples and package source | Matched predicate arity increase |
| Assertion normalization | Manual plus `assertion_read/9` use in frontend | Matched |
| CiaoPP pipeline | Manuals plus `auto_interface`, `analyze_driver`, `transform_driver` | Matched |
| Abstract-domain inventory | CiaoPP manual and source tree | Manual labels inventory incomplete |
| Runtime execution of examples | Local executable lookup | Unverified locally because Ciao is not installed |

## Primary sources

- [Ciao documentation portal](https://ciao-lang.org/documentation.html)
- [Ciao 1.25 manual](https://ciao-lang.org/ciao/build/doc/ciao.html/)
- [Packages and language extension](https://ciao-lang.org/ciao/build/doc/ciao.html/packages.html)
- [Module system](https://ciao-lang.org/ciao/build/doc/ciao.html/modules.html)
- [Functional notation](https://ciao-lang.org/ciao/build/doc/ciao.html/fsyntax_doc.html)
- [Assertion language](https://ciao-lang.org/ciao/build/doc/ciao.html/assertions_doc.html)
- [Regular types](https://ciao-lang.org/ciao/build/doc/ciao.html/regtypes_doc.html)
- [Runtime assertion checking](https://ciao-lang.org/ciao/build/doc/ciao.html/rtchecks_doc.html)
- [Tabling and TCLP](https://ciao-lang.org/ciao/build/doc/ciao.html/tabling_doc.html)
- [Standalone compiler](https://ciao-lang.org/ciao/build/doc/ciao.html/ciaoc.html)
- [Bundles and workspaces](https://ciao-lang.org/ciao/build/doc/ciao.html/bundles_doc.html)
- [CiaoPP reference](https://ciao-lang.org/ciao/build/doc/ciaopp.html/)
- [CiaoPP abstract domains](https://ciao-lang.org/ciao/build/doc/ciaopp.html/part_domains.html)
- [CiaoPP preprocessing units](https://ciao-lang.org/ciao/build/doc/ciaopp.html/p_unit.html)
- [Ciao repository at pinned revision](https://github.com/ciao-lang/ciao/tree/fdff410cf2b7f2b85baff97485a2db5522d785f3)
- [CiaoPP repository at pinned revision](https://github.com/ciao-lang/ciaopp/tree/241dd12ae8fca3fc06480f2a8cb7a83b70ef7928)
- [Ciao issues](https://github.com/ciao-lang/ciao/issues)
- [CiaoPP issues](https://github.com/ciao-lang/ciaopp/issues)
- [Ciao discussions](https://github.com/orgs/ciao-lang/discussions)
