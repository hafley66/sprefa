# feature-prolog-rehome-dl6 (pass 1 of 2; the coordinator reads every diff hunk)

You are lane `feature-prolog-rehome-dl6`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `feature/prolog-rehome-dl6`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.
If reality deviates from this brief, STOP and hail the exact text; do not improvise.

## Goal, in Chris's words
"sprefa be an auto refactor": the re-home of `v6/prolog/*.pl` into a numbered, reading-order tree under `v6/prolog/next/` is a dl6 PROGRAM, not a hand job. The program reads the prolog module graph through the extractor, computes the reading order, derives every move and every shim, and hands ONE StageRequest to soopy. Staging is the dry run (soopy writes previews into a state dir, never into the repo). Commit happens only when an approval fact exists. This lane builds the program and its two prerequisites. It MOVES NOTHING in `v6/prolog/`.

## What already exists (read these first, in this order)
1. `v6/dl/fixtures/source-mutations.dl6` (whole file): the stage/commit shape. `rel stage(root, state, request) -> (stage_id, outcome, detail, document)` and `rel commit(root, state, stage_id) -> (outcome, detail, document)`; approval joins on the exact stage_id. Its header says dl6 "cannot yet collect action rows into a JSON array": that is STALE, `json_group_array/1,2` and `json_object/2` are live aggregates (`v6/prolog/compile/registry.pl:163-165`; fixtures `json_array_groups_and_nests`, `ordered_json_group_array_nested_json`, `json_object_groups_and_orders_keys` in `compile/out/manifest.json`). Copy their spelling.
2. `~/projects/hafley-rs/crates/soopy/src/_7b_source_actions.rs:20-215`: the exact JSON. `StageRequest { schema_version: 1, root, actions }`; every enum is `#[serde(tag = "kind", rename_all = "snake_case")]`. `SourceAction::Move { source, expected, destination }`, `Create { path, bytes }`, `Replace { source, expected, edits }`. `SourceRootId::Directory { directory }`, `SourcePath::Directory { path }`, `ActionSource::Directory { file }`. Read `DirectoryId`, `RootPath`, `FileRef`, `ContentId` in the same crate and spell them exactly; `expected` is the content id soopy computes for the current bytes (find how `/soopy/files` reports `digest` and whether it equals ContentId; if not, STOP and hail).
3. `v6/sprefa-engine-rs/src/hosts.rs:20-21` executors: `/soopy/files`, `/soopy/stage`, `/soopy/commit`; `source_stage_response` at `hosts.rs:562` (inputs `root`, `state`, `request`; state dir must be OUTSIDE root; previews come back as the `document` json: `FilePreview { kind, path_before, path_after, summary, unified, binary, before_bytes, after_bytes }` from `soopy/src/_7e_stage_store.rs:106`).
4. `v6/dl/deadcode/dead-module-rail.dl6:20-60`: how a program declares `use extract. use soopy.`, `rel files(glob) -> (path, digest)`, and `rel specifier_at(path, digest, families) -> (record, family, module) key(1, 3)`. Column names on an arrow rel SELECT which executor outputs you receive.
5. `v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs:535-585` `extract_specifiers`: the specifier row carries `module`, `name`, `kind` (line 33-35). So a program may declare `specifier_at(...) -> (record, family, module, name, kind)` and receive all three; confirm by reading how the declared column list maps to values there. If `kind` is not selectable by name, STOP and hail.
6. `v6/sprefa-extract/src/lang/prolog/_0_source.rs:372-445`: the prolog extractor. `use_module/ensure_loaded/consult` become `Specifier { kind: SideEffect | Named, module, name }`; the `module/2` export list becomes `Specifier { kind: Reexport, module: <own name> }`. `include` and `reexport` directives are NOT handled (line 380 match).
7. `v6/prolog/0_generic_expand.pl:50-72`: the include-split shape (module head + `:- include('<folder>/<part>.pl')` per part). PR #453 did it for the parser; `git fetch origin refactor/parser-split` and read `v6/prolog/compile/parse_dl_dcg.pl` there.
8. `v6/prolog/compile.pl:206-323`: the phase order the numbering follows (parse -> expand -> refs/types -> shapes/plans -> lower -> emit).

