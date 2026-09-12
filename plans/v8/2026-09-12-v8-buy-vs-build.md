# v8 buy-vs-build: evaluator, reader, interning

Candidate-by-candidate research for the Rust rewrite of the DL7 compiler
(`v7/` SWI-Prolog -> `v8/` Rust). Every fact carries the URL it came from.
Facts gathered 2026-09-12 against crates.io, docs.rs and GitHub live APIs.

## Table of contents

- [0. What v8 has to satisfy](#0-what-v8-has-to-satisfy)
- [1. Datalog / relational fixpoint engines](#1-datalog--relational-fixpoint-engines)
  - [1.1 Candidate table](#11-candidate-table)
  - [1.2 Per-candidate notes](#12-per-candidate-notes)
  - [1.3 Compile-time-rules group](#13-compile-time-rules-group)
- [2. Lisp / S-expression readers](#2-lisp--s-expression-readers)
  - [2.1 Candidate table](#21-candidate-table)
  - [2.2 Per-candidate notes](#22-per-candidate-notes)
  - [2.3 The tree-sitter ABI check](#23-the-tree-sitter-abi-check)
- [3. Interning and index storage](#3-interning-and-index-storage)
  - [3.1 Candidate table](#31-candidate-table)
  - [3.2 Per-candidate notes](#32-per-candidate-notes)
- [4. Recommendation per question](#4-recommendation-per-question)
- [5. Source index](#5-source-index)

---

## 0. What v8 has to satisfy

Requirements taken from the brief and from the v7 contract, not re-derived.

| requirement | v7 receipt |
|---|---|
| rules are DATA loaded at runtime from `.dl7` | `v7/src/1_libtime/0_evaluator.pl:29` `evaluate(+Rules, +Seeds, -Closure, -Diagnostics)`, `must_be(ground, Rules)` |
| stratified fixpoint, negation over lower strata | `v7/src/1_libtime/0_evaluator.pl:5` exports `stratify_rules/3`; `evaluate_strata/8` passes `LowerRows` down |
| semi-naive rounds | closure-round budget is a pinned compiler metric (`v7/README.md`, "8 versus 7 closure rounds") |
| integer comparison builtins | required by brief |
| per-column indexes | `v7/src/1_libtime/0_evaluator.pl:23` `evaluation_lower_index/7` |
| deterministic row order | `v7/src/1_libtime/0_evaluator.pl` sorts stratum seeds (`sort(Seeds0, StratumSeeds)`) |
| effect / trace sink per round | `v7/src/2_comptime/1b_compiler_tracer.pl` `run_compile_step/4`, wrapping install / collect / cleanup per stratum |
| two fixpoint phases share one evaluator | macrotime over a syntax graph (`v7/src/1_libtime/0a_syntax_macro_program.pl`) and comptime over a lowered program (`v7/src/2_comptime/`) both call `evaluate/4` |

Size of the thing being replaced: `v7/src/1_libtime/0_evaluator.pl` is 683 lines
of Prolog; the DL7 tree-sitter grammar is 57 lines
(`v7/tree-sitter-dl7/grammar.js`).

---

## 1. Datalog / relational fixpoint engines

### 1.1 Candidate table

| crate | version | last release | license | rules at runtime? | stratified negation? | semi-naive? | indexes? | deterministic order? | dep weight (required normal deps) | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| ascent | 0.8.1 | 2026-08-29 | MIT | NO, proc macro | YES | YES | YES, per-rule logical indices | not stated | 8 | disqualified for the compiler; live option for EMITTED programs |
| crepe | 0.2.0 | 2025-12-14 | MIT OR Apache-2.0 | NO, proc macro | YES | YES | generated index structs | not stated | 5 (all proc-macro) | disqualified for the compiler; weaker than ascent for emitted use |
| datafrog | 2.0.1 | 2019-01-02 | Apache-2.0/MIT | partial, joins are Rust closures | NO | YES, recent/stable split | leapjoin over sorted `Relation` | YES, sorted distinct | 0 | closest base layer; not a complete engine |
| differential-dataflow | 0.25.1 | 2026-07-15 | MIT | dataflow built by Rust code; you write the interpreter | NO built-in stratifier | stronger (incremental) | arrangements | NO, `(data, time, diff)` multiset, worker-scheduled | 7 + timely's 11 | overweight for a single-process compiler |
| timely | 0.31.0 | 2026-07-14 | MIT | n/a, lower layer | n/a | n/a | n/a | NO | 11 | not a Datalog engine |
| cozo | 0.7.6 | 2023-12-11 | MPL-2.0 | YES, CozoScript string | YES | YES | database indexes | database-ordered | 44 required + 11 optional | a database, and upstream is stale |
| mnestic (maintained cozo fork) | 0.18.0 | 2026-09-05 | MPL-2.0 | YES, CozoScript string | YES | YES | database indexes | database-ordered | 44 required + 19 optional | maintained, still a whole database |
| retia-bin (Rust-only cozo-bin fork) | 0.2.0 | 2026-06-28 | MPL-2.0 | YES | YES | YES | database indexes | database-ordered | binary crate, no library surface published | binary, not a library |
| egglog | 3.0.0 | 2026-08-19 | MIT | YES, `parse_and_run_program` | NO negation in the fact language | YES, `EGraph.seminaive: bool` | YES | not stated | 21 required + 3 optional | equality saturation; the negation gap is fatal |
| ddlog / differential-datalog | no crates.io release | repo archived, last push 2023-07-07 | - | NO, source-to-source compiler | YES | YES | YES | - | - | dead |
| souffle (crates.io stub) | 0.0.1 | 2019-07-28 | MIT/Apache-2.0 | NO | - | - | - | - | 741 bytes packaged, "demo crate, wip" | not a binding |
| souffle (the C++ tool) | - | repo pushed 2026-07-13 | UPL-1.0 | NO, synthesizes C++ from `.dl` | YES | YES | YES | YES | C++ toolchain at build time | wrong shape and wrong process model |
| naga-datalog | does not exist | - | - | - | - | - | - | - | - | not a crate; `naga` on crates.io is the wgpu shader translator |
| datalog | 0.0.2 | 2017-04-07 | MIT | - | - | - | - | - | 2,417 bytes | abandoned; its own description begins "INCOMPLETE!" |
| dlog | 0.1.2 | 2021-04-23 | non-standard | - | - | - | - | - | - | name collision: a time-tracking CLI |
| datalog-core | 0.1.0 | 2026-08-22 | MIT OR Apache-2.0 | YES, rule IR built from data | YES, stratification analysis only | NO evaluator | NO | NO | 1 (`rustc-hash`) | IR-only; 42 lifetime downloads, 0 stars |
| flowlog (build/runtime/parser) | build 0.4.0, runtime 0.3.0, parser 0.1.0 | 2026-07-26 | Apache-2.0 | NO, `build.rs` compile step | YES, stratifier stage | via differential-dataflow | DD arrangements | not documented | runtime pulls DD + timely + lasso + regex | disqualified, build-time door |
| purrdf-datalog | 1.1.0 | 2026-09-04 | MIT OR Apache-2.0 | YES, `DlClause` IR | YES | YES, `seminaive` module | YES, ordered-map arrangements | YES, byte-identical output claimed | 7 | arity-4 RDF quads only; disqualified |
| minigraf | 2.0.0 | 2026-08-26 | MIT OR Apache-2.0 | YES, query language | not documented | not documented | database indexes | bi-temporal | - | embedded graph DB, not an evaluator library |

### 1.2 Per-candidate notes

**ascent 0.8.1.** The README states "Ascent supports stratified negation and
aggregation" at line 155 of `README.MD`, and the generated code carries the
semi-naive split explicitly: `MirRelationVersion` has `Total`, `Delta`,
`TotalDelta` and `New` variants (`ascent_macro/src/ascent_mir.rs:189-199`) and
codegen emits `RelIndexMerge::merge_delta_to_total_new_to_delta`
(`ascent_macro/src/ascent_codegen.rs:480`). Per-relation logical indices are
minted per rule and documented into the generated struct
(`ascent_macro/src/ascent_codegen.rs:23-52`). Every rule must appear literally
inside the `ascent!` or `ascent_run!` macro body; `ascent_run!` only changes
whether local Rust variables are in scope, not where the rules come from. That
makes ascent unusable as the compiler's own evaluator, where the rule set is
whatever a `.dl7` file says. It stays the best-shaped candidate for EMITTED
programs, where v8 would generate Rust source containing an `ascent!` block.

**crepe 0.2.0.** Description: "Datalog in Rust as a procedural macro". The
README's feature list names "Semi-naive evaluation" (line 15) and "Stratified
negation" (line 16); usage is `crepe! { ... }` followed by `Crepe::new()` and
`runtime.run()` (README lines 42-45). Same compile-time disqualification as
ascent, with a smaller feature set and a smaller user base (83,981 recent
downloads against ascent's 168,928). If v8 ever emits Rust Datalog, ascent wins
that slot on maintenance and on BYODS.

**datafrog 2.0.1.** Zero dependencies, 442,489 recent downloads, and the
`Relation` type is documented as a "Sorted list of distinct tuples", which
delivers the deterministic-order requirement for free. `Variable` is "a
monotonically increasing set of Tuples" and the recent/stable/to_add split is
the semi-naive machinery. The README is blunt about what it is not: "Datafrog
has no runtime, and relies on you to build and repeatedly apply the update
rules." There is no negation operator, no stratifier, no builtin comparisons and
no trace hook. The join closures are Rust code, so a rules-as-data front end
means writing a plan interpreter that erases tuple types into something like
`Vec<Value>` and hand-rolling every one of the five missing features. Last
release 2019-01-02, though the repo was pushed 2026-08-19.

**differential-dataflow 0.25.1 and timely 0.31.0.** Both are alive
(differential pushed 2026-09-12, timely 2026-09-11; 3,004 and 3,644 stars). The
`iterate` operator establishes a fixed-point loop, so recursion is available,
and negation is expressible as `negate` + `concat` + `consolidate`. Nothing in
either crate stratifies a rule set or checks negation safety; that stays v8's
job either way. The cost is the output model: a differential collection is a
stream of `(data, time, diff)` triples whose emission order depends on worker
scheduling, and `iterate` "does not automatically insert consolidate", so
deterministic row order is work you add back. For a single-process compiler
fixpoint that runs to completion and then hands rows to the next phase, the
incremental machinery is paid for and not used.

**cozo 0.7.6 / mnestic 0.18.0 / retia-bin 0.2.0.** Cozo takes CozoScript as a
runtime string, which is exactly the rules-at-runtime shape, and the manual
states the safety rule for negation directly: "Recursion cannot occur in negated
positions (safety rule): `r[a] := not r[a]` is not allowed". The disqualifier is
weight and layer. Cozo 0.7.6 declares 44 required and 11 optional normal
dependencies, including `jieba-rs`, `rust-stemmers`, `ndarray`, `quadrature` and
`uuid`; its RocksDB backend crate `cozorocks` packages 4,674,283 bytes. Upstream
`cozodb/cozo` was last pushed 2024-12-04 with 49 open issues. `mnestic` is a
maintained fork (pushed 2026-09-08) with the same 44-dep core plus `oxiri`;
`retia-bin` is a Rust-only fork of `cozo-bin`, a binary, with no published
library crate. Adopting any of them means the compiler's macrotime and comptime
fixpoints run inside a transactional database and return rows through a query
result type, which contradicts "one evaluator, two phases, an effect sink per
round".

**egglog 3.0.0.** The runtime story is the best of any candidate here:
`EGraph::parse_and_run_program`, `parse_program`, `run_program`, `step_rules`,
`query` and `get_overall_run_report` are all public methods, and `seminaive:
bool` is a public field on `EGraph`. Dependency weight is 21 required crates,
most of them its own workspace members. The blocker is the fact language:
`egglog::ast::GenericFact` has exactly two variants, `Eq(Span, Expr, Expr)` and
`Fact(Expr)`. There is no negated literal, because egglog's semantics are
monotone over an e-graph with union-find congruence closure. A DL7 rule with a
negated body atom over a lower stratum has no encoding here, and forcing one
through `check`/`fail` commands would change the fixpoint's meaning.

**ddlog.** `vmware/differential-datalog` now redirects to
`vmware-archive/differential-datalog`, which the GitHub API reports as
`archived: true`, last push 2023-07-07, 1,501 stars. No crates.io release under
either name. It was a `.dl` -> Rust source-to-source compiler, not a library, so
even alive it would have been the wrong door.

**souffle.** The crates.io `souffle` crate is version 0.0.1, published
2019-07-28, packaged at 741 bytes, self-described "demo crate, wip", 8 recent
downloads. It is not a binding. The real Souffle (`souffle-lang/souffle`, pushed
2026-07-13, UPL-1.0, 1,162 stars) synthesizes a parallel C++ program from a
`.dl` specification; using it means shelling a C++ compiler per program, which
collides with the project's zero-shell-in-the-engine decision and with rules
arriving as runtime data.

**naga-datalog.** No crate by that name exists on crates.io. The `naga` crate
that does exist (30.0.1, 2026-08-22) is the wgpu shader translator and is
unrelated. Treat the name as a mis-recollection.

**datalog 0.0.2 and dlog 0.1.2.** `datalog` was last published 2017-04-07, has
20 recent downloads, and its own crates.io description opens with "INCOMPLETE!".
`dlog` 0.1.2 (2021-04-23) is "A command line utility to efficiently track where
you spend your time", a name collision with no Datalog content. Both are dead
ends, recorded so nobody re-checks them.

**datalog-core 0.1.0.** Published 2026-08-22 by legra-ai, 1 dependency
(`rustc-hash`), and its docs are explicit about scope: "This crate contains
rule-program data structures and pure validation only. It does not provide a
parser or CST for a Datalog source language, and it does not define an execution
runtime." It builds `Program` / `Rule` / `Atom` / `Literal` at runtime and does
safety validation, negation-cycle detection and stratification. That is the one
piece v8 would otherwise write by hand that this crate covers. Against it: 42
lifetime downloads, 0 GitHub stars, 0 open issues, one release, and an IR shape
v8 does not control. Stratification is roughly 100 lines of Tarjan over a
dependency graph; taking a strange crate's IR to avoid writing it is a bad
trade.

**flowlog (flowlog-build 0.4.0, flowlog-runtime 0.3.0, flowlog-parser 0.1.0).**
Genuinely new and genuinely active: `flowlog-rs/flowlog` was pushed 2026-09-11.
It compiles Datalog to differential-dataflow, has a stratifier stage, and
`flowlog-runtime` re-exports timely/differential-dataflow. The delivery model is
the disqualifier: the CLI compiles a `.dl` file, and library mode is
`flowlog-build`, a "Build-time FlowLog compiler" invoked from `build.rs`. There
is no documented API to load a program at runtime and read results back. 48
stars, 254 recent downloads on the build crate. Worth re-checking in a year if
they publish a runtime loader.

**purrdf-datalog 1.1.0.** On paper this matches the requirement list better than
anything else on crates.io: a `DlClause` IR built from data, a type-state
pipeline `Parsed -> Stratified -> Planned -> Executable` that makes an
unstratified program unrepresentable at the executor, a `seminaive` module
implementing "the stratified semi-naive fixpoint itself", ordered-map
arrangements, and a determinism claim that "identical input yields
byte-identical output, on every target" with "no map iteration order reaches an
output path". Seven dependencies. It is disqualified by arity: the docs state "A
`ClauseAtom` is `triple(?s, ?p, ?o, ?g)`: four ordinary `ClauseTerm` positions"
and "Every atom is an arity-4 quad, and the predicate is DATA". DL7 relations
are arbitrary-arity with typed columns. Encoding an N-column relation as N-1
quads would blow up row counts and destroy the per-column index design. Read its
`seminaive` module as a design reference, do not link it.

**minigraf 2.0.0.** "Zero-config, single-file, embedded graph database with
bi-temporal Datalog queries", published 2026-08-26, 822 recent downloads. Same
category error as cozo: a database with a query language, not an evaluator a
compiler drives round by round with a trace sink.

### 1.3 Compile-time-rules group

Explicitly disqualified for the compiler's own evaluator because the rule text
must exist in Rust source before `rustc` runs:

| crate | why it cannot take runtime rules |
|---|---|
| ascent | rules live inside the `ascent!` / `ascent_run!` proc-macro body |
| crepe | rules live inside the `crepe!` proc-macro body |
| flowlog | `flowlog-build` is a `build.rs` compiler; no runtime loader |
| souffle | synthesizes C++ from `.dl`, then that C++ is compiled |
| ddlog | source-to-source `.dl` -> Rust, and archived |

Same group, as targets for EMITTED DL7 programs (a separate decision, not
settled here): ascent is the strongest, because v8 would be generating Rust text
anyway and ascent already provides stratified negation, semi-naive rounds and
per-rule indices in the generated struct.

---

## 2. Lisp / S-expression readers

DL7 surface syntax, from `v7/tree-sitter-dl7/grammar.js`: parenthesized
expressions, a `string_literal` with backslash escapes, a `query_literal`
delimited by `{` `}` that may contain a quoted `}`, a `bare_token` matched as
`/[^\s();"{}]+/` and classified by the adapter, and a `line_comment` of `;` to
end of line. The grammar comment states the split: the grammar owns "character
recognition, parenthesized nesting, concrete syntax spans, and recoverable
syntax errors", and the adapter owns "bare-token classification, semantic
identities, variable sharing, literal decoding, and compiler diagnostics". The
`:` infix colon form is adapter-level, inside `bare_token`.

### 2.1 Candidate table

| crate | version | last release | license | spans? | error recovery? | fits DL7 lexical set? | dep weight | custom-reader cost on top | verdict |
|---|---|---|---|---|---|---|---|---|---|
| tree-sitter | 0.27.0 | 2026-08-30 | MIT | YES, byte + row/col per node | YES, `ERROR` / `MISSING` nodes | YES, the grammar already exists | 3 required (`regex`, `streaming-iterator`, `tree-sitter-language`) + `cc` at build | adapter only; grammar is written | RECOMMENDED |
| winnow | 1.0.4 | 2026-07-13 | MIT | YES, `LocatingSlice` + `Parser::with_span` | manual, `Recover` trait present | yes, hand-written | 0 required, 5 optional | full reader, roughly 250-400 lines | strongest fallback |
| chumsky | 0.13.0 | 2026-05-06 | MIT | YES, first-class spans | YES, first-class recovery | yes, hand-written | 3 required | full reader, roughly 200-300 lines | good, but development moved off GitHub |
| nom | 8.0.0 | 2025-01-26 | MIT | via `nom_locate` 5.0.0 (separate crate) | manual | yes, hand-written | 1 (`memchr`) + 1 for spans | full reader, roughly 250-400 lines | winnow supersedes it here |
| pest | 2.9.1 | 2026-09-05 | MIT OR Apache-2.0 | YES, `Span` / `Position` | error with line/col, no node-level recovery | yes, `.pest` grammar | 1 (`ucd-trie`) | a second grammar file to keep in sync with `grammar.js` | duplicates the grammar v7 already owns |
| logos | 0.16.1 | 2026-01-30 | MIT OR Apache-2.0 | YES, `Lexer::span()` | n/a, lexer only | tokens yes, nesting no | 0 required | lexer free, paren nesting and error recovery by hand | pairs with a hand reader, not a whole answer |
| lalrpop | 0.23.1 | 2026-03-11 | Apache-2.0 OR MIT | YES, `@L` / `@R` locations | LR error recovery is limited | yes, `.lalrpop` grammar | 13 required, at build time | a third grammar dialect; LR is overkill for s-exprs | no |
| lexpr | 0.2.7 | 2023-03-16 | MIT OR Apache-2.0 | YES, `Datum::span()`, `Position` | no | partly; Scheme/Elisp `Value` model, no `{...}` query literal | 3 (`itoa`, `lexpr-macros`, `ryu`) | fork or post-process the `Value` tree | no |
| serde-lexpr | 0.1.3 | 2023-03-16 | MIT OR Apache-2.0 | inherits lexpr | no | no, serde mapping is the wrong target | lexpr + serde | n/a | no ("ser-lexpr" is not a crate; this is the real name) |
| sexp | 1.1.4 | 2016-10-13 | MIT | line/column on `Error` only, none on atoms | no | no, `Atom` is S/I/F only, no keywords | 0 | rewrite | no |
| sexpr | 0.1.0 | 2016-10-12 | MIT | no | no | no | - | rewrite | dead, 9 recent downloads |

### 2.2 Per-candidate notes

**tree-sitter 0.27.0.** The grammar for DL7 already exists and is committed:
`v7/tree-sitter-dl7/grammar.js` (57 lines) plus a generated
`v7/tree-sitter-dl7/src/parser.c` (390 lines) and the three vendored headers
under `src/tree_sitter/`. Every `Node` exposes `start_byte`, `end_byte`,
`start_position`, `is_error`, `is_missing` and `has_error`, which covers spans
and recoverable syntax errors in one API. Dependency weight is 3 required
crates. The v8 cost is only the adapter (bare-token classification, the `:`
infix colon form, literal decoding), which is exactly the boundary the grammar's
header comment already draws. Choosing anything else means v7 and v8 disagree
about what DL7 parses, and the editor grammar drifts from the compiler.

**winnow 1.0.4.** Zero required dependencies, 256,996,016 recent downloads, last
released 2026-07-13. `LocatingSlice` wraps a stream with span tracking and
`Parser::span` / `Parser::with_span` attach spans to parsed tokens; the docs note
that byte-offset-to-line conversion is left to the caller (the `line-span` crate,
0.1.5, covers it). A `Recover` trait exists for error recovery. A DL7 reader here
is a hand-written recursive descent over five token shapes, call it 250-400
lines plus tests. This is the choice if the tree-sitter C dependency ever has to
go.

**chumsky 0.13.0.** Spans and error recovery are the crate's headline features
and 4,734,777 recent downloads back it. One caution worth recording: the GitHub
repo `zesterer/chumsky` reports `archived: true` with last push 2026-03-27, and
crates.io now lists the repository as `https://codeberg.org/zesterer/chumsky`.
Development moved rather than stopped, but a mirror that reads as archived is a
maintenance-signal hazard for anyone auditing the dependency later.

**nom 8.0.0.** Last released 2025-01-26, 152,878,082 recent downloads, one
dependency (`memchr`). Spans require `nom_locate` 5.0.0 (2025-02-03), a separate
crate. Same authors, same lineage as winnow; winnow folds span tracking into the
core and is more recently released, so nom loses on both counts for a new
project.

**pest 2.9.1.** Very much alive (released 2026-09-05, 64,663,232 recent
downloads, one dependency). It gives `Span` and `Position` and a readable
grammar file. The problem is duplication: DL7 would then have a `.pest` grammar
next to the existing `grammar.js`, and any syntax change must land in both or
the editor and the compiler diverge. `flowlog-parser` and `cozo` both use pest,
which is evidence it works for this class of language, not evidence v8 should
own two grammars.

**logos 0.16.1.** Zero required dependencies, 18,980,841 recent downloads,
`Lexer::span()` on every token. It solves the token layer of DL7 cleanly
(`bare_token`, `string_literal`, `line_comment`) but has nothing to say about
paren nesting, the `{...}` query literal's quote-aware scan, or error recovery.
It is a component of a hand-written reader, not a replacement for one.

**lalrpop 0.23.1.** 13 required build-time dependencies including `petgraph`,
`regex`, `sha3` and `string_cache`. LR(1) parser generation solves ambiguity
problems that a fully parenthesized Lisp does not have. It adds a third grammar
dialect to the project. No.

**lexpr 0.2.7 and serde-lexpr 0.1.3.** Both last released 2023-03-16; the repo
`rotty/lexpr-rs` was pushed 2026-08-28, so it is maintained but slow. lexpr is
the only S-expression crate in this list with real span support:
`lexpr::datum::Datum` exposes `span()`, `value()`, `list_iter()` and
`vector_iter()`, and `lexpr::parse` exposes `Position`, plus `Options` with
`KeywordSyntax`, `Brackets`, `StringSyntax`, `CharSyntax`, `NilSymbol` and
`TSymbol` knobs. What it cannot do is DL7: there is no `{...}` query literal, the
`Value` model is Scheme/Emacs-Lisp shaped, and the `:` infix colon form would
have to be recovered by re-scanning symbol text. Note for the record: the brief's
"ser-lexpr" is not a crates.io name; the serde companion is `serde-lexpr`.

**sexp 1.1.4.** Last published 2016-10-13, zero dependencies, 6,414 recent
downloads. Its `Atom` enum is documented as excluding floats and carries only
string/int/float-free cases; there are no keywords, no spans on atoms, and
location information appears only on the parse `Error`. Too small and a decade
stale.

**sexpr 0.1.0.** Published 2016-10-12, 2,595 lifetime downloads, 9 recent. Dead.

### 2.3 The tree-sitter ABI check

The specific question was whether the generated `parser.c` can be built with
`cc` in `build.rs` against the current `tree-sitter` crate. It can, and the
version numbers line up exactly.

| fact | value | source |
|---|---|---|
| generated parser ABI in this repo | `#define LANGUAGE_VERSION 15` at `v7/tree-sitter-dl7/src/parser.c:9` | local file |
| `tree_sitter::LANGUAGE_VERSION` | `15` | https://docs.rs/tree-sitter/latest/tree_sitter/constant.LANGUAGE_VERSION.html |
| `tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION` | `13` | https://docs.rs/tree-sitter/latest/tree_sitter/constant.MIN_COMPATIBLE_LANGUAGE_VERSION.html |
| upstream C header agrees | `TREE_SITTER_LANGUAGE_VERSION 15`, `TREE_SITTER_MIN_COMPATIBLE_LANGUAGE_VERSION 13` | https://github.com/tree-sitter/tree-sitter/blob/master/lib/include/tree_sitter/api.h |
| `tree-sitter` crate | 0.27.0, 2026-08-30, MIT, 3 required deps, 14,011,138 recent downloads | https://crates.io/crates/tree-sitter |
| `tree-sitter-language` crate | 0.1.8, 2026-08-30, MIT, exposes `LanguageFn::from_raw` / `into_raw` | https://docs.rs/tree-sitter-language/latest/tree_sitter_language/struct.LanguageFn.html |
| `cc` crate | 1.4.5, 2026-09-04, MIT OR Apache-2.0, 272,685,623 recent downloads | https://crates.io/crates/cc |
| grammar already vendors its headers | `src/tree_sitter/alloc.h`, `array.h`, `parser.h` are tracked in git | local `git ls-files` |
| grammar currently declares no Rust binding | `"bindings": { "c": true, ..., "rust": false }` in `v7/tree-sitter-dl7/tree-sitter.json` | local file |

Work implied: flip `bindings.rust` to `true` (or hand-write the 10-line
binding), add a `build.rs` that `cc::Build::new().file("src/parser.c")`, and
declare the extern as a `tree_sitter_language::LanguageFn`. `parser.c` is 390
lines with 9 states and 11 symbols, so compile time is not a concern.

---

## 3. Interning and index storage

### 3.1 Candidate table

| crate | version | last release | license | required deps | recent downloads | packaged size | what it buys | verdict |
|---|---|---|---|---|---|---|---|---|
| indexmap | 2.14.2 | 2026-09-05 | Apache-2.0 OR MIT | 2 (`equivalent`, `hashbrown`) | 349,153,749 | 103,014 B | insertion-ordered map, the deterministic-order requirement | BUY |
| hashbrown | 0.17.1 | 2026-05-09 | MIT OR Apache-2.0 | 0 | 646,560,309 | 155,512 B | raw-entry / entry APIs std's `HashMap` withholds | BUY, when raw entry is needed |
| string-interner | 0.20.0 | 2026-04-30 | MIT/Apache-2.0 | 1 (`hashbrown`) | 5,607,294 | 32,522 B | `Symbol` newtype interner, several backends | BUY (default pick) |
| lasso | 0.7.3 | 2024-08-19 | MIT OR Apache-2.0 | 1 (`hashbrown`) | 2,494,469 | 78,870 B | single- and multi-threaded interners, `Rodeo` / `ThreadedRodeo` | viable, but upstream is quiet |
| ustr | 1.1.0 | 2024-10-26 | BSD-2-Clause-Patent | 4 (`ahash`, `byteorder`, `lazy_static`, `parking_lot`) | 1,070,802 | 350,646 B | global never-freed interner, `Ustr` is `Copy` | no, global state and a non-standard license |
| slotmap | 1.1.1 | 2025-12-06 | Zlib | 0 | 25,766,789 | 61,862 B | generational arena keys for syntax nodes | BUY if node identity needs generations |
| roaring | 0.11.5 | 2026-08-12 | MIT OR Apache-2.0 | 0 | 10,759,271 | 135,118 B | compressed bitsets for row-id index posting lists | BUY only once a measurement asks for it |

### 3.2 Per-candidate notes

**indexmap 2.14.2.** 349,153,749 recent downloads, two dependencies, released
2026-09-05. Deterministic row order is a stated v8 requirement, and an
insertion-ordered map delivers it without a sort per round. Writing one that is
as fast means reimplementing the entries-vector-plus-index-table design plus
`swap_remove` / `shift_remove` semantics. No argument for building.

**hashbrown 0.17.1.** Zero required dependencies, 646,560,309 recent downloads.
It is already the allocation behind `std::collections::HashMap`, so the only
reason to depend on it directly is the raw-entry and entry-ref APIs std does not
export, which matter for "look up by hash, insert if missing" in an interner or
an index. Note that `ascent`, `egglog`, `string-interner`, `lasso`, `chumsky`
and `purrdf-datalog` all already depend on it, so it is unlikely to be a new
node in the graph.

**string-interner 0.20.0.** Released 2026-04-30, one dependency (`hashbrown`),
32,522 bytes packaged, 5,607,294 recent downloads, repo pushed 2026-05-06. It
gives a `Symbol` newtype and swappable backends (bucket, string, buffer), which
is precisely the interning surface a Datalog term table needs. The default pick.

**lasso 0.7.3.** One dependency, 2,494,469 recent downloads, and the only
candidate with a first-class threaded interner (`ThreadedRodeo`). Against it:
last release 2024-08-19, repo last pushed the same day, 14 open issues, and the
v8 evaluator described in the brief is single-process. Pick it only if
multi-threaded interning becomes a requirement.

**ustr 1.1.0.** 1,070,802 recent downloads and a genuinely fast `Copy` symbol
type, but the interner is a process-global static that never frees, and the
license is `BSD-2-Clause-Patent`, which GitHub cannot classify
(`license.spdx_id: NOASSERTION`) and which does not match this repo's `MIT OR
Apache-2.0`. It also pulls 4 required dependencies, the most in this group. Two
compiler phases sharing one global string table across test processes is the
kind of hidden coupling that makes a test suite order-dependent.

**slotmap 1.1.1.** Zero dependencies, 25,766,789 recent downloads, repo pushed
2026-09-04. Generational keys are the right shape for macrotime's syntax-graph
nodes, where a rewrite retires a node and a stale key must not silently resolve
to its replacement. If v8's syntax graph is append-only within a compile, a plain
`Vec` index is enough and slotmap is unnecessary weight; decide from the rewrite
design, not from this document.

**roaring 0.11.5.** Zero required dependencies, 10,759,271 recent downloads,
released 2026-08-12. Compressed bitsets pay off for per-column index posting
lists once relations reach tens of thousands of rows; below that a sorted
`Vec<u32>` wins on constant factors. The v7 receipt to size this against is the
cold-checkpoint 15,542 compiler rows in `v7/README.md`. Add it after a
measurement, not before.

**The ownership question.** Buy is the obvious answer for all of question 3, and
the numbers say why. The combined required-dependency count for
`indexmap + string-interner + slotmap` is 3 distinct crates (`equivalent`,
`hashbrown`, and hashbrown again, shared), packaged at 103,014 + 32,522 + 61,862
bytes. All three are at or above 5.6 million recent downloads, all three were
released within the last ten months, and all three are `MIT OR Apache-2.0` or
Zlib, compatible with this repo's `MIT OR Apache-2.0`. None of them is a
"common-shaped problem" v8 has a special requirement in: an interner maps bytes
to a `u32` and back, and there is no DL7-specific twist on that. Writing any of
them costs days and buys a slower, less tested version of a crate that costs one
`Cargo.toml` line.

---

## 4. Recommendation per question

| question | recommendation | runner-up | why the runner-up lost |
|---|---|---|---|
| 1. Datalog / fixpoint engine | BUILD the v8 evaluator, on `indexmap` + `hashbrown` for storage; read `purrdf-datalog`'s `seminaive` module and `datalog-core`'s stratifier as design references | datafrog 2.0.1 | It has zero dependencies, sorted-distinct `Relation` for free order, and the recent/stable semi-naive split, but it supplies no negation, no stratifier, no comparison builtins and no per-round hook, and its join closures are Rust code, so rules-as-data still needs the plan interpreter written. Its last release was 2019-01-02. |
| 2. Lisp / S-expression reader | BUY tree-sitter 0.27.0 and keep `v7/tree-sitter-dl7`; write only the v8 adapter | winnow 1.0.4 | Zero required dependencies, built-in `LocatingSlice` spans, and released 2026-07-13, but it means a second DL7 grammar that must be kept byte-for-byte in agreement with `grammar.js`, and it gives up tree-sitter's `ERROR` / `MISSING` recovery nodes that the grammar header already assigns to the parser layer. |
| 3. Interning and index storage | BUY: `string-interner` 0.20.0, `indexmap` 2.14.2, `hashbrown` 0.17.1; add `slotmap` 1.1.1 only if the syntax graph needs generational keys, `roaring` 0.11.5 only after a row-count measurement | lasso 0.7.3 | Same one-dependency footprint and a real threaded interner, but the last release was 2024-08-19 with no pushes since, and v8's evaluator is single-process, so `ThreadedRodeo` buys nothing today. `ustr` lost harder: process-global never-freed state and a `BSD-2-Clause-Patent` license GitHub cannot classify. |

---

## 5. Source index

Registry and repository facts, all read 2026-09-12.

| subject | URL |
|---|---|
| ascent | https://crates.io/crates/ascent , https://github.com/s-arash/ascent , https://github.com/s-arash/ascent/blob/master/README.MD , https://github.com/s-arash/ascent/blob/master/ascent_macro/src/ascent_mir.rs , https://github.com/s-arash/ascent/blob/master/ascent_macro/src/ascent_codegen.rs |
| crepe | https://crates.io/crates/crepe , https://github.com/ekzhang/crepe/blob/main/README.md |
| datafrog | https://crates.io/crates/datafrog , https://github.com/rust-lang/datafrog , https://docs.rs/datafrog/latest/datafrog/struct.Relation.html |
| differential-dataflow | https://crates.io/crates/differential-dataflow , https://docs.rs/differential-dataflow/latest/differential_dataflow/operators/iterate/index.html |
| timely | https://crates.io/crates/timely , https://github.com/TimelyDataflow/timely-dataflow |
| cozo | https://crates.io/crates/cozo , https://github.com/cozodb/cozo , https://docs.cozodb.org/en/latest/queries.html , https://crates.io/crates/cozorocks |
| mnestic | https://crates.io/crates/mnestic , https://github.com/shuruheel/mnestic |
| retia | https://crates.io/crates/retia-bin |
| egglog | https://crates.io/crates/egglog , https://docs.rs/egglog/latest/egglog/struct.EGraph.html , https://docs.rs/egglog/latest/egglog/ast/enum.GenericFact.html , https://github.com/egraphs-good/egglog |
| ddlog | https://github.com/vmware-archive/differential-datalog |
| souffle | https://crates.io/crates/souffle , https://github.com/souffle-lang/souffle |
| naga (name check) | https://crates.io/crates/naga |
| datalog | https://crates.io/crates/datalog |
| dlog | https://crates.io/crates/dlog |
| datalog-core | https://crates.io/crates/datalog-core , https://docs.rs/datalog-core/latest/datalog_core/ , https://github.com/legra-ai/datalog-core |
| flowlog | https://crates.io/crates/flowlog-build , https://crates.io/crates/flowlog-runtime , https://crates.io/crates/flowlog-parser , https://github.com/flowlog-rs/flowlog |
| purrdf-datalog | https://crates.io/crates/purrdf-datalog , https://docs.rs/purrdf-datalog/latest/purrdf_datalog/ , https://docs.rs/purrdf-datalog/latest/purrdf_datalog/clause/index.html |
| minigraf | https://crates.io/crates/minigraf |
| lexpr / serde-lexpr | https://crates.io/crates/lexpr , https://crates.io/crates/serde-lexpr , https://docs.rs/lexpr/latest/lexpr/datum/struct.Datum.html , https://docs.rs/lexpr/latest/lexpr/parse/index.html , https://github.com/rotty/lexpr-rs |
| sexp / sexpr | https://crates.io/crates/sexp , https://docs.rs/sexp/latest/sexp/ , https://crates.io/crates/sexpr |
| nom / nom_locate | https://crates.io/crates/nom , https://crates.io/crates/nom_locate |
| winnow | https://crates.io/crates/winnow , https://docs.rs/winnow/latest/winnow/stream/struct.LocatingSlice.html |
| chumsky | https://crates.io/crates/chumsky , https://codeberg.org/zesterer/chumsky , https://github.com/zesterer/chumsky |
| pest | https://crates.io/crates/pest |
| logos | https://crates.io/crates/logos |
| lalrpop | https://crates.io/crates/lalrpop |
| tree-sitter | https://crates.io/crates/tree-sitter , https://docs.rs/tree-sitter/latest/tree_sitter/constant.LANGUAGE_VERSION.html , https://docs.rs/tree-sitter/latest/tree_sitter/constant.MIN_COMPATIBLE_LANGUAGE_VERSION.html , https://docs.rs/tree-sitter/latest/tree_sitter/struct.Node.html , https://github.com/tree-sitter/tree-sitter/blob/master/lib/include/tree_sitter/api.h |
| tree-sitter-language | https://crates.io/crates/tree-sitter-language , https://docs.rs/tree-sitter-language/latest/tree_sitter_language/struct.LanguageFn.html |
| cc | https://crates.io/crates/cc |
| string-interner | https://crates.io/crates/string-interner , https://github.com/robbepop/string-interner |
| lasso | https://crates.io/crates/lasso , https://github.com/Kixiron/lasso |
| ustr | https://crates.io/crates/ustr , https://github.com/anderslanglands/ustr |
| indexmap | https://crates.io/crates/indexmap |
| hashbrown | https://crates.io/crates/hashbrown |
| slotmap | https://crates.io/crates/slotmap |
| roaring | https://crates.io/crates/roaring |
| line-span | https://crates.io/crates/line-span |

Version, release-date, license and dependency-count rows were read from the
crates.io JSON API (`/api/v1/crates/<name>` and
`/api/v1/crates/<name>/<version>/dependencies`); archived flags, last-push dates,
star counts and open-issue counts from the GitHub REST API
(`/repos/<owner>/<name>`).
