# Refactor-suggestion convergence study

Six independent runs of one identical prompt over the same 19 large `v5/src` files.
Same task, no file partitioning, no stratification, no reviewer bias injected into the prompt.

- **O-A / O-B / O-C** = three Opus 4.8 runs
- **H-A / H-B / H-C** = three Haiku 4.5 runs

Each run read the source directly (no dl engine). Goal of this doc: which findings
**recur across independent runs** (= high-confidence real debt) and **how the two model
tiers differ** in what they surface.

---

## 1. Convergence matrix

✓ = raised explicitly with a concrete location. Ranked by consensus count.

| # | Finding | O-A | O-B | O-C | H-A | H-B | H-C | n |
|---|---------|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| 1 | **`propose.rs` 5–9 copy-paste kernel drivers** (ast_shape/symbol/tree/call_seq/cfg/ddg/callgraph…), differ by one hash/tokenizer line; gain formula already drifts | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **6** |
| 2 | **engine.rs builtin-rel-group fan-out** — `*_rels_used` (~14 one-liners) + `*_rel_decls` (~15 builders) + `builtin_rel_names` + reserved-name ladder + `declare_builtins` all hand-kept in lockstep | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **6** |
| 3 | **engine.rs 4 corpus refreshers** (`refresh_type/call/dataflow/doc_rels`) repeat the identical `_file` LIKE-filter → par_iter extract → resolver → emit-rows skeleton | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **6** |
| 4 | **`modgraph.rs::strip_noise`** 80-line hand-rolled comment/string FSM, per-quote inline arms, `rust:bool` toggles 5–6 behaviors | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **6** |
| 5 | **`datapath.rs` `entries` ≡ `entries_kd`** (and `toml_pair`/`toml_pair_kd`): same 3-format walk forked only on "track the key span" | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | **6** |
| 6 | **`typegraph.rs` parses each file 3× (Rust) / 2× (TS)** across extract / extract_calls / extract_dataflow; trait was meant to unify, didn't | ✓ | ✓ | ✓ | ✓ | ◑ | ✓ | **5–6** |
| 7 | **engine.rs `tick` vs `tick_paths`** duplicate the classify-preamble + refresh ladder; `tick_paths` omits the source/derived collision bail (latent gap) | ✓ | ✓ | ✓ | · | ✓ | · | **4** |
| 8 | **`scip_import.rs::rows`** ~105-line two-pass monolith iterating `documents` twice + 7 trailing sorts | ✓ | ✓ | ✓ | ✓ | ✓ | · | **5** |
| 9 | **`typegraph.rs` 3 parallel dataflow walkers** (`flow_kt`/`ts_flow_expr`/`flow_expr`, ~600 lines) — same 2-rule model, AST type swapped | ✓ | ✓ | ✓ | · | ✓ | · | **4** |
| 10 | **`daemon.rs::handle_request`** ~195-line god match, per-arm lock+serialize+send boilerplate | ✓ | ✓ | ✓ | · | ✓ | · | **4** |
| 11 | **`rspath.rs` `super::`-pop / module-prefix substitution** written twice (`resolve_to_absolute` ≡ `reconvert_prefix`) | · | ✓ | ✓ | ✓ | ✓ | · | **4** |
| 12 | **BodyItem has no term-traversal API** → `rewrite_terms` + `normalize_body_item` + `is_source` + `body_sql` re-walk all variants; silent-skip correctness hazard on a new field | ✓ | ✓ | ✓ | · | ◑ | · | **3–4** |
| 13 | **engine.rs `.conn()` bypass count is ~70, not the ledger's ~36**; most are unmetered reads | ✓ | ✓ | ✓ | · | · | ◑ | **3** |
| 14 | **engine.rs:1971 per-changed-file raw `_file` upsert in `tick_paths` loop** = a real N+1, violates repo rule | ✓ | ✓ | ✓ | · | · | · | **3** |
| 15 | **engine.rs `rebuild_legacy_{type,call,module}_rels`** are one DELETE+INSERT-SELECT each | ✓ | ✓ | ✓ | ✓ | · | ✓ | **5** |
| 16 | **`daemon.rs` broadcast holds `subscribers` lock across blocking socket writes** (`broadcast_diag_changed`/`broadcast_rev_advanced`) | · | ✓ | ✓ | · | · | ✓ | **3** |
| 17 | **`lsp.rs` 3 offset↔line/col converters, O(filelen) per call**, inconsistent 0/1-based | · | ✓ | ✓ | · | ◑ | · | **2–3** |
| 18 | **`spine.rs` blake3 field-hashing open-coded 4×** (`of_coord`/`of`/`of_located`/…) | ✓ | ✓ | ✓ | · | · | · | **3** |
| 19 | **`parse.rs` ~8–11 hand-rolled comma-list loops + shared op-head prologue** uninfactored | ✓ | ✓ | ✓ | ✓ | · | ◑ | **4–5** |
| 20 | **`lower.rs::body_sql`** Pos/Neg/query term-match written 3–4× with drifting bail strings | ✓ | ✓ | ✓ | · | · | ✓ | **4** |
| 21 | **engine.rs Value helper closures `t`/`i` redefined ~10–15×** in refresh fns | · | · | · | ✓ | · | ✓ | **2** |
| 22 | **`modgraph.rs` regex OnceLock factory boilerplate ×7** | · | · | · | ✓ | ✓ | · | **2** |
| 23 | **`modgraph.rs::RustCrates::owner` duplicated verbatim** (build closure vs method), `format!` alloc in hot loop | · | ✓ | ✓ | · | · | · | **2** |
| 24 | **`db.rs::insert_rows` bare `BEGIN`/`ROLLBACK` outside the bump counter**; N+1 meter blind to literal-inlined SQL; per-chunk tuple rebuild | ◑ | ✓ | · | · | · | ◑ | **2–3** |

