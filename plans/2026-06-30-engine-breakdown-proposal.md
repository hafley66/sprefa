# engine.rs breakdown — proposal

`v5/src/engine.rs` is 8212 lines (was 8495; −283 this arc). It is the file an
AI re-reads every session, so its size is a recurring tax. This proposes how to
break it down: which new files, which trait/type changes, and why. Grounded in a
read of the actual file, not a guess.

Principle (settled, do not relitigate): **trait extraction, not file-split by
name**. Name-scatter was killed in the refactor gauntlet (reorder-by-name wrecked
call locality 18→60; see `project_refactor_exploration_dl`). The win comes from
"same role = same trait = one file", and from lifting whole subsystems (the
effect runtime, the tick driver) out as units. `RelKind` (relkind.rs) is the
proven first instance; `coupling-metrics.dl` is the dogfood that measures it.

## Where the lines actually are (measured read)

engine.rs is four subsystems wearing one file:

| subsystem | rough lines | what |
|---|---|---|
| effect / stream / async runtime | ~2000 | `run_stream`, `drain_effects`, `drain_streams`, `rebuild_async`, `apply_cursors`, `run_gen`, `extract_rule_rows`, `apply_cursors` |
| tick orchestrator | ~670 | `tick`, `tick_paths` (change detection, digest prune, rebuild scheduling, the refresh fan-out) |
| built-in relation families | ~2000 | the 21 families: `*_RELS` const + `*_rel_decls` + `*_rels_used` + `refresh_*` body, ×21 |
| source/derived/schema plumbing | ~1500 | `reconcile_sources`, `parse_file` glue, `rebuild_derived` + SQL fixpoint, `declare*`, `ensure_meta` (DDL) |
| `Engine` struct + shared seam | ~1000 | the struct, ctor, `db`/`root`/`rels`/`refresh_rel`/`read_content`/`repo_roots`/`node_file_set` |

The rel families are only ~a quarter of the file. The biggest single lift is the
**effect/stream runtime** (~2000 lines) — it has nothing to do with the datalog
core and extracts cleanly. The 283 lines so far are the cheap, safe warm-up; the
real reduction is Stages 4–6 below.

## Trait taxonomy — one trait, optional methods

The 21 families do NOT share one shape. Today's `RelKind` fits only the simplest
(no-arg, whole-set, self-diffing `refresh(eng) -> Ok(changed?)`). Sorting the
rest by what they additionally need:

| bucket | families | extra need beyond RelKind |
|---|---|---|
| A. self-diff bool (FITS TODAY) | changed, changed_line, created, agent, type_shape, type_lgg, catalog | none — **7 migrated** |
| B. needs file list + heavy compute | propose_extract, propose_clone | `repo_roots`/`node_file_set`/`read_content` on the seam |
| C. reload-gated (not self-diffing) | scip | a `dirty(changed_paths)` input gate (`index.scip` changed) |
| D. embed supercluster | embed (`similar`) | vector cache + knn helper move together |
| E. `()` always-on, ordered | builtin, module, type, call, dataflow, doc, node, spine, daemon, effect | ordering (type before type_shape; node before spine), delta refresh, `()` not bool |
| F. arg-taking | every, clock | program-derived args (intervals/periods) |

Rather than six traits, extend the one `RelKind` with **defaulted optional
methods** so a family opts into only what it needs:

```rust
pub trait RelKind: Sync {
    fn rels(&self) -> &'static [&'static str];
    fn decls(&self) -> Vec<RelDecl>;
    fn reserved_msg(&self) -> &'static str;

    // bucket A/F: prog lets every/clock pull their own args; others ignore it.
    fn refresh(&self, cx: &mut RelCtx, prog: &Program) -> Result<bool>;

    fn used(&self, prog: &Program) -> bool { rels_used(prog, self.rels()) }

    // bucket C: should an incremental tick even call refresh? default: always.
    // scip overrides to gate on `index.scip` ∈ changed_paths.
    fn dirty(&self, _changed: &HashSet<String>) -> bool { true }

    // bucket E: path-scoped incremental. None = "no delta path, do a full
    // refresh". spine/node/module override; the rest inherit None.
    fn refresh_delta(&self, _cx: &mut RelCtx, _changed: &HashSet<String>)
        -> Result<Option<bool>> { Ok(None) }
}
```

Two signature changes from today: `refresh` takes `&mut RelCtx` (the seam, below)
and `&Program`. The 7 existing impls gain an unused `_prog` and swap `eng` for
`cx` — mechanical.

Bucket E's `()`-always families return `bool` instead (they already compute a
changed signal; `refresh_builtin_rels` etc. just don't surface it). Ordering is
the **registry order** of `rel_kinds()` (already how the loop runs) — no new
mechanism, just place `TypeKind` before `TypeShapeKind`, `NodeKind` before
`SpineKind`.

## The Engine seam — `RelCtx`

