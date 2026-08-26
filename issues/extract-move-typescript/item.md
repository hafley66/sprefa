---
created: 2026-08-25
updated: 2026-08-26
type: feature
assignee: chris
status: open
priority: normal
epic: extract-port-closeout
labels: [extract, refactor, typescript]
---

# extract move: TypeScript and categorized batch moves

## Description

Extend `sprefa-extract`'s `extract move` command from its current single-Prolog-file contract to TypeScript/JavaScript file moves and categorized batch moves.

Current implementation: `v6/sprefa-extract/src/0_move.rs`. It scans only Prolog files, extracts Prolog specifiers, and accepts one `old new` pair. The Grapht source-layout refactor in `hafley-rxjs` provides the working fixture: 38 flat implementation files were moved into four numbered directories, with corresponding tests and direct adapter consumers.

The manual refactor exposed reference classes that the move planner must classify rather than treating every path-shaped string alike:

- static imports and export-from specifiers
- imports inside the moved file that must be re-aimed from its destination directory
- package subpath exports such as `dist/27_browser.js` to `dist/browser.js`
- executable and script references to compiled output paths
- `new URL(..., import.meta.url)` asset and fixture paths
- filesystem paths assembled with `resolve`, `join`, `dirname`, and `fileURLToPath`
- test helper, result, fixture, and bin paths after tests gain one directory level
- unrelated local adapter imports that must remain unchanged
- file mode preservation

The first Luna pass on the Grapht fixture produced 205 passing tests only after review corrected 27 files containing bad relative-path rewrites. Earlier intermediate state had 80 passing and 26 failing tests. Failures included imports pointed two directories above `src`, fixture URLs resolving under `packages/fixtures`, bins resolving under `tests/bin`, results resolving under `tests/results`, and adapter-local imports redirected into the package root.

## Proposed CLI contract

Type signatures first:

```rust
struct MovePair {
    old: PathBuf,
    new: PathBuf,
}

enum ReferenceClass {
    ModuleSpecifier,
    PackageExportTarget,
    ImportMetaUrl,
    FilesystemExpression,
    ScriptPath,
}

fn plan_moves(root: &Path, moves: &[MovePair], languages: LanguageSet) -> Result<MovePlan, MoveError>;
fn classify_reference(source: &Source, span: Span) -> Option<ReferenceClass>;
fn rewrite_reference(reference: &Reference, moves: &MoveMap) -> Option<Edit>;
fn validate_plan(plan: &MovePlan) -> Vec<MoveDiagnostic>;
```

Pseudocode body:

```text
parse every move pair and reject duplicate sources or destinations
build one old-path -> new-path map before scanning any importer
extract typed path references from the selected corpus
for each reference:
  resolve its target from the reference owner's original directory
  move the owner directory when the owner itself moves
  map the resolved target through the complete move map
  emit the shortest policy-preserving replacement
stage reference edits, file moves, and optional shims separately
validate every rewritten relative target against the staged tree
preview deterministically, then commit through soopy
```

Instance timeline: parse the whole batch, scan the unchanged snapshot once, calculate all destinations against the staged final tree, preview, then commit reference edits and moves. No move may affect how a later move is interpreted.

Storage and uniqueness: one canonical repository-relative path per source and destination; reject repeated sources, repeated destinations, destination/source cycles that cannot be staged, paths outside the root, and destinations that exist outside the move set. Store references keyed by owner path plus byte span so one source file receives one ordered replacement action.

## Acceptance Criteria

- [ ] `extract move` supports TypeScript and JavaScript static import and export-from specifiers.
- [ ] The moved file's own relative imports are recalculated from its destination directory.
- [ ] Multiple move pairs are planned against one pre-move snapshot and one final path map.
- [ ] Dry-run output is deterministic and leaves the corpus byte-identical.
- [ ] Commit mode preserves file content, executable mode, and Git-visible rename identity where possible.
- [ ] Every planned relative module target resolves in the staged final tree or produces a named diagnostic.
- [ ] Non-module path references are classified separately; unsupported classes produce named diagnostics instead of silent rewrites.
- [ ] Package export targets, `import.meta.url` assets, script output paths, and filesystem expressions each have explicit support or explicit unsupported receipts.
- [ ] The Grapht 38-file categorization is captured as a fixture or generated equivalent, including nested source/test categories and adapter consumers.
- [ ] The fixture catches the 27 reviewed path failures from the manual/Luna refactor.
- [ ] Existing Prolog move behavior and shim behavior remain covered.

## Tests Run

- [ ] Existing `v6/sprefa-extract/tests/1_move.rs` passes.
- [ ] TypeScript single-file move golden passes.
- [ ] TypeScript categorized batch-move golden passes.
- [ ] Grapht-equivalent import, fixture, bin, result, package-export, and adapter-local reference cases pass.
- [ ] Dry-run and commit receipts match except for mutation status.

## Implementation Notes