◑ = raised partially or as a weaker variant.

---

## 2. Latent bugs (not just duplication) — found ONLY by Opus

None of the three Haiku runs surfaced a behavioral bug; they stopped at structural
duplication. Opus runs independently flagged:

| Bug | Location | Runs |
|-----|----------|------|
| Dead `true \|\|` short-circuit in `has_call` | `lower.rs:252` | O-A, O-B, O-C |
| Spine delta path takes `delta` then `let _ = delta;` — always wholesale; `SpineDelta` doc describes nonexistent behavior | `engine.rs:2807` | O-A, O-B |
| `.dl` parse-error URI built without percent-encoding (squiggles land on nothing) | `lsp.rs:530` | O-B, O-C |
| Parse-error line off-by-one vs diag line (1-based fed to `Position.line`) | `lsp.rs:514` | O-A, O-C |
| `TypeEntity` parent sym minted as `Class` regardless of real owner kind | `typegraph.rs:1824` | O-A, O-C |
| `HoleInConcrete.code()` returns wrong `"unknown-scheme"` (leaks to `TypeDiag.code`) | `desc.rs:126` | O-A, O-C |
| Respawn exit path skips `shutdown_cleanup` → stale sock+pid | `daemon.rs:654` | O-B, O-C |
| `percent_decode` off-by-one drops trailing `%XX` | `lsp.rs:468` | O-B |
| TOCTOU on `program_files` between check and swap | `daemon.rs:203` | O-C |

---

## 3. How the two model tiers differed

**Both tiers converged on the same top-5 duplication clusters** (matrix rows 1–5, all 6/6).
That is the robust signal: when six blind runs independently name `propose.rs` kernels, the
engine builtin-rel fan-out, the 4 corpus refreshers, `strip_noise`, and `entries`/`entries_kd`,
those are real, not artifacts of one model's taste.

Where they diverged:

| Dimension | Opus | Haiku |
|-----------|------|-------|
| Behavioral bugs | 9 distinct latent bugs, cross-confirmed | 0 |
| Cross-cutting abstraction | Named the *unifying seam* (BodyItem `terms_mut`, `BuiltinGroup` registry, `FlowLang` trait, `CorpusIndexer`) and traced which findings it dissolves | Named the local dup ("extract a helper") but rarely the shared abstraction across files |
| N+1 / locking hazards | Found `engine.rs:1971` N+1, daemon lock-across-IO, db.rs counter blind-spot | Found db observer-lock (H-C only); missed the engine N+1 |
| Quantitative claims | "~70 `.conn()` sites not 36", verified against source with spot-checks | Counts present but unverified; H-A self-capped at 28 findings |
| Self-correction | — | H-C correctly noted one item was *already fixed* (commit 156f179) — honest, but spent a finding on it |
| Density vs precision | Fewer, deeper, line-exact, ranked by leverage | More items, shallower, occasional vague ranges ("lines previously noted") |
| Token cost | 66k–78k per run | 37k–104k per run (high variance; H-C ran 23 tool calls / 104k) |

**One-line summary:** Haiku is a competent *duplication detector*; Opus is a duplication
detector **plus** a bug finder and an abstraction designer. The overlap (rows 1–5) is where
you can act with the most confidence — no single model's bias is carrying it.

---

## 4. Action shortlist (consensus-weighted)

Ordered by (consensus count × leverage), highest first:

1. **`propose.rs`: `statement_kernel(content, seed, hash)` + `token_kernel`** — collapses 5–9
   drivers (~500→~80 lines), forces the drifting gain formula into one auditable closure. (6/6)
2. **engine.rs `BuiltinGroup` registry slice** driving decls/names/used/refresh/reserved + the
   `tick`/`tick_paths` ladder — collapses the 7–8-site shotgun edit to a 1-line table append. (6/6 + 4/6)
3. **engine.rs `CorpusIndexer` + `NameResolver`** for the 4 refreshers; one `source_lang_files()`
   driven by the lang registry (kills the 3–5× hardcoded extension LIKE). (6/6)
4. **`typegraph.rs` `extract_all()` single-parse + one generic dataflow driver** behind a
   `FlowLang` trait. (5–6/6 + 4/6)
5. **`BodyItem::terms_mut()` / `is_source()`** as the single source of truth — fixes the
   silent-skip rename hazard shared by frontend/typecheck/lower. (3–4/6, highest correctness leverage per Opus)
6. **`modgraph::strip_noise`, `datapath` `entries` merge, `scip_import::rows` single-pass,
   `spine` `hash_fields`, `rspath` `pop_segments`** — mechanical, cross-confirmed, low-risk. (4–6/6)
7. **Fix the 9 latent bugs** in §2 independently of any refactor (each is a small diff).

Method note: this convergence approach is itself the finding — running N blind agents and
keeping only the cross-confirmed items filters single-model hallucination and taste. The 6/6
rows survived two model tiers; treat those as ground truth, the 2–3/6 rows as leads to verify.