For a refresh body to LIVE outside engine.rs (not a thin wrapper), every Engine
method it calls must be reachable. Today that worked for buckets A only because
`db`/`root`/`refresh_rel` happen to be `pub(crate)`. Bucket B needs more. Rather
than widen Engine's private surface piecemeal (encapsulation rot), introduce a
borrow struct that exposes EXACTLY the safe surface:

```rust
pub struct RelCtx<'a> {
    pub(crate) db: &'a Db,
    pub(crate) root: &'a Path,
    // the few read helpers families need:
    //   refresh_rel, repo_roots, node_file_set, read_content
}
impl<'a> RelCtx<'a> {
    pub fn conn(&self) -> &Connection { self.db.conn() }
    pub fn refresh_rel(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;
    pub fn repo_roots(&self) -> HashMap<String, PathBuf>;
    pub fn node_file_set(&self, only: Option<&HashSet<String>>) -> Result<Vec<FileRow>>;
    pub fn read_content(&self, root: &Path, rev: &str, path: &str) -> Result<String>;
}
```

`Engine::tick` builds one `RelCtx` and passes it to the registry loop. A family
can touch the seam and nothing else — the move becomes safe AND the blast radius
of "what a rel body can do" is bounded. This is the type change that unblocks
Stages 2 and 4.

## New file layout

```
src/rels/
  mod.rs        trait RelKind, rel_kinds(), RelCtx, shared helpers (col/git_anchors/rekey)
  git.rs        changed, changed_line, created                 (bucket A, done)
  analysis.rs   agent, type_shape, type_lgg                    (bucket A, done)
  catalog.rs    rel_catalog, fn_catalog                        (bucket A, done)
  propose.rs    propose_extract, propose_clone                 (bucket B)
  scip.rs       scip_*                                          (bucket C)
  embed.rs      similar + knn helper                           (bucket D)
  graph.rs      module, type, call, dataflow, doc, node, spine (bucket E)
  daemon.rs     program, head, rev_advanced, effect_log        (bucket E, projections)
  clock.rs      every, clock                                   (bucket F)
src/tick.rs     tick, tick_paths (the orchestrator)
src/source.rs   reconcile_sources, parse_file glue, insert_source_rows, retract_path, digest prune
src/derived.rs  rebuild_derived, the SQL fixpoint, rebuild_closures
src/effect.rs   run_stream, drain_effects, drain_streams, rebuild_async, apply_cursors, run_gen
src/schema.rs   declare*, declare_builtins, ensure_meta (DDL), create_auto_indexes
src/engine.rs   the Engine struct + ctor + the pub(crate) seam ONLY
```

relkind.rs (514 lines now) becomes `rels/mod.rs` + the per-bucket files.

## Staged sequence (each stage builds green + suite green; gauge must drop)

| stage | move | engine.rs Δ (est) | risk |
|---|---|---|---|
| 0 (done) | RelKind + git families | baseline | — |
| 1 (done) | + agent/type_shape/type_lgg/catalog | −283 | low |
| 2 | `RelCtx` seam + propose/scip/embed (buckets B/C/D) | −400 | med (dirty(), knn move) |
| 3 | split rels into `src/rels/*` module dir | ~0 (relocation) | low |
| 4 | bucket E behind RelKind (module/type/call/dataflow/doc/node/spine/daemon/effect) — the big extractor bodies leave | −1500 | high (ordering, delta) |
| 5 | extract `src/effect.rs` (the stream/async runtime) | −2000 | med (self-contained) |
| 6 | extract `src/tick.rs` + `src/source.rs` + `src/derived.rs` + `src/schema.rs` | −1500 | high (the core) |

Target: engine.rs ~1000–1500 lines (struct + seam). Stages 4–6 are where the
size complaint is actually answered; Stages 1–3 build the trait + seam that make
4–6 safe instead of a big-bang rewrite.

## Dogfood gauge (the acceptance test per stage)

`examples/coupling-metrics.dl` measures engine.rs's RelKind coupling via its own
AST (before-snapshot: 93 family members / 89 dispatch sites). Make it the gate:
after each stage it must show a monotonic drop in family-members-in-engine.rs and
dispatch-sites. A stage that doesn't move the number didn't earn its risk. This
keeps the refactor honest and measurable rather than a vibe.

## Invariants the breakdown must not break

- N+1: every refresh stays collect-then-flush via `refresh_rel`/`insert_rows`.
- Auto-doc rail: `builtin_rel_docs` stays the single doc source; `rel_catalog`
  reads it; `undocumented_builtins` fails the build on a missing entry (verified
  yelling 2026-06-30). Moving decls does not move docs — docs stay in one place.
- One rel = one rule kind; reserved-name guard still covers every built-in name
  (now via the registry loop, not 21 hand-written `if`s).
- Ordering: type before type_shape/type_lgg; node before spine (spine reads the
  node spans). Encoded as `rel_kinds()` registry order.
- No `provenance`/`substrate`/`load-bearing`/`regime` identifiers.
