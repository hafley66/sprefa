# SCIP atlas: where dl is in diet-scip vs real-scip, and what's next

Snapshot 2026-07-11 (post perf arc + engine split). "Diet scip" = the
syntactic pseudo-scip tier (TypeLang extractors + name/import resolution);
"real scip" = ingesting compiler-backed index.scip.

## Where we ARE

### Real-scip ingestion (solid)
- rels: `scip_def` / `scip_ref` / `scip_edge` (repo-column'd, per-index origin
  threading), `scip_occurrence` (0-based spans + roles), `scip_binding`
  (aliased local names).
- Resolution: occurrence-level, position-FIRST (`ScipOccIndex` — same-name
  same-line refuses, name-map fallback with conflict refusal); SCIP preferred
  over syntactic when present; `pre_extract` hook loads the index BEFORE
  extraction (fresh-db tick 1 is index-aware).
- Demand: `scip_want(repo)` lazily runs installed indexers per root, merged
  load so cross-repo refs resolve; `SPREFA_SCIP_INDEX` override; `dl index`
  artifact.
- Honesty: oracle parity harness per language, site-level keying. Measured:
  with-scip rust 33% (self 77.9% syntactic), go 93.3%, python 79.3%,
  precision 0.97-1.00 everywhere.

### Diet-scip tier (solid, measured)
- 6 TypeLangs (Rust syn, TS/JS oxc, Kotlin/Go/Python tree-sitter) behind one
  registry; entities + edges + sigs + df lift + docs from ONE parse.
- Name resolution: repo-scoped buckets + import narrowing + module_binding
  alias hop; honest-bare on ambiguity (fails toward exclusion, which is why
  precision stays ~1.0).
- Known ceiling: trait/interface-heavy corpora resolve bare (otel-rust 14%
  index-free) — dynamic dispatch is invisible to syntax.

## Known gaps (ledgered, unfixed)

1. **Daemon scip staleness**: index.scip is gitignored; watchgate drops the
   event; a rebuilt index sits stale until an unrelated full tick. Fix:
   watchgate allowlist (like .git refs) + `dl index` pokes the daemon. (S)
   <!-- todo(bug): watchgate allowlist for index.scip; dl index pokes daemon -->
2. **Type resolver stays name-level**: TypeEdge/type_sig refs carry no source
   position, so occurrence-level resolution only helps calls. Fix: thread
   spans through TypeEdge (M, typegraph-wide).
   <!-- todo(feature): positions on type refs so ScipOccIndex covers type_link -->
3. **SCIP at WORK only**: rev twins (type_link_rev etc.) resolve syntactically
   at committed revs. Fix shape: `scip_want(repo, rev)` + index-at-rev via
   worktree checkout (L — decide if any consumer needs it first).
4. CORRECTED (2026-07-11 audit-by-collision): `scip_impl(impl, iface)` IS
   already ingested (src/rels/scip.rs, used by the flow_*_dispatch examples) —
   the original claim here was stale and cost a redundant codex item that
   collided with the shipped decl. REAL residual: the existing scip_impl
   lacks the repo column (cross-repo threading like scip_def) and
   is_type_definition is still dropped (no scip_typedef rel).
   <!-- todo(feature): repo column on scip_impl + scip_typedef from is_type_definition -->
5. Kotlin parity unmeasured (needs a JDK box, harness runtime-skips).

## Operator/rel ideas (ranked)

1. **`scip_impl(sym, iface_sym)` + `scip_typedef(sym, type_sym)`** from
   relationships (gap 4). Unlocks: real dynamic-dispatch call edges
   (`call_edge` through an interface), `implementors(iface)` blast radius,
   retiring the hand-built py_interface_dispatch pattern. (M, pure importer)
2. **`at(path, line, col, sym)` builtin rel**: the `dl what` anchor machinery
   as a queryable rel — "what symbol is under this coordinate" joins with
   everything (hover rails, flowmarks, editor round-trips). (S, machinery exists)
3. **`sym_pkg(sym, package, version)`**: SCIP symbols encode package+version —
   parse them out and the pin-skew arc joins compiler truth against manifest
   pins (cross-repo version drift at SYMBOL granularity). (S-M)
4. **`scip_doc(sym, markdown)`**: symbol documentation from the index;
   cross-check rail against doc_comment (docs drift between source and what
   the compiler publishes). (S)
5. **Rev-parameterized demand** `scip_want(repo, rev)` — only if a diff
   consumer materializes (gap 3).

## dl language-feature ideas (types + async done)

- Body-level pure-fn bind + concat: DESIGNED, sign-off pending
  (plans/2026-07-11-string-ergonomics-design.md).
- `argmax`/`argmin` as first-class aggregates (the per-message negation-argmax
  pattern is reimplemented in 4+ programs; one aggregate kills the idiom's
  boilerplate and its off-by-one traps).
  <!-- todo(feature): argmax aggregate sugar -->
- Parameterized rel modules: `use "std/flow.dl" with (edge = my_edge)` — the
  flow stack is reused by copy-editing the union rule today.
- `sym` literals in queries resolved by suffix (`? call_edge(_, ~"Engine::tick")`)
  — today you match on decoded text; a suffix-matching sym operator would use
  the interned ids.

## Ease-of-use (mostly done; residual)

Done this arc: `dl what/summary`, semi-naive (cold 5.6s / warm ~35ms), honest
stmt_ms, --parse-only, --max-wall, parse-tier reserved names, backslash warn,
S6 bail, file-size + drift rails, PLANS.md index, cross-harness hooks/skills.
Residual: ambient-config hermeticity (DESIGNED, sign-off pending —
plans/2026-07-11-ambient-config-hermeticity-design.md); daemon-hijack
visibility is part of that design; module_edge nondeterminism (queue item 15,
evidence-backed) is the last foundation crack — fix BEFORE trusting any
cluster-level dogfood metrics, it cascades into bom/cohesion numbers.

## Recommended order

1. Queue item 15 (module_edge determinism) — foundation, everything joins it.
2. SCIP relationships importer (idea 1) — biggest recall unlock, pure add.
3. Watchgate index.scip allowlist (gap 1) — daily-driver papercut, S.
4. Hermeticity design sign-off -> build (design done).
5. `at()` rel + sym_pkg as dogfood sweeteners.
