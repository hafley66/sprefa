# diet-scip vs real scip, and a generic type-system import

Receipts plan. Every claim carries a `path:line` or a cited URL. The plain-words
twin with the diagrams is
`plans/2026-08-16-extract-generic-typesystems.PLAN.visual.human.unga.md`.

## Table of contents

1. [The question, restated](#1-the-question-restated)
2. [What diet-scip is today, measured](#2-what-diet-scip-is-today-measured)
3. [What real scip buys, and what it costs](#3-what-real-scip-buys-and-what-it-costs)
4. [The three things the ratchet already proves](#4-the-three-things-the-ratchet-already-proves)
5. [Candidate-by-candidate: how other systems get a language to yield its types](#5-candidate-by-candidate)
6. [Where a generic type-system import would have to bite](#6-where-a-generic-type-system-import-would-have-to-bite)
7. [Diet type guessers: the honest ceiling](#7-diet-type-guessers-the-honest-ceiling)
8. [Adopt vs build, per layer](#8-adopt-vs-build-per-layer)
9. [Forks for Chris](#9-forks-for-chris)
10. [Sources](#10-sources)

---

## 1. The question, restated

Three asks, in the user's framing:

1. diet-scip vs real scip: what is left to improve.
2. how a GENERIC type-system import could work: parse ASTs, project a type
   language, add "diet type guessers" for unresolved refs.
3. how Glean and Joern get a language to yield all its types, and what this repo
   should adopt vs build.

The build-vs-buy law applies to all three. Nothing below concludes "write our
own" without a written candidate analysis first.

## 2. What diet-scip is today, measured

The two doors are named in the wire contract itself, at
`v6/sprefa-extract/src/schema.rs:155-183`:

| door | what runs | what it emits |
|---|---|---|
| `--family scip ROOT` | a real indexer subprocess, budgeted, its whole process group killed on the deadline | v5's eight `scip_*` relation shapes behind a `scip_index` header row |
| `--family diet_scip PATH...` | this crate's own front-ends plus name-match resolution | `resolved_edge`, `resolved_type_edge` |

The contract line that matters, verbatim from `schema.rs:156`:
**"DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA."**

The measured comparison lives at `v6/sprefa-extract/src/deps.rs:16-31`, over
v6/tsv2 against madge, 761 madge edges:

```
         edges  agree  madge-only  own-only  recall  precision
scip       764    755           6         9   0.992      0.988
diet       761    761           0         0   1.000      1.000
```

The module's own reading of its own perfect score, `deps.rs:22-31`:

- diet's 1.000/1.000 is **agreement between two syntactic import scanners, not
  correctness**.
- the 9 edges scip has and diet lacks are **references that reach a declaration
  through an inferred type with no import statement naming it**. That divergence
  is structural. No syntactic resolver closes it without a type checker.
- the 6 edges diet has and scip lacks are files the corpus tsconfig's `include`
  omits, so the indexer never saw them. That one is a program-definition bug on
  the scip side, not a resolution difference.

What diet cannot see at all, stated at `deps.rs:33-37`: dynamic `import()` with
a computed specifier, `require(...)`, `import x = require(...)`.

**So the gap between the doors is one shape, not many: type-driven references
with no syntactic import.** Everything else is either equal or a fixable program
definition.

## 3. What real scip buys, and what it costs

Buys:

- inferred types. A reference that only a type checker can see.
- cross-repo symbol identity: SCIP symbols are qualified strings, not bare names
  (`v6/sprefa-extract/src/scip.rs:22-24` describes rust-analyzer's
  `rust-analyzer cargo <crate> <version> <path>` form).
- the role bitfield: definition / import / write / read / generated / test /
  forward_definition, `v6/sprefa-extract/src/types.rs` `OccurrenceRole`.

Costs, all named in the code:

| cost | receipt |
|---|---|
| a subprocess per root, minutes long, that forks | `v6/sprefa-extract/src/scip.rs:31-35` |
| a hermetic staging copy for indexers that write into the source dir (scip-typescript's `--infer-tsconfig`, rust-analyzer's `cargo metadata`) | `v6/sprefa-extract/src/scip.rs:6-10`, `:19-24` |
| a 13MB index per corpus | `v6/sprefa-extract/src/deps.rs:5-8` |
| a toolchain per language, which may be absent | `v6/sprefa-extract/src/scip_ensure.rs:9-12` |
| three of v5's six languages have no v6 impl at all | `v6/sprefa-extract/src/scip_ensure.rs:35-40`, issue `extract-scip-indexer-roster` |

## 4. The three things the ratchet already proves

This repo did not pick one door. It uses the expensive door to GRADE the cheap
one, per language, in the test suite. That is the design already in the tree, and
it is the answer to "what should we adopt" for a large part of the question.

| leg | receipt |
|---|---|
| `Resolve<CallF>` for rust is "NameResolve primary, ScipOverride on scip disagreement" | `v6/sprefa-extract/src/lang/rust.rs:20-23` |
| go's arm is described as "the scip-ratcheted twin of the TsSource arm" | `v6/sprefa-extract/src/lang/go.rs:16` |
| the disagreement is a first-class edge kind, not a log line | `v6/sprefa-extract/src/types.rs:377-391` (`CallEdgeKind::{NameResolve, ScipOverride}`) |
| three ratchet tests run the REAL indexer and never fake green | `v6/sprefa-extract/tests/golden_parity.rs:679-709` and its go/rust twins; "scip-typescript build failed (the ratchet never fakes green)" |

The shape: cheap answer by default, expensive answer as the grader, disagreement
recorded as data. Extending that shape is cheaper than replacing it.

## 5. Candidate-by-candidate

Every candidate gets the same four questions: how does it get types, what does it
cost to adopt here, what would it replace, and what is the reason not to.

### 5.1 SCIP indexers (Sourcegraph)

**How it gets types.** It does not. It delegates: each indexer is a wrapper
around the language's own type checker (scip-typescript around tsc,
rust-analyzer's `scip` subcommand around rust-analyzer's own analysis, scip-java
around the javac/scalac/kotlinc pipeline). The output format is a protobuf of
occurrences, symbols, relationships and signatures.

**Roster, as of the SCIP docs**: scip-java (Java, Scala, Kotlin),
scip-typescript (TS/JS), rust-analyzer (Rust), scip-clang (C/C++), scip-ruby,
scip-python, scip-dotnet, scip-dart, scip-php.

**Cost here.** Already paid for three languages. The wire is
indexer-agnostic (`v6/sprefa-extract/src/scip_decode.rs`,
`scip_ensure.rs:37-38`: "the decode is already indexer-agnostic, so each is one
`build` body plus its staging decision, not a new wire"). Three more languages
are three impls, which is exactly issue `extract-scip-indexer-roster`.

**Replaces.** Nothing. It is the grader.

**Reason not to.** Per-language toolchain requirement, wall-clock cost, and the
fact that it is a whole-program answer where this crate is content-local
(`v6/sprefa-extract/src/lib.rs:1-8`).

**Verdict: ADOPT MORE OF IT, as the grader only.** Finish the roster. Never make
it the default path.

### 5.2 Glean (Meta)

**How it gets types.** Same delegation, one layer up. Glean is a fact STORE with
a schema language and a Datalog-family query language (Angle), and its facts come
from per-language indexers: C++, Hack, Python, Haskell, Flow written natively,
plus LSIF/SCIP import for Go, Java, Rust and TypeScript. Angle supports derived
predicates, computed on the fly or ahead of time, which is how Glean gives a
language-neutral view over language-specific facts.

**Cost here.** Glean is a Haskell/C++ service with its own storage engine. That
is a database, and this repo already has one (SQLite) plus its own Datalog. The
overlap is nearly total: `predicate` is `rel`, Angle is `.dl6`, derived
predicates are derived rels.

**Replaces.** The whole v6 engine, or nothing.

**Reason not to.** Adopting Glean means adopting its store and its query
language, which is the one layer this repo is legitimately bespoke in
(CLAUDE.md: "The datalog engine core is the one legitimately bespoke layer").

**Verdict: DO NOT ADOPT. STEAL THE SCHEMA IDEA.** The single transferable lesson
is Glean's split between per-language predicates and a language-NEUTRAL derived
layer (their `codemarkup` schema). This repo has the per-language half
(`TypeF` populated by four front-ends) and does not have a written neutral layer.
That layer, in this repo, would be `.dl6` rules over `type_node` /
`resolved_type_edge`, not new Rust.

### 5.3 Joern (code property graph)

**How it gets types.** Two ways, and the second is the interesting one.

1. Fuzzy parsing: language front-ends that tolerate missing code and do not need
   a compilation environment, producing a CPG that unions AST, CFG and PDG. 12
   language front-ends.
2. Type RECOVERY as a pass over that graph, for languages that will not tell it:
   a flow-insensitive algorithm that associates variables, parameters and fields
   with the types they are assigned or annotated with, propagated through the
   graph. Where that fails, the JoernTI work bolts on a learned model
   (CodeTIDAL5, 71.27% on ManyTypes4TypeScript) as a post-processing pass on the
   JS front-end.

**Cost here.** Joern is JVM (Scala) with its own graph store. Adopting the tool
is the same swallow-the-world problem as Glean.

**Replaces.** Nothing this repo would want replaced.

**Reason not to.** JVM runtime, its own storage, its own query DSL.

**Verdict: DO NOT ADOPT THE TOOL. ADOPT THE PASS SHAPE.** Joern's
flow-insensitive type propagation is the exact "diet type guesser" the user
asked about, and it is describable in one paragraph: seed types from
annotations and literals, propagate along assignment edges to a fixpoint. This
repo already has the assignment edges (`DfEdgeKind::Direct`,
`v6/sprefa-extract/src/types.rs:605-612`) and already has a fixpoint engine.
The guesser is a `.dl6` rule set, not a Rust pass. See section 7.

### 5.4 stack-graphs (GitHub)

**How it gets types.** It does not do types. It does NAME RESOLUTION, encoding
name binding as a graph where paths are valid bindings and resolution is
path-finding. Built on tree-sitter, so it needs no build system and no type
checker. File-incremental by construction: each file produces an isolated
subgraph with no visibility into any other file.

The stated limitation, from the comparison literature: SCIP indexers leverage
type checkers to provide inferred types even when not annotated; stack graphs do
not. They work from explicit information only.

**Cost here.** It is a Rust crate, which is the one candidate that fits this
crate's language and its sync/CPU shape. Its rules are written per language in a
DSL over tree-sitter queries.

**Replaces.** The name-match half of `Resolve<CallF>` and `Resolve<TypeF>`. Today
that half is literally a corpus-wide name match
(`v6/sprefa-extract/src/lang/dl6/_0_source.rs:425-447` shows the pattern:
`call_name_match(output, index, callee)`), and `schema.rs:176-178` concedes it
"is wrong wherever a name is ambiguous corpus-wide, which is what the other name
buys."

**Reason not to.** A stack-graph rule set per language is real work, and it is
the same work as writing the resolver by hand, only in someone else's DSL.

**Verdict: THE ONE REAL BUY CANDIDATE, and it needs a spike before a decision.**
Its file-incremental property matches this crate's content-local law exactly
(`v6/sprefa-extract/src/types.rs:1077-1081`: phase 1 keys on
`(BlobHash, lang, FamilyMask)`, phase 2 on `(BlobHash, ProjectDigest, FamilyMask)`).
That is the same insight, arrived at independently. A spike on ONE language,
graded against the existing scip ratchet, would answer whether it beats the name
match. Nothing else in this document has that shape.

### 5.5 Kythe (Google)

**How it gets types.** Compiler-plugin indexers emitting a language-neutral graph
schema, designed for interoperability without language-specific adaptations in
the consumer.

**Cost here.** Compiler plugins, per language, in a build system.

**Reason not to.** Beyond the plugin cost: the US development team was laid off
in April 2024 and replaced with an overseas maintenance team, which is a
maintenance-risk fact, not a technical one.

**Verdict: DO NOT ADOPT.** SCIP is the live protocol in this space and this repo
already speaks it.

### 5.6 Language servers directly (LSP), not via SCIP

**How it gets types.** The type checker, live, over a document.

**Cost here.** A stateful long-running process per language, an async protocol,
and per-request latency. This crate is declared sync, pure CPU, no async
(`v6/sprefa-extract/src/lib.rs:1-8`), and the user decision "I DO NOT WANT TO RUN
V5 ANYTHING ANYMORE" came partly out of the LSP path
(`plans/2026-08-12-v6-native-lsp.PLAN.md`).

**Verdict: DO NOT ADOPT for extraction.** SCIP is the batch form of exactly this
answer and it is already wired.

### 5.7 Doing nothing beyond the current name match

**Cost.** Zero.

**What it forfeits.** The 9-edge shape from section 2, corpus-wide name
ambiguity (`schema.rs:176-178`), and every unannotated-type answer.

**Verdict: the baseline every option above must beat, per language.** The
ratchet tests already measure that number, so this is falsifiable rather than
argued.

## 6. Where a generic type-system import would have to bite

The user's sketch: parse ASTs, project a type language, guess the rest. Against
the current code, that decomposes into four layers, and this repo has exactly
two of them.

| layer | state | receipt |
|---|---|---|
| L1 parse | DONE, 8 front-ends | `v6/sprefa-extract/src/lang/mod.rs:40-51` |
| L2 project a type language | DONE, 9 entity kinds + 7 edge kinds | `types.rs:196-224`, `:226-253` |
| L3 bind a name to a declaration | PARTIAL, corpus name match plus a scip override | `types.rs:377-391`, per-lang `Resolve` arms |
| L4 INFER a type where none is written | ABSENT | `types.rs:594-600`: `TypeSig.ty` is "the referenced type's bare name (unresolved in phase 1; `Resolve<TypeF>` binds it)". A bare name is not a type. |

**L2 is the observation that decides the answer, and it is already true.** The projected
type language is nine node kinds and seven edge kinds. That is a small,
closed vocabulary that every one of the four front-ends already fills. A fifth
language does not extend it; `extract-python-arm` is a new filler for the same
vocabulary, not a new vocabulary.

So "generic type-system import" is not a new abstraction to design. It is
already the shape of the crate. The open work is L4, and one honest question
about L2:

**Is a 7-kind edge vocabulary enough to carry a type SYSTEM, or only a type
GRAPH?** Today it carries the graph. It has no arrow type as a first-class node
(a function's signature is `TypeSig` rows in aux, not a type node), no type
application, no variance, no bounds. `TypeEdgeKind::Generic` is one edge, so
`Vec<Map<K, Vec<V>>>` flattens to a set of `generic` edges with the nesting gone.
That is a design question for Chris, not a lane, and it is the same question the
generics inspection doc was told to answer first (CLAUDE.md: "Generics need a
written inspection, in docs, before any generics work").

## 7. Diet type guessers: the honest ceiling

The user's phrase, made precise. A "diet type guesser" is L4 without a type
checker. Three tiers exist, and only the first two are defensible here.

### Tier 1: propagation from written types (Joern's actual algorithm)

Seed: every place a type IS written (annotations, literals, constructor calls,
`as` casts). Propagate along assignment edges to a fixpoint. Flow-insensitive:
one type set per variable, not per program point.

This repo already has every input:

| input | receipt |
|---|---|
| assignment edges | `DfEdgeKind::Direct`, `types.rs:605-612` ("dst receives the value of src") |
| the literal seeds | `DfNodeKind::Lit`, `types.rs:552` |
| the constructor seeds | `DfNodeKind::New`, `types.rs:554` |
| the annotation seeds | `TypeSig` rows, `types.rs:594-600` |
| a fixpoint engine | the v6 datalog engine, which is the whole point of the repo |

**So Tier 1 is a `.dl6` rule set over facts that already exist, not new Rust.**
That is the cheapest real answer in this document. It is also blocked on one
thing: propagation across function boundaries needs the arg-to-param hop, which
is the commented-out `Flow` union (`types.rs:605-612`, issue
`extract-df-flow-union`, needs-chris).

Ceiling: it recovers types that were written SOMEWHERE and flowed. It cannot
recover a type that was never written anywhere, and it is unsound in every
direction a real checker is sound (no generic instantiation, no overload
resolution, no subtyping).

### Tier 2: name-match plus scip override, the current arm

Already built, already graded. Section 4.

### Tier 3: a learned model (JoernTI / CodeTIDAL5)

71.27% on ManyTypes4TypeScript, state of the art at publication, integrated as a
post-processing pass.

**Reject for this repo, on a stated law rather than a judgment.** A guessed type
that cannot be traced to a written one would enter a system whose central user
decision is "no coercions" (CLAUDE.md, pinned at `lower.pl:1826` and
`lower.pl:335`). A probabilistic type feeding a comparison the language refuses
to make silently is a contradiction. If it ever lands, it lands as its own
edge kind with its own confidence column, never merged into `type_edge`.

### The rail that makes any of this safe

Whatever tier lands, the answer must say WHICH tier answered. The precedent is
already in the tree twice:

- `CallEdgeKind::{NameResolve, ScipOverride}` (`types.rs:377-391`) records HOW a
  call edge was bound.
- `deps.rs` `Policy` records WHY a specifier resolved or did not
  (`deps.rs:39-42`: "EVERY RULE IS A STATED POLICY, never a silent heuristic").

A `TypeBinding` kind alongside those two is the same move a third time.

## 8. Adopt vs build, per layer

| layer | verdict | why |
|---|---|---|
| indexer subprocess seam | ADOPT, finish the roster | `scip_ensure.rs:35-40`; three impls owed |
| type-checker-grade inference | ADOPT (SCIP), never build | a type checker is not a common-shaped problem, it is the language's own compiler |
| name resolution | SPIKE stack-graphs against the existing ratchet before choosing | file-incremental matches this crate's cache key exactly |
| the projected type vocabulary | KEEP, it is already generic | `types.rs:196-253`; a fifth language fills it rather than extends it |
| Tier-1 type propagation | BUILD, in `.dl6`, not Rust | every input fact already exists; the engine is the fixpoint |
| learned type inference | REJECT for now | collides with the no-coercions decision |
| a fact store / query language | ALREADY BUILT, do not adopt Glean or Joern | the one legitimately bespoke layer |

## 9. Forks for Chris

Decided by nobody. Each needs a call before the work it gates can be dispatched.

| # | fork | the two sides | what it gates |
|---|---|---|---|
| F1 | Does the type vocabulary carry a type SYSTEM or a type GRAPH? | (a) 7 edge kinds is the answer, nesting and arrows stay outside the graph; (b) the vocabulary grows arrow types, application and bounds as first-class | every generics answer; ties to the generics inspection doc |
| F2 | Is Tier-1 propagation a `.dl6` rule set or a Rust pass? | (a) `.dl6`, so the engine is the fixpoint and the rules are readable; (b) Rust, so it rides the one parse and needs no engine round trip | where L4 lives |
| F3 | Does an inferred type get its own edge kind? | (a) yes, `TypeBindingKind` beside `CallEdgeKind`, so a consumer can refuse guesses; (b) no, inferred and written types are one relation | the no-coercions decision reaches type edges or does not |
| F4 | Do we spike stack-graphs? | (a) one language, graded against the existing scip ratchet, then decide; (b) keep the name match and spend the effort on the indexer roster instead | the resolution layer for the next five years |
| F5 | Is the interprocedural hop a Flow edge kind, a separate family, or a dl join? | see issue `extract-df-flow-union` | Tier-1 propagation across function boundaries |

## 10. Sources

- [Indexing code at scale with Glean, Engineering at Meta](https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/)
- [Glean documentation, Introduction](https://glean.software/docs/introduction/)
- [Glean, Querying Glean (Angle)](https://glean.software/docs/query/intro/)
- [facebookincubator/Glean](https://github.com/facebookincubator/Glean)
- [Introducing stack graphs, The GitHub Blog](https://github.blog/open-source/introducing-stack-graphs/)
- [github/stack-graphs](https://github.com/github/stack-graphs)
- [Stack Graphs: Name Resolution at Scale (DROPS)](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.EVCS.2023.8)
- [Joern documentation](https://docs.joern.io/)
- [Code Property Graph](https://cpg.joern.io/)
- [joernio/joern](https://github.com/joernio/joern)
- [Learning Type Inference for Enhanced Dataflow Analysis (JoernTI / CodeTIDAL5)](https://arxiv.org/abs/2310.00673)
- [joernio/joernti](https://github.com/joernio/joernti)
- [SCIP Code Intelligence Protocol](https://scip-code.org/)
- [SCIP, a better code indexing format than LSIF, Sourcegraph](https://sourcegraph.com/blog/announcing-scip)
- [sourcegraph/scip](https://github.com/sourcegraph/scip/tree/main)
- [Kythe overview](https://kythe.io/docs/kythe-overview.html)
- [Google Kythe, Wikipedia](https://en.wikipedia.org/wiki/Google_Kythe)