## Deliverable 1: extractor rows for include and reexport
In `_0_source.rs`, extend the match at :380:
- `include(Path)` -> `Specifier { kind: SpecifierKind::Include, module: None, name: <path text> }`
- `reexport(Path)` -> `Specifier { kind: SpecifierKind::ReexportModule, module: None, name: <path text> }`
- `reexport(Path, List)` -> one `Specifier { kind: SpecifierKind::ReexportModule, module: Some(<path>), name: <indicator> }` per indicator
Add the two `SpecifierKind` variants in `v6/sprefa-extract/src/family.rs` (find the enum; keep existing variants untouched; add the Display/`kind` string spelling `include` and `reexport_module` wherever the existing variants spell theirs). Unit test in the extractor's prolog test file (find `#[cfg(test)]` or `tests/` for prolog; copy an existing use_module test): a 6-line prolog snippet with one `include`, one `reexport/1`, one `reexport/2`, asserting the exact rows. `cargo test -p sprefa-extract` green (background, timeout 600).

## Deliverable 2: the program, `v6/dl/prolog_rehome/rehome.dl6`
Exact rels and output shapes (column names are the contract; the coordinator greps for them):

```
use extract.
use soopy.

# seeds
rel root(path: text).                       # absolute repo root, one fact, from `env` or a seed file; say which
rel state_dir(path: text).                  # OUTSIDE root, one fact
rel phase(folder: text, glob: text, rank: int).   # hand facts: the phase a file belongs to, e.g.
#   phase("0_parse",   "v6/prolog/compile/parse_dl_dcg*.pl", 0). phase("0_parse", "v6/prolog/compile/registry.pl", 0). phase("0_parse", "v6/prolog/use_resolve.pl", 0).
#   phase("1_expand",  "v6/prolog/0_*_expand*.pl", 1).  phase("1_expand", "v6/prolog/1_expansion.pl", 1). phase("1_expand", "v6/prolog/1_host_expand.pl", 1).
#   phase("2_analyze", "v6/prolog/analyze.pl", 2). ... 0_program_check, 0_type_plane, 0_type_ids, 2_subscribe, 3_clock_check, strat
#   phase("3_lower",   "v6/prolog/lower.pl", 3).
#   phase("4_emit",    "v6/prolog/emit_*.pl", 4). phase("4_emit", "v6/prolog/compile/[4-9]_emit_*.pl", 4).
#   phase("5_driver",  "v6/prolog/compile.pl", 5). dl6c, sweep, diag, print_dl, ARCH
#   Every .pl under v6/prolog (excluding conformance/, compile/test/, compile/out/, compile/scripts/) must match exactly one phase; a file matching zero or two is a `?` finding row, not a guess.
rel approval(stage_id: text).               # ABSENT by default; presence = commit

# extract
rel files(glob: text) -> (path: text, digest: text).
rel specifier_at(path: text, digest: text, families: text) -> (record: text, family: text, module: text, name: text, kind: text) key(1, 3).

# graph
rel module_file(path: text, module: text).                       # from the module/2 export rows (kind reexport, module = own name)
rel imports(path: text, target_path: text).                       # use_module/ensure_loaded/consult/reexport_module name resolved to a repo path: relative to the importing file's dir, `.pl` appended when absent; library(...) and unresolvable names dropped into `unresolved_import(path, name)`
rel includes(path: text, part_path: text).                        # include rows resolved the same way
rel reaches(path: text, dep_path: text).                          # transitive closure of imports (NOT includes)
rel reach_count(path: text, deps: int).                           # count(dep_path) per path; 0 for leaves. This IS the topo rank: a dependency's closure is a strict subset of its dependent's, so deps < dependents. Ties break by path text.
rel cycle(path: text, dep_path: text).                            # reaches(a,b) and reaches(b,a): a finding row; the plan still emits, cycles keep relative names

# plan
rel file_phase(path: text, folder: text, rank: int).              # exactly one per file, from phase globs
rel file_order(path: text, folder: text, ordinal: int).           # ordinal = position of path among files of the same folder ordered by (reach_count, path); 0-based; MUST be dense
rel next_path(path: text, next_path: text).                       # "v6/prolog/next/<folder>/<ordinal>_<stem>.pl" where stem = original file stem with any leading digit-underscore prefix removed (0_dot_expand -> dot_expand); include parts move with their module: "v6/prolog/next/<folder>/<ordinal>_<stem>/<part>.pl"
rel shim_text(path: text, text: text).                            # ":- module(<module>_shim, []).\n:- reexport('<relative path from the shim to next_path, no .pl>').\n" for module files; for ARCH.pl and any file that reads prolog_load_context(directory, _) the shim ALSO asserts the original directory: find those files with a cst pattern over `prolog_load_context` and list them in a `?` row
rel action(ordinal: int, action: json).                            # ordered: all Move actions (module files and part files) first, then one Create per shim at the old path; json exactly per soopy's serde shape
rel request(json: json).                                           # json_object with schema_version 1, root, and json_group_array of actions ordered by ordinal

# stage = dry run; commit = apply
rel stage(root: text, state: text, request: text) -> (stage_id: text, outcome: text, detail: text, document: json).
rel plan_stage(stage_id: text, outcome: text, detail: text).
rel plan_preview(stage_id: text, path_before: text, path_after: text, summary: text).   # one row per FilePreview in document
rel commit(root: text, state: text, stage_id: text) -> (outcome: text, detail: text, document: json).
rel plan_commit(stage_id: text, outcome: text, detail: text).      # only derives when approval(stage_id) exists

? file_order(path, folder, ordinal).
? next_path(path, next_path).
? cycle(path, dep_path).
? unresolved_import(path, name).
? plan_stage(stage_id, outcome, detail).
? plan_preview(stage_id, path_before, path_after, summary).
? plan_commit(stage_id, outcome, detail).
```
rx reading, put it at the top of the file as one comment block: `files$ -> mergeMap(extract) -> scan(graph) -> map(plan) -> switchMap(stage) ; approval$ -> withLatestFrom(stage) -> commit`.

