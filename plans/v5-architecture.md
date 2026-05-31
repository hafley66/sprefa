# v5 Architecture — Glean++ over SQLite

The standing map for v5. Captures the design decisions from the 2026-05-31 heavy
planning session so they are not relitigated. v5 grew a Glean shape by accreting
datalog fast; this names the core/module/effect/sink layers deliberately, decides
the contested forks (Prolog? DD?), and fixes the memory budget.

## Thesis

**Glean++** = Glean's typed-predicate fact model **+ a pluggable diet-indexer surface**
(ast-grep / json / re / glob — "diet static typing via crude syntax reading")
**+ a compiler-backed importer tier** (SCIP) **+ an effect edge** (sh/http/write)
**+ a reconcile loop** (k8s apply-state). Glean alone is pure and has none of the
last three; that is the "++".

## Four decisions (settled, with reasons)

### 1. Datalog + effects, NOT Prolog
What you reach for with `sh`/docker-cache/k8s is not stronger logic — it is three
orthogonal axes. Prolog's unification/backtracking/function-symbols buy none of them.

| Want | Actually is | Status |
|---|---|---|
| `sh`, http, write | an **effect axis** (pure core = WHAT; interpreter = DO) | missing; v4 had `effect_runtime` |
| docker-like caching | **content-addressed memo** (hash the input) | have (digest-skip; ref-spine content-ids make it free) |
| k8s apply-state | the **reconcile loop** (.dl = desired state, tick = reconcile, SQLite = etcd) | have (this IS the tick) |

Datalog's one real gap vs Prolog is **function symbols** (synthesize/recurse structured
values in heads). Code analysis over spans/files/symbols rarely needs it; `closure()`
covers transitive recursion, `${}` covers value construction. When a concrete tree/list
recursion appears, add ONE built-in predicate — not a paradigm. **No Prolog.**

### 2. Glean/Salsa/Soufflé blend over SQLite, NOT Differential Dataflow
DD's incremental *semantics* are good; its *storage* is the RAM problem. Arrangements
(indexed multi-versioned `(data,time,diff)`) live **in RAM** + history until compaction.
On a single board with no swap that kills you. Tell: **Materialize is DD-over-SQL and had
to bolt on a disk `persist` layer** (VLDB 2023) for exactly this. Do not adopt in-RAM DD.

v5's actual (better-fit) position, named:
- **Glean** disk-backed fact model (Glean is RocksDB-backed, NOT DD-in-RAM — the framing reinforces this) →
- **Salsa** red-green early-cutoff (digest-skip IS red-green: recompute → output digest unchanged → re-green, cut downstream) →
- **Soufflé** bottom-up fixpoint (SQL `INSERT`-until-zero) →
- **− DD's in-RAM arrangements.**

| DD (trap) | v5 |
|---|---|
| arrangements pinned in RAM | relations in SQLite, OS-paged |
| `(data,time,diff)` + retained history | per-relation content-hash digest (Axis A) + `_prov` |
| frontier recompute | tick + affected-derived dependency gating |
| compaction to bound history | no history; content-dedup so revs don't multiply storage |

Decided already in memory: digest-skip at RELATION grain, NOT row-granular `MEMO_DEPS`
("the RAM trap"). This is the theory behind that call.

### 3. SCIP as an importer tier (not just an oracle)
If rust-analyzer is already running, **import its incremental `index.scip`** as a
compiler-backed indexer (Glean-style). Loads `scip_def`/`scip_ref`/`scip_edge` keyed by
the same `ref` coordinate; diet `module_edge` UNIONs with `scip_edge` where present (the
composition already proven). Two tiers — diet syntactic (90%) and compiler-resolved (10%)
— in one relational model, picked per query. Incremental because RA maintains it.

### 4. Peak-RSS invariant
**Peak RSS ≈ max(parse working set, condensation adjacency, SQLite cache) — none is the corpus.**
Because the trace is SQLite, RAM is a per-tick working set, not the state. Steady state is
fine (warm 61 MB on 36k-file kernel); peak is a cold-build concern:

| In-RAM spike | Lever |
|---|---|
| rayon parse holds N contents + ASTs | cap pool to cores; chunk collect-then-insert (Db seam chunks writes; cap in-flight Vec) |
| `load_edges` adjacency for SCC | RAM/speed knob: Rust condensation (fast, spike) vs SQL recursive-CTE view (paged, bounded) |
| query result | streamed already; seeded reach bounded by reachable set |
| SQLite cache | `PRAGMA cache_size` = the budget; SQLite pages the rest |

Content-addressing also compresses history: a file unchanged across 200 revs = ONE content
row + 200 cheap path rows. Closure-over-time reads the queried slice from SQLite (paged);
historical closures are not all resident.

## Layers

