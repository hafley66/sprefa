# CST/AST querying — rulings from the 2026-08-02 duel bracket

User-ratified 2026-08-02 ("agreed on all accounts"). Evidence: four DUEL docs +
two lane reports banked in `plans/duels-2026-08-02/` (worktrees disposable).
Every load-bearing citation below was coordinator-verified against the code.

## Rulings

1. **Lowering = STRING** (unanimous, flash + kimi on identical prompts).
   S-expr sugar parses by DCG into the existing `ts_query/1` term vocabulary;
   `compile_ts_query`/`ts_pattern_text` (1_host_expand.pl:414-478) emit query
   text; unsupported shapes refuse via `unmapped_feature`. Matching is bought
   (tree-sitter/ast-grep), never rebuilt as datalog joins. Missing emitters:
   anchors, negation, sg-metavariables — small branches or deliberate defers.
2. **Extraction demand = LAZY** (unanimous). Demand rows
   `cst_need(path, content_hash, lang)` derived from query goals fire the
   `extract` HostDef. Eager is the same code path with a `files(P)`-planted
   demand-set (flash's dissolve), so no second mechanism exists. Compact
   type/call/df planes keep their current eager ingest (kimi's boundary).
3. **Caching = engine-side, in effect_cache.** Request cols widen to
   (path, content_hash, lang, grammar_hash, query_text). Transactional with
   the tick, visible to the effect trail. Host stays stateless per its "No DB,
   no async" charter; per-invocation parse-tree reuse (v5 AstTreeCache
   precedent, eval.rs:1047) stays host-side. One-day host-side expression: a
   content-addressed cache dir keyed by the SAME tuple — only storage moves.
4. **sprefa-extract is the matching executor** — the CPU workhorse; the engine
   is runtime (demand + cache + joins + splice). ast-grep-core/-language 0.38
   already linked (Cargo.toml:18-19); rayon-parallel by charter. The v6 `sg`
   CLI host becomes a compatibility door. Kimi independently reached this:
   the missing phase-2 runner (`unsupported_host_execution_phase_2
   (tree_sitter_query)`, SYNTAX.md:330) should be a thin tree-sitter query
   wrapper in sprefa-extract, or a port of v5 `run_ts`.

## Consensus defaults carried from the first flash/kimi planning round

- Schema: generic `node` + `child` (+ ordinal, field), typed views derived;
  rel-per-kind storage cut. v5 nested-set spans (README.md:557) make
  inside/has an indexed interval-containment join, no recursion.
- Span columns stay flat start/end ints; the column_type_wrapper refusal
  stands until struct-as-rows lands generally.
- Grammar hash salts the effect digest (ruling 3 covers it).
- Rewrites = captures feeding splice rows through the staged-writes path; no
  ast-grep fix: templates.
- Quantified captures explode to rows+ordinal; group_concat/json_group_array
  (landed, ARCH ordered_aggregate_arc) aggregate when needed.
- Still split, needs a ruling: trivia/comments as separate rel (flash) vs
  CST-native named nodes with doc-formats separate (kimi). A14 lives here.

## Known caveat (kimi steelman, user-acked "fair")

STRING cannot push dynamic selectivity into the matcher: a 10-row
`deprecated(name)` filter still materializes every `identifier` capture before
the join. Falsifying lab spec in duels-2026-08-02/duel-a-kimi.md (10^5
identifiers / 10 hot, 10x wall + 1000x intermediate-rows criteria). If it
fails, the answer is a hybrid that compiles filter-bound patterns differently,
not a wholesale flip.

## Unbuilt queue implied by the rulings

- Phase-2 tree-sitter runner in sprefa-extract (port run_ts or wrap
  ast-grep-core's tree-sitter access); wire the conformance fixture host
  (2_hosts_wiring.pl:200-242) to it.
- S-expr sugar DCG in parse_dl.pl; anchor/negation emitter branches or named
  refusals.
- cst_need demand planting in the planner; grammar_hash into request cols.
- Lab B (eager-vs-lazy on linux corpus): both arm designs banked; needs a
  kernel checkout. Lab A (in-process library vs CLI spawn throughput on
  linux-sim via extract-ab lineage) is the payoff measurement for ruling 4.
- Effect-trail arc (prior sitting): stage/apply promotion, trail rows extend
  PerfTrace seam, disk digest into effect_cache response side.

## Model scorecard (for the opencode-orchestration skill's next edit)

flash 0731: 4/4 lanes brief-faithful; ARCH reconcile surgical (gate-verified,
hook side-effect correctly attributed); one novel design move
(eager-as-demand-set). The "never give flash judgment" doctrine over-rotates:
with grounding-file lists and falsification-shaped prompts it produced
citation-heavy design work. Its residual weakness is anchor sloppiness (cites
the pointing row, not the row where numbers live).
kimi-for-coding: 3/3 lanes; best citation precision (spot-checks 4/4 exact);
found run_ts, registry row, SYNTAX gap, and the pushdown steelman nobody else
saw. Slower wall-clock than flash on identical prompts.
