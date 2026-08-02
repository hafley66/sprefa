# Prolog in Haskell — what exists (survey, 2026-08-01)

Short answer: yes, repeatedly, since 1999. Three distinct things get called this:
(1) a Prolog *interpreter* written in Haskell (toy-to-subset, all stale),
(2) logic programming *as a Haskell library* (LogicT + unification-fd — the live, serious path),
(3) *FFI to a real Prolog* (essentially unmaintained; subprocess/socket is the working route).

## 1. Prolog implementations in Haskell

| Item | Link | What | Status |
|---|---|---|---|
| `prolog` (Hackage) | https://hackage.haskell.org/package/prolog | Subset-Prolog interpreter + parsec parser + TH quasiquoter (`Language.Prolog.Quote`). Bartsch, now Fourné. | v0.3.2, uploaded 2020-08; repo https://github.com/mfourne/prolog last push 2020-08, 8 stars. **Effectively abandoned.** |
| Spivey & Seres, *Embedding Prolog in Haskell* | https://www.semanticscholar.org/paper/600467b2b1763ca4587d1f21f70aaabe29aa7b0e | Haskell Workshop 1999. The founding paper: Prolog's search as a monadic/algebraic embedding, DFS and BFS as instances of one tree-search strategy. | Paper, 1999. Seres thesis *The Algebra of Logic Programming* (2001): http://www.silvija.net/0000OxfordPublications/seres_thesis.pdf |
| `hasklog` | https://github.com/cimbul/hasklog | Prolog subset interpreter **plus a compiler to a simplified WAM**. Most interesting of the toys. | Dormant. |
| `kpci` | https://github.com/fwcd/kpci | Small interactive Prolog REPL in Haskell. | Hobby project. |
| mb64 gist | https://gist.github.com/mb64/97a014a96cd4949d2a6712b773742c98 | "A simple, readable Prolog interpreter in Haskell" — single file, good reading. | Gist, static. |
| `dcepelik/hspl`, `wulalagu/haskell-prolog` | https://github.com/dcepelik/hspl | Coursework-grade interpreters. | Dead. |

Nothing in this row is a production Prolog. There is no Haskell equivalent of SWI/Scryer/Trealla.

## 2. Logic programming as a Haskell DSL (the live path)

| Item | Link | What | Status |
|---|---|---|---|
| `logict` | https://hackage.haskell.org/package/logict | `LogicT` backtracking monad transformer: `msplit`, `interleave`, `>>-`, `once`, `ifte`, `lnot` — fair disjunction/conjunction, i.e. Prolog's search discipline minus its syntax. | v0.8.2.0, 2024-11; maintained by Bodigrim (a GHC-core-libraries maintainer). ~72.5k downloads. **The healthiest thing in this survey.** |
| Kiselyov, Shan, Friedman, Sabry, *Backtracking, Interleaving, and Terminating Monad Transformers* | https://arxiv.org/pdf/1406.2058 | ICFP 2005. The paper `logict` implements. | Canonical. |
| `unification-fd` | https://hackage.haskell.org/package/unification-fd | Generic first-order structural unification over any `Traversable` term functor; STRef or IntMap variable backends (the IntMap one backtracks). Path compression, cyclic-term detection. | v0.12.0.3 uploaded **2026-02**; repo pushed 2026-05, 45 stars. **Actively maintained.** The de-facto unification library. |
| `logict` + `unification-fd` together | — | Combined, these *are* Prolog's two halves as libraries: SLD search and term unification, both typed, both composable with your own monad stack. | Both live. |
| `hkanren` | https://github.com/sergv/hkanren | Typed miniKanren DSL, successor to `ds-kanren`. | Last push 2016, 10 stars. **Dead.** |
| `ds-kanren` | https://hackage.haskell.org/package/ds-kanren | miniKanren subset (Gratzer). | v0.2.0.1, 2014. **Dead.** |
| `MiniKanrenT` | https://github.com/jvranish/MiniKanrenT | miniKanren as a monad transformer. | Dead. |
| µKanren ports | https://www.msully.net/blog/2015/02/26/microkanren-%CE%BCkanren-in-haskell/ , https://github.com/sndtkrh/haskanren , https://github.com/fsestini/mu-kanren | ~40-line µKanren transliterations. Pedagogical. | Static. |
| Erwig, *Escape from Zurg* | https://web.engr.oregonstate.edu/~erwig/papers/Zurg_JFP04.pdf | JFP 2004 pearl: logic-programming-style search in plain Haskell lists. | Paper. |

Verdict on the Kanren family in Haskell: many attempts, zero survivors. Haskell people
route to `logict` instead, because `LogicT` is the same search semantics already in the
ecosystem's maintained core.