- [ ] Reuse the TypeScript parser and module-specifier records already emitted by `sprefa-extract`; do not regex source imports.
- [ ] Keep reference classification separate from path resolution and staged mutation.
- [ ] Keep the current soopy staged mutation boundary.
- [ ] Preserve the existing read/parse fan-out and deterministic sequential merge.
- [ ] Decide whether package manifests and script strings belong in this command or in a separate typed path-reference pass before marking those acceptance items supported.

## Comments

- 2026-08-25: Logged from the `hafley-rxjs` Grapht filesystem categorization. Resulting layout was `src/0_bench` (14 files), `src/1_sequence` (7), `src/2_graph` (13), `src/3_contracts` (4), plus root barrels. Direct Grapht tests passed 205/205 after correcting the move-induced paths.

## Agent Runs

### 2026-08-26T03:45:22Z · @codex

2026-08-25 Grapht trial against landed commit `13e12ef02`.

- Built `extract` offline from a detached worktree.
- Reconstructed the pre-refactor Grapht tree from `f427e81`, including `packages/grapht`, `packages/d2`, `packages/mmd`, and root `fixtures`.
- Generated a 66-row TSV from the reviewed rename map through `00005e2`.
- Dry run: exit 0, tree untouched, all 66 moves planned.
- Commit run in the disposable tree: exit 0, all 66 moves committed.
- Static TypeScript imports and export-from specifiers matched the reviewed result, including imports inside moved files to unmoved sibling packages.
- Byte comparison against reviewed `00005e2`: 18 differing entries remain. They are two documentation files, three bin scripts, `justfile`, `package.json`, eight moved TS files containing `import.meta.url` or filesystem-relative constants, one package-gate/lane root constant pair, and the empty old `tests/helpers` directory.
- Confirmed covered boundary: TypeScript module specifiers and batch file moves.
- Confirmed uncovered boundary: package export targets, compiled-output paths in scripts, `new URL(..., import.meta.url)`, `resolve`/`dirname` filesystem constants, documentation references, and empty-directory cleanup.
- Disposable roots: `/private/tmp/grapht-move-full.tY3V3k` and `/private/tmp/grapht-move-expected.iUMxzq`.

### 2026-08-26T17:22:28Z · @feature-extract-move-ts-gaps

2026-08-26 gap arcs against the codex trial's 18 differing entries. Four PRs, each graded on a clean cherry-pick onto merged main (full `cargo test --features cli` battery 0 failures every time) and re-measured on a fresh `f427e81` trial tree.

- PR #484 `extract move: remove the directories a run empties` — 18 -> 17. Sweep reads the pre-move tree; soopy has no directory action, so removal is non-recursive `fs::remove_dir` after the commit loop, receipt line `rmdir <rel>` in both modes.
- PR #485 `extract move: re-aim the relative path constants a moved TS file writes` — 17 -> 8. New `lang/ts_paths.rs`, oxc AST classification of string literals under `new URL`/`resolve`/`join`/`fileURLToPath`, resolve-through-the-moves re-aim (co-moving `./helpers` stays byte-identical). Sabotage receipt: first build rewrote `issue.path.join(".")` (array separator), trial measured 17 -> 10 WORSE; pinned by `an_array_separator_is_not_a_path_segment`.
- PR #486 `extract move: package.json target rewrite` — 8 -> 7. `1_move_manifest.rs`, typed serde_json edit of `main`/`module`/`types`/`browser`/`bin`/`exports` leaves, compiled-image mapping through the tsconfig `outDir`/`rootDir` chain, key order preserved (`preserve_order` via oxc_resolver, `wire.rs:57-59`), byte-exact round-trip.
- PR #487 `extract move: --text-refs report mode` — diff stays 7, report-only per `plans/2026-08-25-extract-move-typescript.PLAN.md:642-644` ("out of scope for the move verb; they are manual"). `2_move_text.rs` scans non-TS text for old-path spellings and prints `text-ref <file>:<line> <old> -> <new>`; never writes a byte.

Residual after all four arcs, 7 tree entries, 6 enumerated by `--text-refs` itself on the trial run:

- `packages/grapht/PROTOCOL.md` (2 rows), `packages/grapht/4_keyed_scene_renderer_plan.md`
- `packages/grapht/bin/grapht-adapter-identity`, `bin/grapht-bench` (2 rows), `bin/grapht-fixtures`
- `packages/grapht/justfile`
- `packages/grapht/tests/3_integration/20_packageGate.test.ts` — the `source("grapht", "15_sequenceGeometry.ts")` hunk, a plain string argument to a user function, out of every arc's class by design; the one entry no pass names.

The trial report also surfaced true old-path references outside `packages/grapht`: 4 `issues/*/item.md` files, `packages/grapht-golden/tsconfig.json`, `packages/marbler/plans/2026-08-17-agent-network-view.PLAN.md`.

Trial artifacts: `/private/tmp/grapht-moves-66.tsv` (the 66-row TSV), `/private/tmp/grapht-trial-diff-18.txt` (the baseline diff), expected tree `/private/tmp/grapht-move-expected.iUMxzq`.

