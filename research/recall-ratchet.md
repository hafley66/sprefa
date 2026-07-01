# Recall Ratchet: diet vs rust-analyzer oracle at FILE granularity

Date: 2026-06-26. Method: prebuilt `v5/target/debug/dl`, `SPREFA_SCIP_INDEX=.../index.scip`,
`--root v5`, throwaway `--db /tmp/*.db`. RA = rust-analyzer SCIP; diet = sprefa
`call_edge`+`module_edge`+`type_edge` projected to file pairs.

## Headline

The "41% recall" is a **scan-scope mismatch**, not an extraction deficit. RA
indexes the whole crate (src + tests + build.rs + bins); the diet's eval scan is
`src/**/*.rs` only. Of RA's 193 file edges, **112 involve a file the diet never
scanned** (build.rs / tests/it/* / external). On the comparable scope (both
endpoints under `src/**`) diet recall is **79/81 = 97.5%**. There are only **2**
genuine in-scope extraction misses, both the same kind: fully-qualified `crate::`
paths the resolver won't lower.

**Single highest-leverage move:** widen the diet scan to the full crate source
set (`tests/**`, `build.rs`, bins). Measured jump: **40.9% -> 76.7%** (+35.8pp,
+69 edges). No extractor change.

---

## Task 1: recall-lever.dl numbers

```
miss_n = 114
rec_n  = 0
ratio  = 0/114 = 0.0%
```

`rec_n = 0` is not "name resolution is worthless"; the lever is broken three
ways:

1. **The ref spine carries `use`-import path specs, not identifier occurrences.**
   The 155 `ref` rows are full import paths: `std::path::Path`, `std::sync::Arc`,
   `super::*`, `tree_sitter::Parser`, `tray_icon::menu`, `syn::spanned::Spanned`.
   They are external-crate-dominated (std/syn/tree_sitter/tao/tray_icon). They do
   NOT contain in-repo call-site or type-position tokens.
2. **The join is the wrong shape.** `name_type` compares the FULL path spec
   (`std::path::Path`) to `type_entity.name` (the SHORT name, `Path`). Always
   false. Even a last-segment join would mostly hit external crates with no
   `type_entity` row.
3. **The spine is conditionally empty.** With only `ref`+`string`+`type_entity`
   in the program, `ref` = 0 rows; it populates (155) only when call/module
   relations are also referenced (the node walk + import-leaf interning is gated
   behind those rels). So the lever's result depends on what else the program
   touches.

Conclusion: name-resolution over the *current* ref spine is the wrong lever. The
spine would need to be repopulated with all in-repo identifier occurrences
before this measurement means anything.

---

## Task 2: the 114 misses, reclassified by scope

`seen(path) <- scan("WORK","src/**/*.rs",path,rev).` marks diet scan scope.

| RA edge class | count | diet can hit? |
|---|---|---|
| both endpoints under `src/**` (in scope) | **81** | yes |
| at least one endpoint outside `src/**` (out of scope) | **112** | no (unscanned) |
| in-scope HITS (diet ∩ RA) | 79 | -- |
| **in-scope MISSES** | **2** | -- |

The 2 in-scope misses:
- `src/daemon.rs -> src/tray.rs`
- `src/engine.rs -> src/config.rs`

The 112 out-of-scope misses (the scan artifact) break down:

| bucket | edges | note |
|---|---|---|
| `* -> build.rs` | 20 | RA cfg/extern linkage (see below) |
| `tests/it/main.rs -> tests/it/<mod>.rs` | 58 | `mod <mod>;` decls in the test crate root |
| `tests/it/<x>.rs -> src/<y>.rs` | 22 | `use sprefa_v5::<y>::...` |
| `src/<x>.rs -> tests/it/<y>.rs` | 11 | RA reverse/test-crate artifact (no syntactic basis) |
| `src/lsp.rs -> tree-sitter-dl/src/lib.rs` | 1 | external crate |
| **total out-of-scope** | **112** | |

---

## Task 3: source-confirmed miss buckets (sampled)

Reading the actual `v5/src/*.rs` and `v5/tests/it/*.rs`:

### A. Fully-qualified `crate::` paths (the 2 in-scope misses) -- confirmed
- `src/daemon.rs:503` -- `crate::tray::run_tray(daemon.clone())?;` -> def in
  `src/tray.rs:17`. The diet's call resolver does not lower `crate::tray::run_tray`
  to `run_tray` in tray.rs.
- `src/engine.rs:907` (also 984, 1031, 1049, 2877, 2977) -- `crate::config::RepoConfig`
  type refs -> struct in `src/config.rs:55`. The type resolver does not lower
  `crate::config::RepoConfig`.

### B. `mod <x>;` declarations in an unscanned crate root -- confirmed
- `tests/it/main.rs:4-58` -- 58 `mod <name>;` decls. These are exactly what
  `module_edge` already produces for src/; the diet misses them only because
  tests/it/main.rs is outside the scan glob. Zero extractor work.

### C. Cross-crate `use <lib_name>::...` -- confirmed, partial
- `tests/it/repo_sink.rs:11-13` -- `use sprefa_v5::{db, engine::Engine, prepare_paths};`
- `tests/it/closure_incremental_bench.rs:11` -- `use sprefa_v5::{db, engine::Engine, lex, parse};`
- `tests/it/cst_node_perf.rs:16` -- same shape.
- `tests/it/data_driven_scan.rs:10-12`, `tests/it/daemon_stateful_revs.rs:12-14` --
  `use sprefa_v5::db; use sprefa_v5::engine::Engine; use sprefa_v5::prepare_paths;`
- `tests/it/spine_meta.rs:2` -- `use sprefa_v5::spine::{content_hash_hex, FileId, StringId, ...};`

Resolver behavior is split along **type vs module**:
- lib **types** resolve: `engine::Engine`, `spine::{FileId, StringId, ...}` ->
  `type_edge` -> src/engine.rs, src/spine.rs recovered.
- lib **modules used as call path prefix** (`db::open`, `parse::parse`,
  `ast::...`, `config::...`) do NOT. `lex` resolves inconsistently (name-shared
  module+fn `lex::lex`). `db`/`parse`/`ast`/`config` stay unresolved.

So the root cause is a **cross-crate path resolver gap**: `<crate_name>::<mod>::<item>`
is not lowered because the diet never maps the external crate name `sprefa_v5` to
this crate's lib root (`src/lib.rs`), so the intra-crate module walk never starts.

### D. RA noise (not real references) -- confirmed
- `* -> build.rs` (20): `build.rs` is a build script that compiles vendored
  tree-sitter grammar C sources and declares the `extern "C"` grammar entry
  points that `src/engine.rs` links. RA emits one synthetic cfg/extern edge per
  consuming file. Not a code-graph relationship; not recoverable or desirable.
- `src/<x>.rs -> tests/it/<y>.rs` (11): no syntactic basis. `src/lib.rs:5,22,23`
  declares `pub mod daemon; pub mod scc; pub mod scip_import;` which resolve to
  `src/daemon.rs` / `src/scc.rs` / `src/scip_import.rs` (intra-crate, already
  captured), NOT to `tests/it/`. These are RA test-crate indexing artifacts.

### E. External crate -- confirmed
- `src/lsp.rs -> tree-sitter-dl/src/lib.rs` (1): a separate crate not in this
  index. Unrecoverable without multi-repo indexing.

### Buckets NOT seen (sampled-for, absent at file granularity in-scope)
- trait-impl-head (`impl Trait for X` with no `use`): none as a *file-crossing*
  miss in scope (trait refs ride `use` imports the diet already resolves).
- `pub use` re-export: none surfaced as misses.
- macro invocation (`bail!`, `format!`): none at file granularity.
- const/static reference: none at file granularity.

---

## Task 4: per-bucket magnitude (of the 114 misses)

| bucket | edges | % of 114 | recoverable by diet? |
|---|---|---|---|
| `mod` decls in unscanned test root | 58 | 50.9% | yes -- scan widen |
| cross-crate `use <lib>::<mod>` (call-prefix) | 22 | 19.3% | partial -- crate-name resolver |
| `-> build.rs` (RA cfg/extern noise) | 20 | 17.5% | no (noise) |
| `src -> tests/it` (RA reverse noise) | 11 | 9.6% | no (noise) |
| `crate::` / `super::` qualified paths (in-scope) | 2 | 1.8% | yes -- path resolver |
| external crate | 1 | 0.9% | no (multi-repo) |

Recoverable total: 58 + (6..11 of 22) + 2 = **66..71 edges** of the 114.
The remaining ~45..48 are RA noise (31) + external (1) + the hardest cross-crate
cases (~11).

---

## Task 5: top 3 diet additions, ranked by recall delta

Baseline: **79/193 = 40.9%**.

### #1. Widen the scan to the full crate source set (scope fix) -- +35.8pp
- **What:** change the default/eval scan from `src/**/*.rs` to
  `{src,tests,benches,examples}/**/*.rs` + `build.rs` + `src/bin/**/*.rs`.
- **Rule:** none (configuration, not extraction). `module_edge` already lowers
  `mod X;`; it just needs the crate roots in scope.
- **Measured:** reran recall with `tests/**` + `build.rs` added to `seen` ->
  hits 79 -> **148**, misses 114 -> **45**. **Recall 40.9% -> 76.7%.**
- **Recovered:** the 58 `mod` decls + 11 of 22 cross-crate (the type-shaped ones
  resolve once the test files are parsed).
- **Cost:** one glob change; +64 files parsed (35 -> 99).

### #2. Cross-crate name -> lib-root resolver -- +3.0..5.5pp
- **What:** parse `Cargo.toml` `[package] name` / `[lib] name` to map an external
  crate name used in `use <name>::...` to a source root (for the self crate,
  `sprefa_v5 -> src/lib.rs`). Then reuse the existing intra-crate module
  resolver on the rest of the path.
- **Rule:** on `use <CRATE>::<seg1>::<seg2>...`, if `<CRATE>` is this workspace's
  lib name, rewrite to `crate::<seg1>::<seg2>...` and run the existing module
  walk (`module_edge` / `mod` decls) to find the def file; lower call_edge /
  type_edge for the final segment.
- **Estimated:** recovers 6..11 of the 11 remaining cross-crate test->lib misses
  (the call-prefix ones: `db::open`, `parse::parse`, etc.; a few need call-target
  disambiguation on generic names like `open`). Hits 148 -> **154..159**.
- **Recall 76.7% -> 80.8%..82.4%.**

### #3. Fully-qualified intra-crate path resolver (`crate::` / `super::` / `self::`) -- +1.0pp (file); high symbol-level
- **What:** extend the call/type extractor to lower a path expression whose head
  segment is `crate` / `self` / `super` through the module graph.
- **Rule:** on a call/type ref `crate::a::b::c` (or `super::x::y`), consume the
  prefix via the module graph (`crate` -> `src/lib.rs`; `super` -> parent module
  of the referencing file), walk segments through `mod` decls, and match the
  final segment against `call_def` / `type_entity` in the resolved file; emit
  `call_edge` / `type_edge`. (This is the machinery #2 reuses.)
- **Confirmed misses fixed:** `src/daemon.rs:503 crate::tray::run_tray` and
  `src/engine.rs crate::config::RepoConfig` -> +2 file edges.
- **Estimated:** hits 150 (after #2's lower bound) -> **152**. **Recall 80.8% ->
  81.3%.** Small at file granularity (paths collapse to few file pairs) but
  large at symbol granularity (every `crate::` call/type ref site).

### Ceiling

After #1 + #2 + #3 (upper bound): **161/193 = 83.4%**. The residual 32 are:
- 20 `-> build.rs` (RA cfg/extern synthetic edges -- noise, do not target)
- 11 `src -> tests/it` (RA reverse/test-crate artifacts -- noise, do not target)
- 1 external crate (`tree-sitter-dl`, needs a separate index)

Against RA's *semantically real* edges only (193 - 31 noise = 162), the diet
ceiling is ~99%. **Above ~83% of RA's raw edges you are either replicating RA's
build-script/test-crate linkage (noise you do not want) or indexing external
crates (multi-repo). At that point, use SCIP.** The diet's realistic target is
**~83%**; the path from 41% to 83% is 90% scan scope and 10% path resolution.

---

## Repro commands

```sh
SPREFA_SCIP_INDEX=/Users/chrishafley/projects/sprefa/index.scip \
  v5/target/debug/dl <prog.dl> --root v5 --db /tmp/<name>.db
```
- `v5/examples/recall-lever.dl` -> miss_n=114, rec_n=0.
- `/tmp/scope.dl` (this study) -> rain_n=81, raout_n=112, hitin_n=79, missin_n=2.
- `/tmp/wide.dl` (this study) -> seen_n=99, hit_n=148, miss_n=45.
