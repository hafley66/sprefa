# Engine refactor v2 — finish the trait extraction, split mod.rs by seam

Amends `plans/2026-06-30-engine-breakdown-proposal.md`: its Stages 0/1/2/3/5
are DONE on main (RelKind + registry, src/rels/ split, effect.rs extraction).
This doc replaces the proposal's remaining Stages 4 and 6 with the measured
current state and adds the file-size budget. Mapping: Phase R1 below = Stage 4
(bucket E behind a trait — but as its own ExtractFamily, not forced into
RelKind), Phase R2 = Stage 6 (tick/source/derived/schema file split). The
proposal's deferred RelCtx borrow-struct seam stays deferred. Instrument:
`examples/coupling-metrics.dl` (extended with file-size metrics + budget rail).

## Status ledger (measured 2026-07-02)

Landed:
- `trait RelKind` (src/rels/mod.rs:82) + `rel_kinds()` registry: 16 impls
  across src/rels/{git,analysis,propose,scip,perf,embed,catalog,querylog}.rs.
  tick, tick_paths, declare_builtins, all_builtin_decls, and the reserved-name
  guard iterate the registry instead of hand-listing families.
- mod.rs carve-outs: extract.rs (1246 lines), tick.rs (803 lines); rel decl
  families in src/rels/ (1621 lines).
- RelDecl carries group/doc across all decl sites (perf arc, 041cf1b).

Coupling metrics, epic baseline -> now:
| metric | before | now |
|---|---|---|
| relkind family members | ~93 | 14 decls / 12 used / 10 refresh / 14 consts |
| dispatch sites | 89 | 17 refresh + 23 used |

Remaining mass: src/engine/mod.rs = 5957 lines, 208 fns, one `impl Engine`
block spanning lines 1519-4810 plus satellites (Splices/Cursors/Appends gen
splicing, ScanBinding, GitBatch, source-row eval).

## What remains, and the organizing rule

The 40 remaining dispatch sites are the extraction-tied families that never
got a trait because their refresh does not fit RelKind's git-state signature:
module (refresh_module_rels + _for_revs/_for_paths), type/call/dataflow
(refresh_type_rels), doc (refresh_doc_rels), spine (refresh_spine_rels).
Hand-dispatched at tick.rs:241-289 (full tick) and tick.rs:651-694
(incremental), each with its own used-gate, dirty test, and digest skip.

Rules of the refactor (the "practical" constraint):
1. A trait only where >=3 members share a shape TODAY (RelKind proved the
   pattern; ExtractFamily has 4 members). No speculative traits.
2. Files split along behavior seams, one topic per file, each holding its own
   `impl Engine { }` block (inherent impls may live in any file of the crate).
   Name-sort splitting stays dead (original epic ruling, preserved).
3. Target budget: no file in src/engine/ or src/rels/ over 1500 lines. The
   rail in coupling-metrics.dl makes the budget loud.

## Phase R1 — ExtractFamily trait (kills the 40 dispatch sites)

```rust
// src/rels/extract_family.rs

/// A built-in rel family populated from parsed file content (not git state):
/// module, type/call/dataflow, doc, spine. Mirrors RelKind's registry shape;
/// differs in refresh inputs (corpus paths + per-family digest skip).
pub trait ExtractFamily: Sync {
    fn rels(&self) -> &'static [&'static str];
    fn decls(&self) -> Vec<RelDecl>;
    fn reserved_msg(&self) -> &'static str;
    /// The `extract:<family>` digest key (perf-arc skip, already persisted
    /// in `_reldigest`). One key per family.
    fn digest_key(&self) -> &'static str;
    /// Full recompute. Ok(true) iff stored rows changed.
    fn refresh(&self, eng: &mut Engine) -> Result<bool>;
    /// Incremental recompute for a changed-path set. Default: full refresh.
    /// ModuleFamily overrides (_for_paths/_for_revs fallback logic moves
    /// INTO the impl, out of tick.rs).
    fn refresh_paths(&self, eng: &mut Engine, changed: &[String]) -> Result<bool> {
        let _ = changed; self.refresh(eng)
    }
    fn used(&self, prog: &Program) -> bool;
}

pub fn extract_families() -> &'static [&'static dyn ExtractFamily];
// [&ModuleFamily, &TypeFamily, &DocFamily, &SpineFamily]
```

