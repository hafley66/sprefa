# plan: leaky enums, traits, structs review (v6 Rust)

## Context

Read-only review of `v6/sprefa-extract`, `v6/sprefa-engine-rs`, `v6/sprefa-store`
(52,815 lines of `src/**/*.rs` combined) for types that leak their internals:
enums matched outside their home, single-impl or dead traits, struct fields that
ARE the API, stringly-typed kinds, `pub` items with no external reader, bool/flag
threading. Lens: Chris's rule "each language as its own impl across the board;
never make match arms per lang." No refactor is applied. Base: 3dd679c93.

## Ranked findings

Risk: L = mechanical, M = needs a fixture gate run, H = touches the wire or the
incremental plane.

| # | kind | item | defined at | leak sites (count, files) | proposed shape | blast radius | risk |
|---|---|---|---|---|---|---|---|
| 1 | 2 | dead trait set: `Reach`, `Cascade`, `Reconcile`, `GraphStore`, `GraphStorePlan` | `v6/sprefa-store/src/tasks.rs:79,97,109,130,223` | 0 references repo-wide; `pub mod tasks` at `v6/sprefa-store/src/lib.rs:23`, only impls are `Tasks` (`tasks.rs:257,293,320,343`), `GraphStorePlan` has NO impl. TS twin lives at `v6/sprefa-store/js/src/engine/tasks.ts:97` | delete the module from the Rust crate until the Rust engine needs it, or keep behind a non-`pub` mod with the js twin cited | 1 file (delete 366 lines) | L |
| 2 | 1 | `ExtractLang` — one enum aggregating every language | `v6/sprefa-extract/src/lang/extract_lang.rs:16` | `from_path`/`name`/`parse_name` match arms per lang (`extract_lang.rs:29,53,64`); referenced in 4 files (`astgrep.rs`, `1_ast_rule.rs`, `prolog/_1_rehome.rs`, self) | the `Source` roster (`types.rs:1940`, `dispatch.rs:16` `source_for`) already routes per lang; fold `Dl6/Prolog/Markdown/MarkdownInline` arms into the per-lang `Source` impls and keep the enum only where ast-grep's `Language` trait demands one | 4-6 files | M |
| 3 | 1 | `DfNodeKind` — closed enum, per-lang match/construction | `v6/sprefa-extract/src/types.rs:784` | 125 sites, 6 files: `lang/ts.rs`(28), `types.rs`(27), `lang/rust.rs`(26), `lang/go.rs`(19), `lang/kotlin.rs`(14), `lang/prolog/_0_source.rs`(11) | the shared `df_push` path keeps the wire; each lang gets an `impl DfEmit for XSource`-shaped owner (Family aux already exists, `DfFAux` `types.rs`) so the variant set is per-lang vocab, not one closed enum | 6 files | H |
| 4 | 1 | `TypeEntityKind` — 9-variant closed enum with lang-restricted variants | `v6/sprefa-extract/src/types.rs:203` (`Struct`/`Trait` Rust-only, `Interface` TS-only per its own doc) | 41+ sites, 7 files: `lang/ts.rs`(13), `types.rs`(9), `lang/rust.rs`(8), `lang/go.rs`(6), `lang/kotlin.rs`(4), `lang/python/_0_source.rs`(3), `lang/dl6/_0_source.rs`(2), `lang/prolog/_0_source.rs`(1) | `as_str` stays on the enum; per-lang emission moves behind `Family for TypeF` so a new language never edits the enum | 7-8 files | M |
| 5 | 1 | `CallKind` | `v6/sprefa-extract/src/types.rs:405` | 28 sites, 7 files: `ts.rs`(5), `rust.rs`(5), `types.rs`(4), `kotlin.rs`(4), `dl6`(4), `python`(3), `go.rs`(3), `prolog`(1) | same shape as #4 | 7-8 files | M |
| 6 | 4 | `ImportRef.kind: &'static str` — open string set with per-lang free strings | `v6/sprefa-extract/src/types.rs:1961` (doc names `"import" \| "path_literal" \| "manifest_target" \| a language's own`) | compared to a literal at `lang/ts_rehome.rs:61` (`reference.kind == "manifest_target"`); `kind: &'static str` also at `types.rs:696` (`DfLit`, lit/template/concat) and `move_scip.rs:37` | enum `ImportRefKind { Import, PathLiteral, ManifestTarget }` + per-lang payload variant; the literal compares disappear | 3-5 files (move path) | L |
| 7 | 3 | `GenProgram` — 27 `pub` fields, the field list IS the API | `v6/sprefa-engine-rs/src/program.rs:19` | read across the crate: `.plan` x16 in `hosts.rs`, x6 `run.rs`, x1 `incremental.rs`; `.relations` x6 in `source_bind/_1_runtime.rs`, x1 `run.rs` | builder + accessor methods; fields go `pub(crate)` and shrink to what crosses modules | 3-5 files (engine hosts) | M |
| 8 | 3 | `DredPlan` — 24 `pub` fields | `v6/sprefa-engine-rs/src/types.rs:485` | consumers: `incremental.rs`(20), `write_verbs.rs`(4), `program.rs`(2) | methods for the 3-4 hot reads, rest `pub(crate)` | 3 files | M |
| 9 | 3 | `IncrementalLevelStatement` — 18 `pub` fields (sibling `IncrementalRelationPlan` 14, `IncrementalEdgeStatement` 13) | `v6/sprefa-engine-rs/src/types.rs:548` | `incremental.rs`, `write_verbs.rs`, `sql.rs` | same treatment as #8 | 3 files | M |
| 10 | 6 | `FamilyMask` — 5 bool fields selecting behavior, mirrored by 5 `Option`s in `ExtractOutput` | `v6/sprefa-extract/src/types.rs:1901` (+ `ExtractOutput` `types.rs:1929`) | `FamilyMask`/mask threading through `dispatch.rs`, `cache.rs`, `source.rs`, every `Source` impl | mask becomes a set of `FamilyTag` (already the TAG const on `Family`, `types.rs:188`); `ExtractOutput` iterates families instead of 5 named Options | 6-9 files | M |
| 11 | 1 | `RowColumnType` matched in 7 files | `v6/sprefa-engine-rs/src/types.rs:8` | `source_bind/_0_types.rs`(9), `incremental.rs`(8), `sql.rs`(6), `run.rs`(4), `serve.rs`(1), `ticklog.rs`(2), `write_verbs.rs`(1) | the SQLite-text conversion (`run.rs:114` `cell == "true"` etc.) becomes `RowColumnType::decode(&str)` owned next to the enum; call sites stop matching | 4-7 files | M |
| 12 | 2 | `Rehome` trait — 5 default methods each overridden by at most 1 impl | `v6/sprefa-extract/src/types.rs:1976` | overrides: `shim` prolog only (`prolog/_1_rehome.rs:167`), `text_spellings` ts only (`ts_rehome.rs:113`), `plan_errors` rust only (`rust_rehome.rs:170`), `manifests` rust+ts, `manifest_refs` rust+ts | split `Rehome` (import_refs/respell, 4 impls use it) from optional per-lang extension traits (`RehomeManifests`, `RehomeShim`); callers downcast or the roster carries Option<Box<dyn ...>> | 4-6 files (move path) | L |
| 13 | 6 | flag-cluster struct in incremental plane | `v6/sprefa-engine-rs/src/incremental.rs:1374-1381` (`recount_always`, `always`, `shared_plane`, `self_feeding`) | read/written only inside `incremental.rs` (2+2+1 sites) | one enum naming the mode, or split the two code paths the flags select | 1 file | M |
| 14 | 4 | `trace::family_span(lang: &'static str, family: &'static str)` — stringly, literal at every call site | `v6/sprefa-extract/src/trace.rs:18` | 10+ calls with raw literals: `astgrep.rs:239`, `dl6/_0_source.rs:413,421,429`, `go.rs:1672,1701,1709`, plus ts/rust/kotlin/prolog/data files | take `(&dyn Source, FamilyTag)`-shaped args (both already on the types) or a macro off `Family::TAG`; literals die | 8-10 files, one-line each | L |
| 15 | 1 | `ScipMode<'a>` matched cross-crate | `v6/sprefa-extract/src/project.rs:63` | `project.rs`(7), `bin/extract.rs:399-401`(3), `sprefa-engine-rs/src/hosts.rs:979,1022`(2) | hosts should receive a resolved mode from `extract`, not match on the enum; narrow to `enum`-in, `results`-out at the executor seam | 2-3 files | L |
| 16 | 1 | `FlatFact` — the wire envelope matched everywhere | `v6/sprefa-extract/src/types.rs:2039` | 63 sites, 11 files: `wire.rs`(32, big match at `wire.rs:130`), `scip_rows.rs`(11), `scip_v5_rels.rs`(8), `project.rs`(4), `deps.rs`(2), plus engine `source_bind/_1_runtime.rs`, `hosts.rs` | flattening/row-lowering becomes methods on `FlatFact` (`to_rows(&mut Vec<Row>)` per family impl) so consumers push facts, never match them | 6-11 files | H |
| 17 | 5 | `GraphNs` — 14 `pub` fields, external readers are only the crate's own examples | `v6/sprefa-store/src/lib.rs:623` | `examples/explain_plans.rs:39`, `examples/profile_dred.rs:81`; nothing in engine-rs or extract | `pub(crate)` fields + constructor; the engine-rs crate does not depend on sprefa-store at all (grep: 0 `sprefa_store::` uses) | 1-3 files | L |
| 18 | 6 | `classify_plan` returns `(bool, ScanKind)` — flag+enum pair threaded to callers | `v6/sprefa-engine-rs/src/sql.rs:456` (`ScanKind` `sql.rs:436`, 11 uses, 1 file) | one caller-side ternary each | fold the bool into `ScanKind` as a variant (`Scan { .. }`) | 1 file | L |
| 19 | 1 | `ExecutorCadence` matched in 4 files | `v6/sprefa-engine-rs/src/hosts.rs:31` | `hosts.rs`(2), `run.rs`(2), `executors/clock.rs`(1), `executors/watch.rs`(1) | cadence policy becomes a method the executor answers; the scheduler stops matching | 3-4 files (engine hosts) | L |
| 20 | 6 | duplicated per-lang helper fns that belong on the `Rehome` seam | `stem`: `lang/rust_rehome.rs:1556`, `ts_rehome.rs:417`, `prolog/_1_rehome.rs:424`; `moved_names(cx, &dyn Rehome)`: `rust_rehome.rs:1533` AND `ts_rehome.rs:394` (byte-identical shape) | 2-3 copies each | promote `moved_names` to a `Rehome` default method; one shared `stem` util | 3 files (move path) | L |