## 3. Datalog in Haskell

| Item | Link | What | Status |
|---|---|---|---|
| `souffle-haskell` | https://hackage.haskell.org/package/souffle-haskell | Bindings to **Souffle**: run Souffle programs interpreted, or compile them to C++ and call the generated engine from Haskell. Typed fact marshalling. Luc Tielen. | v4.0.0, 2024-01; repo pushed 2024-05, 105 stars, not archived. **The only serious datalog-from-Haskell option.** |
| `datalog` (travitch) | https://hackage.haskell.org/package/datalog | Pure-Haskell embeddable Datalog; supports arbitrary Haskell predicates. | v0.2.0.2 uploaded **2014**; repo last push 2020, 104 stars. **Abandoned** (still the top hit, still dead). |
| Datafun | https://github.com/rntz/datafun , paper https://www.cl.cam.ac.uk/~nk480/datafun.pdf | Datalog reconstructed as typed lambda calculus with monotonicity types (Arntzenius & Krishnaswami, ICFP 2016). Reference impl is **Racket**; the seminaive work ships a **Datafun→Haskell compiler** (https://www.rntz.net/files/seminaive-datafun.pdf). | Research vehicle, not a library you depend on. |
| `biscuit-haskell` datalog | https://hackage.haskell.org/package/biscuit-haskell | A datalog evaluator exists in-tree, but it is an authorization-token language, not general-purpose. | Live but out of scope. |

## 4. FFI bridges

| Item | Link | What | Status |
|---|---|---|---|
| `hswip` | https://hackage.haskell.org/package/hswip | Embeds SWI-Prolog in Haskell via `libswipl`, modelled on pyswip. | v0.3, **2010**; all builds failed as of 2016-12-29. **Dead — do not start here.** |
| SWI-Prolog MQI | https://www.swi-prolog.org/pldoc/man?section=mqi-overview , https://github.com/SWI-Prolog/packages-mqi | Official: `swipl mqi` listens on TCP/Unix socket, length-prefixed Prolog terms, password auth. Explicitly designed so *any* language that can spawn a process and speak a socket embeds SWI as a library. No Haskell client exists; writing one is small. | Shipped with SWI, maintained. **The actually-working route.** |
| SWI-Prolog C FFI pack | https://www.swi-prolog.org/pack/list?p=ffi | Reverse direction: Prolog calling C, hence Haskell exposed via `foreign export ccall`. | Maintained. |
| `upl` | https://github.com/djellemah/upl | Bidirectional SWI-Prolog FFI — **for Ruby**, not Haskell. Cited only as proof the shape is doable and that nobody did it for Haskell. | 2023, 11 stars. |

## 5. Adjacent: the "this is already a language" answers

- **Curry** — https://www.curry-language.org/ — Haskell syntax plus logic variables, non-determinism, narrowing. The literal answer to "Prolog inside Haskell" as a language design. Live.
- **KiCS2** — https://www-ps.informatik.uni-kiel.de/kics2/ , https://github.com/curry-language/kics2 — compiles Curry **to Haskell** (GHC backend); search space as a data structure, logic variables as generators; DFS/BFS/IDS strategies. Repo pushed **2026-07**, actively maintained. Deterministic code runs at Haskell speed.
- **PAKCS / Curry2Go** — https://www.curry-language.org/implementations/overview/ — sibling Curry backends (Prolog and Go respectively). Live.
- **Mercury** — https://mercurylang.org/ — pure logic/functional language with a Haskell-grade type system plus modes/determinism; its own compiler, no Haskell embedding. Relevant as prior art on typing logic programs, not as a library.
- **A Monadic Implementation of Functional Logic Programs** — https://arxiv.org/pdf/2604.27863 — recent (2026) work re-deriving Curry-style non-determinism monadically in Haskell.

## If you actually wanted this

1. **Backtracking search in Haskell code**: `logict` (+ `unification-fd` if you need terms). Live, cheap, typed, no new language. This is the answer 90% of the time.
2. **Running real Prolog programs from Haskell**: do not use `hswip` (dead since 2010). Spawn `swipl mqi` and speak the MQI socket protocol; ~200 lines of client, and the Prolog stays a real, supported SWI.
3. **Datalog specifically**: `souffle-haskell` — a real compiled engine with typed Haskell marshalling. The pure-Haskell `datalog` package is abandoned; do not build on it.
4. **A Prolog interpreter written in Haskell as a dependency**: nothing qualifies. `prolog` on Hackage is a 2020 subset; treat all of section 1 as reading material.
5. **If the real want is a functional-logic language**: that is Curry, and KiCS2 already compiles it to Haskell — actively maintained in 2026.