```
L5  Sinks / frontends      lsp · cli · http  — ONE relation, many renderers (v4's trio: tiny + reliable)
L4  Effect interpreter     sh · http · write — ops yield Effect; per-tick batch + content-cache (port v4 effect_runtime SHAPE, not the DD reactive graph)
────────────────────────── pure logic below ──────────────────────────
L3  Modules                module-graph (have) · type-graph · call-graph · entity-graph · scip-import
                           each = {typed predicates} + {an indexer} + {canned library rules}
L2  Engine                 relations + rules + closure/stratify + Db seam (have)
L1' Importers (real 10%)   SCIP ingest · cargo_metadata · tsconfig — compiler-resolved facts
L1  Matchers (diet 90%)    re · glob · json · ast-grep · ast-grep-yaml — pluggable Matcher trait (port v4 Dsl/Compiled/CaptureRow)
L0  Coordinate/type core   repo/rev/content/file + ref/strings — relational+type model; diet typing via crude syntax
```

## Core vs module boundary

- **Core** = L0–L2 + L4–L5: coordinate/type model, matcher+importer registries, datalog
  engine + Db seam, effect interpreter, sink trio.
- **A module** = L3: declares typed predicates AND ships an indexer AND optionally canned
  rules. "custom built-in predicates OR rules" = **both**. `module-graph` already is this
  exactly (`module_edge` predicate + `ModuleResolver` indexer). The work is to GENERALIZE
  the seam (ledger task B) so type-graph / call-graph / scip plug in identically rather than
  each being a bespoke `refresh_*` fn.

## Recover from v4 (mapped)

| v4 piece | Lands | Note |
|---|---|---|
| `Dsl`/`Compiled`/`CaptureRow{Span\|Literal}`/`CaptureSink` (cst/dsl.rs) | L1 | registry-free; host maps name→factory. `Span`→a `ref` coordinate, `Literal`→synthesized typed value. |
| 7 DSLs: re, glob, json, ast, sql, sql_where, markdown (cst/dsls/) | L1 inventory | re/glob tree-sitter-backed; json hand-rolled; ast borrowed engine; 3 LSP-only |
| `effect_runtime` (Effect + interpreter, `sh` op) | L4 | take the SHAPE (pure op → Effect → batched interpreter), leave the DD reactive graph + support-multiplicity behind |
| lsp/cli/http trio + `DslBodyLsp` (cst/lsp/providers.rs, ~16 methods) | L5 | v5 has cli+lsp as "one relation, many sinks"; http = third renderer. HEED the host-LSP-trait debt (6 FATAL design review) — recover carefully |
| `Injected`/`locate`/`Path`/`PathItem` (cst/locate.rs, path.rs) | L1, optional | DSL-in-DSL nesting; only if ast-inside-host-language is needed |

DO NOT port: `FactStore`/`runtime_graph`/`Memo`/support-multiplicity (the DD machinery to
exorcise per the no-DD-vocabulary stance). Port coordinate types + matcher trait + effect
SHAPE + sink pattern only.

## Capture → ref → type (the L1↔L0 bridge)

A matcher emits `CaptureRow{name, kind: Span|Literal}`. `Span` → a `ref(string_id, file, lo, hi)`
row; `Literal` → a synthesized value. The capture's declared column TYPE (Text/Int/Path/File/Dir)
is the diet static type: a `file`-typed capture is checked against the file set; a `path`-typed
value stores a `PathId`. So the matcher layer feeds the ref-spine directly and "it has types"
falls out of the column declarations.

## Task ledger (re-slotted under the layers)

Dup-avoiding order for the type-graph module: **B → E → A**. Ref-spine **C** is separate
(orthogonal, deferrable). Detail + file:line in `/CLAUDE.md`.

- **B** (done): generalized built-in refresh → one `refresh_rel`; the module/matcher registration seam.
- **E** (done): `type_edge` syn extractor — self-hosted type graph; rides B; deterministic tokenless type map.
- **A** (done): migrated remaining obvious N+1 write loops onto `Db::insert_rows`.
- **C** (L0, L): ref-spine Stage 2 — `_strings` interner + `ref` + content-derived ids; unlocks refactor + kills string dup.
- **D** (done): parallel `refresh_module_rels`, rev-aware module relations, and
  `--changed` incremental refresh. WORK content edits refresh touched module sources;
  path-set/manifest changes fall back to the WORK rev, with legacy edges rebuilt
  from rev-aware rows.
- Effect edge (L4): minimal `sh`/`http`/`write` ops + content-cache; reconcile loop already exists.
- Sinks (L5): http renderer over relations (third frontend).
- SCIP importer (L1') (done): ingest existing `index.scip` from `SPREFA_SCIP_INDEX`
  or repo root into `scip_def`/`scip_ref`/`scip_edge`.
- Auto-refactor (rides C): specifier spans + port `rewrite_use_path` + `edit(ref_id,new_str)` sink.

## Done (this arc)
Module graph (all Rust+TS levers ✓, SCIP oracle precision 1.00, broken-import linter),
Db seam (plural-only chokepoint + loud N+1 counter), shared refresh seam,
`type_edge(from,to,kind)`, remaining obvious N+1 write-loop paydown,
rev-aware module graph relations, parallel module extraction, `crate_edge`, Cargo rename.
`--changed` module refresh now updates touched WORK module sources without reparsing
other revs, and falls back to WORK-rev refresh when the file set or module manifests move.
SCIP import now loads an existing compiler index into `scip_def`/`scip_ref`/`scip_edge`.
Branch `codex/v5-refresh-type-edge`.