## Do first

Small blast radius, leak sits in the move/Rehome path or the engine hosts.

1. #6 `ImportRef.kind` stringly kind -> enum. Move path, 3-5 files, L. Gate: `extract move` fixtures + `bash v6/sprefa-engine-rs/grade.sh` untouched.
2. #20 `moved_names`/`stem` duplication -> `Rehome` default method + one util. Move path, 3 files, L.
3. #12 split `Rehome` optional methods into per-lang extension traits. Move path, 4-6 files, L. DONE 2026-08-28 (branch refactor/rehome-legs): `RehomeManifests`, `RehomeShim`, `RehomeTextSpellings`, `RehomePlanCheck` + `RehomeArm` roster struct in `types.rs`; `rehomes()` returns `&[RehomeArm]`, each `None` leg visible in `lang/mod.rs`.
4. #15 `ScipMode` cross-crate match out of `hosts.rs:979,1022`. Engine hosts, 2-3 files, L.
5. #19 `ExecutorCadence` matching out of `run.rs`/scheduler. Engine hosts, 3-4 files, L.

#1 (delete dead `tasks.rs`) is free but is a deletion, not a leak fix; run it as its own PR if wanted.

## Verification

Nothing to run for the review itself (no code changed). For any future fix PR:
`bash v6/sprefa-engine-rs/grade.sh` (RUST-GRADE byte-clean per fixture) and the
extract move fixtures; measure twice per CLAUDE.md.