Constraints:
- dl variable names descriptive, never single letters. rxjs/prolog/SQL vocabulary only. No `sh`.
- The `request` text column: `stage` takes `request: text`; produce the json via `json_object`/`json_group_array` in a level rel and pass it through; if a json-to-text coercion is a compile stop, cite the `"code"` line and STOP.
- Numbers are computed, never hand-picked: `reach_count` and `file_order` derive from the graph.

## Deliverable 3: receipts
1. `cargo test -p sprefa-extract` green with the new test.
2. The program compiles on the Rust door: `cd v6/prolog && swipl --stack_limit=12G -q -l compile.pl -l emit_rust.pl -g "compile_dl6('<abs>/v6/dl/prolog_rehome/rehome.dl6','<scratch>/rehome.rs',[emitter(emit_rust:emit_program)])" -g halt`. Paste the COMPILE-TRACE line.
3. A DRY RUN against this repo: `dl6 run` (build `v6/sprefa-engine-rs` `--release`, bin `dl6`) with `root` = the worktree, `state_dir` = a scratch dir outside it, NO approval fact. Paste: the `file_order` rows (every file, dense ordinals per folder), `next_path` rows, `cycle` rows, `unresolved_import` rows, `plan_stage` (expect outcome `staged`), and the first 20 `plan_preview` rows. Then `git status --short` in the worktree: MUST be empty apart from your own new files (nothing under v6/prolog moved).
4. State the count of Move actions and Create actions in the request and the arithmetic against the file count.
5. A golden: `v6/dl/prolog_rehome/rehome.golden.txt` = the `?` output of receipt 3 sorted, so a later change to the tree shows up as a diff. Say how it is regenerated.

## Yield results over time (mandatory)
Hail `sprefa-coordinator` (`boop beep hail sprefa-coordinator --from feature-prolog-rehome-dl6 --body "..."`) at: extractor test green; program compiles (COMPILE-TRACE line); dry run staged (counts); done (PR number). STOP-and-hail triggers are named above; add: soopy refuses the request (paste `detail`).

## Deliverables to git
Commits per deliverable. Push. PR to main titled `prolog re-home as a dl6 program: extractor include/reexport rows, reading-order plan, soopy stage dry run`. PR body = receipts 1-5 verbatim.

## You own
`v6/sprefa-extract/src/lang/prolog/**`, `v6/sprefa-extract/src/family.rs` (two enum variants and their spelling only), the extractor's prolog tests, `v6/dl/prolog_rehome/**`. Forbidden: `v6/prolog/**` (read only; MOVE NOTHING), `v6/sprefa-engine-rs/**` except a read of `source_bind/_1_runtime.rs` (if `kind` is not selectable there, STOP; do not edit), `~/projects/hafley-rs/**` (read only).

## Style laws (CLAUDE.md)
No em dashes. No `eprintln!`. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount), honest, grounded. Comments state only constraints the code cannot show. Commit messages imperative. Batteries in the background with a timeout; never foreground-wait over 10 s.
