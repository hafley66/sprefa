# Refactor candidate-type stats (12-run aggregate)

Fuel for the automated-refactor discussion in the sibling worktree. **Scope: candidate types to
extract from EXISTING code. No new features were proposed by any run** — 12/12 runs returned only
extractions/dedup/splits of code already present. That itself is the first stat: the task surface is
pure refactor, no feature drift.

Sample: one identical prompt over the same 19 `v5/src` files, run blind ×12.
- **Opus 4.8 ×6** (A,B,C,D,E,F)
- **Haiku 4.5 ×6** (A,B,C,D,E,F)
- (Plus one earlier stratified pass `S`, excluded from counts to keep the sample uniform.)

---

## 1. Candidate refactor types, by cross-run frequency

Each row = a distinct candidate type/seam to extract from existing code. ✓ = proposed with a concrete
location; ◑ = proposed as a weaker variant (helper/macro instead of a type, or partial). Counts are
runs-that-proposed-it out of 6 per tier.

| # | Candidate type to extract | Existing code it replaces | Opus | Haiku | Σ/12 |
|---|---------------------------|---------------------------|:----:|:-----:|:----:|
| C1 | **`BuiltinGroup`/`RelGroup` registry table** | engine.rs 14-group fan-out: `*_RELS` + `*_rel_decls` + `*_rels_used` + `builtin_rel_names` + reserved-name ladder + `declare_builtins` + tick/tick_paths dispatch (5–8 lockstep sites) | 6 | 6 (1◑) | **12** |
| C2 | **`NameResolver {by_name, sym_at, scip}` struct** | the SCIP-then-syntactic resolver closure duplicated in `refresh_type_rels` + `refresh_call_rels` | 6 | 1 | **7** |
| C3 | **`BodyItem::terms_mut()` / `is_source()` on the AST** | hand-walks in `rewrite_terms` + `normalize_body_item` + `lower::body_sql` + `Rule::is_source` + `parse_file` | 6 | 1 (◑) | **7** |
| C4 | **`enum EdgeKind` / `enum DfKind`** | `&'static str` kinds in `TypeEdge.kind` (~50 literals) + `DfNode.kind` | 5 | 0 | **5** |
| C5 | **`TickPlan { mode: Full \| Paths }` + `plan_rules()`** | the duplicated classify-preamble + refresh ladder across `tick` / `tick_paths` | 6 | 1 | **7** |
| C6 | **corpus-extract driver** (`extract_corpus<F>` / `TypeFactsCollector` / `trait CorpusIndexer`) | the 4 refreshers' identical file-query → par_iter → emit-rows skeleton | 6 | 6 | **12** |
| C7 | **`parse_file` op seam** (`Hit` value + `fan_out`/`cross_bind`, or `trait BodyOp`) | the 7–9 identical `for b in binds { for hit {…} }` cross-product arms | 6 | 1 | **7** |
| C8 | **dataflow-lift seam** (`trait FlowNode`/`DfLang`/`DataflowWalker<N>` + `lift_flow`) | the 3 parallel ~150-line walkers `flow_expr`/`flow_kt`/`ts_flow_expr` | 6 | 2 | **8** |
| C9 | **`FlowCtx`/`WalkCx`/`TsFlowContext` context struct** | the 5–9 threaded args (file/starts/fn_sym/scope/out) in the walkers | 3 | 2 | **5** |
| C10 | **propose-kernel registry** (`statement_kernel`/`token_kernel` + `struct Kernel`) | the 5–9 copy-paste `*_proposals` drivers (carries the lines-vs-tokens gain bug) | 6 | 2 | **8** |
| C11 | **`LineIndex` / `offsets` module** | the 3 O(filelen) line scanners in lsp.rs + `line_start_bytes` in typegraph/propose | 4 | 0 | **4** |
| C12 | **`Db::query_rows<T>` metered read seam** (+ `query.rs`/`read.rs`) | ~60 raw `.conn().prepare().query_map().filter_map(ok)` read sites | 4 | 3 | **7** |
| C13 | **tuple-return → named struct** (`NodeWalkResult`/`RefAdvance`/`SpanHit`/`RowMapper`) | 4–5-positional tuple returns across engine/daemon/lsp | 3 | 2 | **5** |
| C14 | **spine hashing helper** (`hash_u64(&[&[u8]])` / `SpineInterner`) | the blake3 update-finalize dance open-coded 4–6× in spine.rs | 4 | 1 | **5** |
| C15 | **`trait ModuleResolver` + registry** | the 3 divergent `Resolver::edges` init paths (Rust/TS/Kotlin) in modgraph | 1 | 1 | **2** |
| C16 | **`trait DataFormat`/`DataLang`** | `Fmt` threaded through 9–12 `match fmt` functions in datapath | 2 | 2 | **4** |
| C17 | **`do_tick`/`tick_and_notify` helper** | daemon `tick_full` ≡ `tick_paths` lock/notify boilerplate | 1 | 2 | **3** |
| C18 | **RPC dispatch split** (`RpcDispatcher` map *or* per-method free fns + `respond<T>`) | `handle_request` 195-line 13-arm god match | 4 | 1 | **5** |