<!-- todo(triage): leaky-types review rows #3 DfNodeKind, #16 FlatFact, #2 ExtractLang need a design pass with Chris (per-lang impl shape) before any lane picks them up -->

<!-- todo(triage): tasks.rs trait set (Reach/Cascade/Reconcile/GraphStore/GraphStorePlan) has zero references in the repo; confirm delete-vs-privatize -->

## Appendix: commands run (verbatim)

```sh
git merge --ff-only 3dd679c93            # already up to date
find v6/sprefa-extract v6/sprefa-engine-rs v6/sprefa-store -name '*.rs' -path '*/src/*' | xargs wc -l | tail -1
git grep -n '^pub trait\|^trait ' -- 'v6/sprefa-extract/**/*.rs' 'v6/sprefa-engine-rs/**/*.rs' 'v6/sprefa-store/**/*.rs'
git grep -n '^pub enum \|^pub(crate) enum ' -- 'v6/sprefa-extract/src' 'v6/sprefa-engine-rs/src' 'v6/sprefa-store/src'
git grep -n "match .*\b$e\b\|$e::" -- ...   # per enum: ChangeKind RelationKind TriggerKind InternMode ArmSchedule ArrivalSign ExecutorCadence WriteVerbStrategy TickBoundary ScanKind RowColumnType ScalarSeam LevelPhase FamilyTag ExtractLang ManifestKind FlatFact CstEdgeKind TypeEntityKind CallKind DfNodeKind DataValueKind CfgNodeKind CpgNodeKind CpgEdgeKind ScipMode Staging Fallback SkipReason RelCol RevKind
git grep -n 'impl .* for' -- v6/sprefa-extract/src/types.rs v6/sprefa-extract/src/lang
git grep -n 'impl .*IHostExecutor for\|impl .*SqlRunner for\|impl .*WriteVerbs for\|impl .*IRevisionDiffer for\|impl .*GraphStore for\|impl .*GraphStorePlan for\|impl .*Reach for\|impl .*Cascade for\|impl .*Reconcile for\|impl .*UnfuckSqlite for\|impl .*FindOrdered for\|impl .*Family for\|impl .*Parser for\|impl .*Source for\|impl .*BlobSource for\|impl .*ScipSource for\|impl .*Resolve<\|impl .*Rehome for\|impl .*Project<'
git grep -n 'kind: &.static str\|kind: String\|role: &.static str\|family: &.static str\|tag: &.static str' -- 'v6/sprefa-extract/src' 'v6/sprefa-engine-rs/src' 'v6/sprefa-store/src'
git grep -n '== "' -- 'v6/sprefa-extract/src' 'v6/sprefa-engine-rs/src' 'v6/sprefa-store/src'
git grep -n 'bool,' -- 'v6/sprefa-extract/src' 'v6/sprefa-engine-rs/src' 'v6/sprefa-store/src'
git grep -n 'pub struct' -- v6/sprefa-extract/src v6/sprefa-engine-rs/src v6/sprefa-store/src   # 282 structs; per-file awk counted pub fields per struct
for fld in uses_tick reconcile_every_tick incremental_safe recount_always shared_plane self_feeding plan relations; do git grep -c "\.$fld" -- v6/sprefa-engine-rs/src; done
git grep -n 'Tasks\|GraphStorePlan\|FindOrdered\|UnfuckSqlite\|sprefa_store::' -- v6/sprefa-store/src v6/sprefa-engine-rs/src v6/sprefa-extract/src
git grep -n 'FamilyTag::\|FlatFact::\|DfNodeKind::\|CallKind::\|TypeEntityKind::\|CstEdgeKind::\|ScipMode' -- per-crate/file counts via `| awk -F: '{print $1}' | sort | uniq -c`
git grep -n 'indexer()\|moved_names\|fn stem(\|family_span\|kind ==' -- v6/sprefa-extract/src
```

Notes on counts: `git grep -c` counts matching lines, so per-file counts above are
line counts of `Enum::` uses (construction + match arms), not AST match sites.
Item 5 (`unreachable_pub`) was sampled by grep rather than the nightly lint; the
nightly toolchain was not invoked.
