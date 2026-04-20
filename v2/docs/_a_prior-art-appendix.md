# Prior art appendix

Source: chat 2026-04-19. Search-engine-mode survey of language
design ideas adjacent to sprf v3. Treat as starting pointers, not
authoritative readings. None of these are recommendations.

## 1. Pattern matching + matcher composition

| Language / system          | Idea                                                                              |
| -------------------------- | --------------------------------------------------------------------------------- |
| Prolog                     | Unification, cut `!`, atoms vs vars by casing, DCG (definite-clause grammars)     |
| Tree-sitter queries        | `(node @cap)` capture syntax, `#match? @cap "re"` predicates, `#eq?`, `#any-of?`  |
| CUE                        | Bidirectional unification of partial values; constraints as types; JSON-shaped    |
| Racket `match`             | Pattern + guard + bind in one form: `(match x [(list a b) #:when (> a 0) ...])`   |
| OCaml/Rust patterns        | Or-patterns `A | B`, guards `if cond`, bindings as substructure                  |
| APL family `where`         | Inline filter on array dimensions                                                  |
| PEG / Ohm                  | Parser with semantic actions; rules are first-class composable                    |
| ast-grep YAML rules        | `all`/`any`/`not`/`inside`/`has`/`follows`/`precedes`                              |

Closest to expression-as-matcher: tree-sitter query predicate
system. Worth reading their predicate registration mechanism.

## 2. Stream / pipeline composition