**Consensus tiers** (Σ/12):
- **Unanimous (12):** C1 builtin-group registry, C6 corpus-extract driver.
- **Strong (7–8):** C2 NameResolver, C3 BodyItem visitor, C5 TickPlan, C7 parse_file seam, C8 dataflow lift, C10 propose kernels, C12 read seam.
- **Moderate (4–5):** C4 EdgeKind, C9 FlowCtx, C11 LineIndex, C13 tuple→struct, C14 spine hash, C18 RPC split.
- **Weak/single-run leads (2–3):** C15 ModuleResolver, C16 DataFormat, C17 do_tick.

---

## 2. The partition-shape split (holds at n=12)

Same finding as the earlier 6-run study, now confirmed at double the sample. For the *same* candidate,
the two tiers reliably pick different shapes:

| Candidate | Opus shape | Haiku shape |
|-----------|-----------|-------------|
| C1 builtin groups | `&[BuiltinGroup{…}]` data table or `trait LazyIndexer` | `HashMap<String,…>` / `all_rel_decls()` / "macro or factory" |
| C2/C6 resolver+corpus | **struct that owns state** (`NameResolver`, `TypeFactsCollector`) | **generic fn over closures** (`generic_extraction_refresh<F,T>`, `refresh_builtin_indexer<F,T>`) |
| C4 kinds | `enum` with `.tag()` | left as `&'static str` (not raised) |
| C7 parse_file | `Hit`/`Bind` value + free fns + one combinator | `struct MatchExtractor` + methods |
| C18 RPC | per-method free fns (often **no new type**) | `RpcDispatcher{HashMap<String, fn>}` |

Rule of thumb for the discussion: **Opus extracts a typed noun (struct/enum/trait owning the facets,
key encoded in the type system); Haiku extracts a verb (generic fn / map / macro / builder, key stays a
runtime string).** Both kill the duplication; only the Opus shape makes a wrong key a compile error.
For *automated* refactoring this matters — the Haiku shapes are mechanically easier to codegen
(table/macro/HashMap), the Opus shapes need type design.

---

## 3. Model-behavior stats (methodology notes)

| Metric | Opus ×6 | Haiku ×6 |
|--------|---------|----------|
| Latent **behavioral bugs** found (not dup) | **6/6 runs** found ≥1; pool of 9 distinct, cross-confirmed | **0/6** |
| Mean candidate-type proposals naming a *new type* | high (struct/enum/trait per seam) | lower (often helper fn / macro, "data-driven approach") |
| Cross-cutting framing (named the seam that dissolves N findings) | 6/6 | 2/6 (H-B, H-F) |
| Mean output tokens | ~74k (range 66–93k) | ~78k (range **37–104k**, high variance) |
| Mean tool calls | **6.7** (5–9) | **19.5** (12–28) — ~3× more I/O for similar coverage |
| Mean wall-clock | ~298s | ~245s (one 721s outlier in round 1) |
| Self-flagged already-fixed item (noise) | 0 | 1 (H-C, commit 156f179) |

Headlines for the sibling worktree:
1. **Bug detection is Opus-only at this sample** (9 distinct bugs, 0 from Haiku). If the automated
   pipeline is meant to catch correctness regressions, Haiku alone won't.
2. **Haiku spends ~3× the tool calls** for comparable-or-narrower findings — relevant to cost/latency
   budgeting if you fan out cheap models.
3. **The two unanimous candidates (C1, C6) are the safe automation seeds** — every model, every run,
   independently. Start the auto-refactor harness there.
4. **Haiku-shaped outputs (table/map/macro) are the more mechanizable targets**; Opus-shaped outputs
   (enum/trait/owned-state struct) are better *specs* but need a type-design step the automation must own.

---

## 4. Cross-confirmed latent bugs (Opus pool, for triage — not refactors)

These are not candidate *types*; they're seeds for an automated *fix* pass. Cross-confirmed across ≥2
Opus runs:

| Bug | Location | Opus runs |
|-----|----------|-----------|
| Dead `true \|\|` in `has_call` | lower.rs:252 | A,B,C,D,E,F (≥5) |
| `.dl` parse-error line off-by-one (1-based fed to 0-based `Position`) | lsp.rs:519 | A,C,D,E,F |
| `.dl` parse-error URI skips percent-encoding | lsp.rs:530 | B,C,D,E |
| Spine delta param dead (`let _ = delta;`), always full reproject | engine.rs:2807 | A,B,D,E,F |
| `tick_paths` per-file `_file` upsert = live N+1 | engine.rs:1971 | A,B,C,D,E,F |
| N+1 counter keys on full literal-inlined SQL → never fires | db.rs:163 | B,D,E |
| lex regex-escape `b[i] as char` Latin-1 promotion (string arm already fixed) | lex.rs:181 | D,E,F |
| `flow_kt` binop double-visits single-operand node | typegraph.rs:472 | D,E |
| `db.rs::insert_rows` bare `BEGIN` (not reentrant) outside the bump counter | db.rs:239 | B,D,E |

---

## 5. Recommended discussion frame

- The **two-axis map** for each candidate: (consensus Σ/12) × (Haiku-mechanizable vs needs-type-design).
  C1/C6 are high-consensus AND have a Haiku-shaped (table/generic-fn) variant → first automation targets.
  C2/C3/C4/C5/C7/C8/C10 are high-consensus but Opus-shaped → automation must own the type-design step.
- **No candidate was a new feature** — confirms the auto-refactor harness can assume "extract from
  existing code" as a closed-world invariant for this codebase.
- Use the §4 bug pool as a separate auto-*fix* track, gated on Opus-tier (Haiku missed all 9).