```rust
// tick.rs full tick, replacing lines ~241-289:
// for fam in extract_families() {
//     if !fam.used(&prog) { continue; }
//     if fam.refresh(self)? { mark rels changed via fam.rels() }
// }
// tick_paths, replacing ~651-694: same loop with fam.refresh_paths(self, &changed)
```

Notes:
- `refresh` takes `&mut Engine` where RelKind takes `&Engine`; the extraction
  refreshers mutate caches (fact cache, ModuleRows). That is WHY they could
  not ride RelKind — do not force them into it.
- The family digest reporting (perf gap B: seed_rel_digests movers, RelKind
  refresh bools) keys per family; `digest_key()` makes that table-driven too.
- The bodies do not move in R1; only the dispatch does. Expected metric move:
  dispatch_refresh 17 -> ~4 (registry loop internals), dispatch_used 23 -> ~4.

## Phase R2 — split mod.rs by seam (the smaller-files ask)

Each new file = one topic, one `impl Engine` block + its private structs.
Current line regions of mod.rs -> target file:

| region (approx lines) | contents | target file | est. size |
|---|---|---|---|
| 1-850 | free fns, decls tables already thin | stays in mod.rs | — |
| 851-1518 | DiagRow/QueryResult/SpineDelta, Engine struct, caches | stays in mod.rs (the struct owns the doc) | ~700 |
| impl: query paths | query_sql, run_queries, diag rendering, log_query | engine/query.rs | ~600 |
| impl: derived fixpoint | rebuild_derived, DerivedStrata, rel_components, closure/scc caches, ClosureSeed | engine/derive.rs | ~1200 |
| impl: source eval | reconcile_sources, eval source-rule rows (the 5597 region), ScanBinding, GitBatch, enumerate_with_hash | engine/source.rs | ~1400 |
| impl: gen/verify splicing | Splices/Cursors/Appends (4812+), gen apply, verify rollback | engine/gen.rs | ~700 |
| impl: meta + digests | ensure_meta, _reldigest, spine meta insert, save_file_meta | engine/meta.rs | ~500 |

mod.rs after R2: struct + decls + glue, ~1200 lines. Move-only commits: one
seam per commit, `cargo test` between each, zero signature changes. rg for
`fn <name>` after each move to prove no duplicate definitions.

## Phase R3 — re-measure, then rail

- Re-run `dl examples/coupling-metrics.dl --root .`; commit the regenerated
  `examples/_auto-doc/coupling-metrics.md` as the after-snapshot.
- The file-size rail (already in the instrument, warn > 1500 lines) goes
  quiet exactly when R2 is done; it is not wired to --check discovery, so it
  screams only when the instrument runs — wire it into .dl/ discovery ONLY
  after R2 lands, else it's permanent noise.

## Instance lifetimes / storage

- Both registries are `&'static [&'static dyn Trait]` — stateless, built at
  compile time, no runtime registration.
- No schema, table, or digest-key changes anywhere in R1-R3. `_reldigest`
  rows keep their existing keys (`extract:<family>`); the trait just names
  what already exists.

## Proof

1. `cargo test --quiet --test it` and `--lib` green after EVERY commit
   (move-only commits make bisection trivial).
2. coupling-metrics after-snapshot: dispatch_refresh/dispatch_used <= 4 each;
   relkind_* rows unchanged (R1 does not touch RelKind).
3. Size rail: zero over-budget rows for src/engine/*.rs + src/rels/*.rs.
4. dl --check --root . discovery set unchanged (3 info diags).
5. No behavior deltas: `git diff --stat` per commit shows moves (adds+deletes
   balance); any non-move hunk gets called out in the commit message.

## Sequencing vs the body-join desugar

The desugar (plans/2026-07-02-source-rule-body-join-desugar.md) touches
frontend.rs only — no conflict with R1 (rels/ + tick.rs) but real conflict
surface with R2's engine/source.rs move (both read the eval region). Order:
desugar OR R1 first (independent), R2 after the desugar lands.