| System                         | Operator       | Note                                          |
| ------------------------------ | -------------- | --------------------------------------------- |
| bash / sh                      | `|` `>` `<`    | text streams, redirect, process substitution  |
| PowerShell                     | `|`            | object pipelines (cursors with structure)     |
| Nushell                        | `|`            | typed shell; pipeline carries Value w/ schema |
| Elvish / Murex / Oil           | `|`            | typed shells, various trade-offs              |
| F# / Elixir / OCaml            | `|>`           | forward-pipe operator                         |
| BigQuery pipeline syntax       | `|>`           | SQL pipelined: `FROM t |> WHERE x |> SELECT y`|
| Linq (C#)                      | `.`            | method chaining                                |
| Reactive Streams / RxJS        | `pipe(...)`    | observable.pipe with backpressure              |
| Java Streams / Kotlin Flow     | `.`            | lazy collection pipelines, async variant       |
| Apache Pig                     | line-per-step  | `B = FILTER A BY ...; C = JOIN ...`           |
| Differential dataflow          | combinators    | incremental relations; updates as diffs        |
| Concatenative (Joy/Factor)     | juxtaposition  | point-free; every word transforms the stack    |
| APL family                     | juxtaposition  | implicit pipeline via array operator composition |
| Glean / Soufflé                | `:-`           | datalog with rule chaining as logical implication |

Closest to ergonomic-named-intermediates: BigQuery `|>` and Pig
line-per-step. Both let you name intermediate results.

## 3. Bind / scope / unification

| Mechanism                               | Where                                     | What                                                  |
| --------------------------------------- | ----------------------------------------- | ----------------------------------------------------- |
| Lexical let                             | most languages                            | name a value in a scope                                |
| Pattern destructuring                   | OCaml, Rust, Scala, Haskell, JS           | bind multiple names from one value                     |
| Unification                             | Prolog, Mercury, Curry                    | bidirectional binding; either side can be unknown     |
| Algebraic data types + match            | ML family                                  | bind by structural shape                               |
| Linear / affine binding                 | Rust, Linear Haskell                       | bindings consumed by use                               |
| Logic variables (miniKanren)            | Scheme tradition                          | search-based binding under constraint                  |
| Dependent binding                       | Coq/Lean/Idris                            | binding carries dependent type info                    |
| Capture-by-naming (Racket syntax)       | Racket macros                             | hygienic binding capture across syntactic boundaries  |
| Erlang/Prolog casing convention         | Erlang, Prolog                            | parser disambiguates atom vs variable by first char    |
| CUE constraints                         | CUE                                       | values as constraints; unification merges              |

Closest to "search for bindings satisfying constraints":
miniKanren. ~200 lines embedded in Scheme.

## 4. Effects, caching, approval, side effects

| System                          | Effect model                                                          | Cache / approve                                       |
| ------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------- |
| Haxl (Meta)                     | typed Fetch effect with batching/dedup                                 | per-request memo by request key                        |
| GraphQL DataLoader              | per-request batch+cache wrapper                                        | request-scoped cache                                   |
| Tower / Tokio middleware        | service composition; layers around inner Service                       | cache as a layer                                       |
| Salsa (rust-analyzer)           | named queries with input/derived split                                  | demand-driven memoization, invalidation by input diff  |
| Bazel / Buck / Pants            | content-addressed build actions                                        | hermetic action cache by input hash                    |
| Nix / Guix                      | pure derivations, content-addressed                                    | global cache by derivation hash                         |
| Make / Ninja                    | timestamp-based incremental build                                       | mtime cache                                             |
| Cargo                           | fingerprint of source + deps + flags                                   | `target/` cache by fingerprint                          |
| Pulumi / Terraform              | declarative target state with diff/apply                                | preview-then-approve workflow                          |
| Ansible / Chef                  | idempotent operations with check-mode                                   | dry-run vs run                                          |
| Algebraic effects (Eff, Koka)   | separate effect declaration from handler                                | effect handler chooses execution                        |
| F# computation expressions      | sugar over monadic composition                                         | per-monad caching/lazy semantics                        |
| Haskell do-notation             | sugar over monadic composition                                         | per-monad caching/lazy semantics                        |
| Sapling / Jujutsu               | content-addressed VCS objects, reversible operations                   | no-op detection, undo                                   |

Closest to incremental-named-queries: Salsa (rust-analyzer).
Closest to action-as-(inputs hash → outputs): Bazel.
Closest to preview/apply gap: Pulumi.

## 5. Code-as-data, queryable codebases, edge declarations

| System                                  | What it does                                                            |
| --------------------------------------- | ----------------------------------------------------------------------- |
| CodeQL (GitHub)                         | datalog-style queries over compiled code DBs                            |
| Semgrep / Comby                         | pattern-match across code with metavars                                  |
| Glean (Meta)                            | datalog facts about code, cross-repo, schema-driven                     |
| Stack Graphs (GitHub)                   | name resolution as graph traversal across files/repos                    |
| SCIP / LSIF                             | code-intelligence index format                                          |
| Kythe (Google)                          | code-knowledge graph schema                                              |
| TreeSitter `tags.scm`                   | per-language definition/reference extraction queries                     |
| Soufflé / DDLog / Datafrog              | datalog engines suitable for static analysis                             |
| Differential Datalog (DDLog)            | incremental datalog                                                      |
| Sourcegraph batch changes               | declarative cross-repo refactor                                          |
| OpenRewrite                             | recipe-based code transforms in JVM ecosystem                           |
| Comby                                   | structural search/replace agnostic of language                          |

Closest to cross-codebase-typed-edges thesis: Glean. Schema design
is mature. Worth reading their paper.

## 6. Host language + sub-language composition

| System                              | Mechanism                                                                  |
| ----------------------------------- | -------------------------------------------------------------------------- |
| Racket `#lang`                      | per-file language choice; macros define new languages atop racket's core   |
| Lisp reader macros                  | extend the parser per-form                                                  |
| MPS (JetBrains)                     | projectional editing; one AST, multiple notations                          |
| Tree-sitter language injection      | grammars host other grammars at declared sites                              |
| MetaOCaml / Template Haskell        | staged code generation                                                      |
| F# type providers                   | sub-language schemas pulled into host's type system                        |
| Roslyn analyzers                    | extension API into a host language's compile pipeline                      |
| PHP `<?= $x ?>`                     | interpolation as language boundary, host owns holes                         |
| Embedded SQL (in JS, in Rust)       | tagged template literals or proc macros                                    |

Closest to per-op-grammar with hygiene: Racket `#lang`. Decades of
prior art on language towers + hygiene.

## 7. Casing-as-syntax precedents

| Language        | Convention                                                               |
| --------------- | ------------------------------------------------------------------------ |
| Prolog          | `lowercase` = atom; `Uppercase` = variable; `_` = anonymous               |
| Erlang          | same as Prolog (Erlang descends from Prolog)                              |
| Mercury         | same as Prolog                                                            |
| Haskell         | `lowercase` = value/var; `Uppercase` = type/constructor                   |
| Go              | `Uppercase` = exported; `lowercase` = unexported                          |
| Ruby            | `Uppercase` = constant; `lowercase` = local; `@var` instance, `$var` global |
| Smalltalk       | `lowercase` = local; `Uppercase` = class                                   |
| Python (convention only) | `UPPER_SNAKE` = const; `CamelCase` = class; `snake_case` = func   |

Casing-as-syntax (parser-enforced) is rare. Prolog/Erlang/Mercury
are the only widely-used examples. Polarizing taste choice with
real ergonomic consequences.

## 8. Less-common prior art touching the design space

| Thing                           | Why it might matter                                                                |
| ------------------------------- | ---------------------------------------------------------------------------------- |
| Unison                          | functions are content-addressed values; "named pipeline as compiled value" literal |
| Esterel / Lustre                | synchronous reactive languages; clock-driven streams; aerospace-grade rigor         |
| Bloom / Dedalus                 | distributed datalog with time as a relation                                          |
| Lean tactic language            | DSL for composing proof tactics; `<;>` combinator for "try alternatives"             |
| Coq's Ltac                      | tactic combinators with backtracking                                                  |
| Forth/Factor word definition    | `: NAME ... ;` is "name a pipeline"; idiomatic concatenative                         |
| Rebol / Red                     | dialects (sub-languages) as first-class                                              |
| Tcl                             | string-as-everything; minimum syntax, max malleability                               |
| Awk                             | pattern-action programs; `/regex/ { action }` is `${re{...}} > action`               |
| Sed                             | line-oriented stream editor                                                          |
| Recursive Schemes (Haskell)     | `cata` / `ana` / `hylo` for tree traversal                                           |
| Skylark / Starlark (Bazel)      | restricted Python for declarative graphs; deterministic by design                    |
| Pkl (Apple)                     | typed config with computation; constraints + composition                             |
| Nickel (Tweag)                  | gradual typing for config; merges + contracts                                        |
| Dhall                           | total functional config; no recursion; safe import resolution                        |
| PromQL / LogQL                  | observability query languages; pipeline semantics                                    |
| Splunk SPL                      | text-pipeline + structured search                                                    |
| Drools / CLIPS                  | production rule systems; rules + working memory + agenda                             |
| CHR (Constraint Handling Rules) | rules with constraint store                                                          |
| Datafun                         | typed datalog with monotone semantics                                                 |

## 9. Design features / semantic gaps not yet surfaced

| Gap                                     | What it covers                                                                |
| --------------------------------------- | ----------------------------------------------------------------------------- |
| Recursion / fixpoint semantics          | Can a sprf rule reference itself? Termination story?                          |
| Negation semantics                      | `!has` exists; what about stratified negation, default negation?              |
| Aggregation primitives                  | `count`, `sum`, `min`, `max`, `group_by`                                       |
| Time / temporal queries                 | "true at rev X but not Y"; "this assumption held until Z"                     |
| Module system / namespacing             | How do `.sprf` files include or compose with each other?                      |
| Type system shape                       | Static? Dynamic? Gradual? Inferred?                                           |
| Error semantics                         | Total functions? Partial? Diagnostic emission as effect?                       |
| Determinism guarantees                  | Same input → same output guaranteed?                                           |
| Hygiene model                           | If named pipelines first-class, what's variable capture rule?                  |
| Termination / totality checking         | Does the system guarantee a query finishes?                                    |
| Equivalence / equality semantics        | When are two cursors equal? Two patterns? Two pipelines?                       |
| Streaming vs batch boundary             | Where does the language switch between push/pull?                              |
| Provenance / explainability             | "Why did this row get emitted?" Datalog has this natively                      |
| Refinement / contracts                  | Specify input/output shapes for ops                                            |
| Reflection / introspection              | Can a sprf rule query its own AST or other rules?                              |
| Concurrency model                       | What runs concurrently with what; observable interleavings                     |

Highest-priority based on stated goals: provenance, aggregation
primitives, module system, negation semantics, temporal queries.

## Honest limits of this survey

- Most entries are starting-pointer accuracy, not deep familiarity.
- "Closest to" callouts are guesses about fit, not authoritative.
- Several entries (CHR, Datafun, Bloom, MPS) are read-about-only.
- Newer entries (Pkl, Nickel, CUE) move fast; specifics may be stale.

The point: dig 1-2 hours into 2-3 entries that catch the eye;
ignore the rest. The win is finding the analogy that makes a design
choice feel inevitable in hindsight.
