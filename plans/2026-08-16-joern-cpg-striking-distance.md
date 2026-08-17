# Joern striking distance: CPG parity for the extract planes

## TOC
1. The claim
2. Edge-color distance table
3. The node dictionary
4. Generic CFG: the kind_role design
5. CDG and walks as derivations
6. Build-vs-buy candidates
7. Forks for Chris

## 1. The claim

A Joern CPG is one node table plus edge tables of different colors sharing its key.
Five of Joern's seven edge colors already exist in `sprefa-extract` families. The
missing two (CFG, CDG) need one per-lang fact table and two generic derivations.
Chat analysis 2026-08-16, verified against `v6/sprefa-extract/src/types.rs`.

## 2. Edge-color distance table

| Joern edge | ours | status |
|---|---|---|
| AST | `CstF` child edges | BUILT, every lang incl. astgrep fallback |
| CALL / ARGUMENT / REF | `CallF` sites, defs, args (`DfArg`, `DfParam`) | BUILT |
| REACHING_DEF | `DfF` `Direct` | BUILT intra-procedural |
| EVAL_TYPE | `TypeF` (declared types only) | PARTIAL |
| their symbol glue | SCIP occurrences, span-joined | BUILT |
| CFG | none | MISSING |
| CDG | none | MISSING |

Joern's front-ends are parse-only with type recovery heuristics: the same tier as
the four walkers here, which is validation of the tier, never a reason to adopt
the JVM stack.

## 3. The node dictionary

```sql
node(id INTEGER PRIMARY KEY, file, span_start, span_end, kind)  -- ONE rel, span-keyed
-- every plane's edges are FK pairs into it; SCIP rows decorate by span-overlap join
```

Surrogate integer ids per the sql-relational-design law. Node identity today is
`(family, span, kind)` (`types.rs:652`); the dictionary is that identity given one
id so edge colors can share endpoints.

## 4. Generic CFG: the kind_role design

CFG construction needs one per-language fact: which CST kinds play which control
role. Rows, never a walker:

```
kind_role("rust", "if_expression",     "branch").
kind_role("rust", "while_expression",  "loop").
kind_role("rust", "return_expression", "jump").
kind_role("go",   "if_statement",      "branch").

cfg_edge(A, B) :- cst_next_sibling(A, B), not kind_role(_, kind_of(A), "jump").
cfg_edge(Cond, Then) :- kind_role(_, kind_of(N), "branch"),
                        cst_child(N, Cond, 0), cst_child(N, Then, 1).
```

rx lowering: `cst$.pipe(withLatestFrom(roleTable$), map(buildCfgEdges))`, a pure
per-file operator; an edited file recomputes only its own cfg rows. Works for
every lang with a grammar, including cst-only fallback langs with no type walker.

## 5. CDG and walks as derivations

CDG is post-dominance over `cfg_edge`, zero per-language work:

```
postdom(N, D)   % every path N -> exit passes D: recursive rel, meet over successors
cdg_edge(B, S)  % the post-dominance frontier of branch B
```

rx lowering: `cfg$.pipe(map(fn => expandUntilFixed(postdomStep)))`, per-function,
bounded, the iteration flavor of fixed point (the engine tick's own kind).

Taint and every "valid walk" query is a recursive rel over the colored edges:

```
reaches(Src, Dst) :- ddg_edge(Src, Dst).
reaches(Src, Dst) :- reaches(Src, Mid), ddg_edge(Mid, Dst), not sanitizer(Mid).
tainted(Sink)     :- source(Src), reaches(Src, Sink), sink(Sink).
```

Call-return matching (a walk into a call must exit via the matching return) is
CFL-reachability: label call edges by site, index the walk rel on the site.
Joern hand-writes these as Scala traversals; dl6 states them as rules.

## 6. Build-vs-buy candidates

| candidate | what it offers | verdict shape |
|---|---|---|
| tree-sitter-graph (GitHub) | declarative CST -> graph rules per lang, the kind_role idea as an existing DSL | price before writing kind_role by hand |
| Joern CPG protobuf | published schema; import = coverage for langs never hand-walked, export = run Joern's own queries as a judge | price an importer next to the SCIP importer |
| Joern itself (JVM) | the queries, resident graph | wrong base: batch, no incremental, Scala DSL |

## 7. Forks for Chris

| # | fork | why it is real |
|---|---|---|
| 1 | DECIDED (user 2026-08-16): CfgF is a new family | one drawer per question, consistent with the four existing families |
| 2 | DECIDED (user 2026-08-16): Joern's 34 edge kinds live as a reference-only prolog enumeration (`v6/prolog/cpg_edge_vocab.pl`, lane in flight), never consulted; rel naming stays ours | research: the vocabulary is 34 kinds, not 7 (`plans/2026-08-16-cpg-spec-research.REPORT.md` sec 1a) |
| 3 | SETTLED by research: hand-authored kind_role rows; tree-sitter-graph dormant, steal the stanza-per-kind idea only | REPORT sec 2; kotlin needs a leading-keyword read, name table alone cannot split jump/exit (REPORT sec 4) |
| 4 | where CDG computes: engine-side dl6 rules vs a rust builtin pass | postdom meet-over-successors needs stratified aggregation; check the engine has it before ruling dl6-side |
