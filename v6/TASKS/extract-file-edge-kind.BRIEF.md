# extract-file-edge-kind: fork C on issues/extract-modulef-collapse

Chris's word 2026-08-17: fork C. No new family. The module-level output grows
v5's distinctions back, language-neutral. Read the card first, then
`v6/sprefa-extract/src/{schema.rs,deps.rs,types.rs}` and the v5 plane at
`src/engine/family/mod.rs:397-408`, `src/graph/modgraph/{mod.rs:60-95,rust.rs:460-520}`.

Repo: /Users/chrishafley/projects/sprefa. Base: `git fetch origin` then origin/main tip
(10166672f or later). Worktree: `git worktree add .boop-worktrees/feature/extract-file-edge-kind -b feature/extract-file-edge-kind origin/main`.
Provision the pre-commit rail before the first commit: `cd v6/sprefa-extract && cargo build --release --features cli --bin extract`, `pnpm install --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js`.
Never edit the sprefa main tree. No subagents.

## Deliverables
1. `file_edge` gains `kind=<slug>` = the SpecifierKind slug that produced the edge (named/default/namespace/side_effect/reexport). Both fillers set it: `--deps` (deps.rs, has the specifier in hand) and `--scip-deps` (folded from an index; use `unknown` only if the index truly cannot say, and say so in the schema doc). Wire, schema.rs record line, jsonschema if one exists, DDL/dl6 render if file_edge reaches a rel.
2. New record `file_unresolved  src_path=<string>  module=<string>  reason=<slug>` emitted by `--deps` for every specifier that did not resolve, `reason` = the existing `deps.rs` resolution-outcome slug (`as_str` at deps.rs:103). v5 name was module_unresolved. Nothing vanishes silently.
3. New record `package_edge  src_manifest=<string>  dst_manifest=<string>  kind=<slug>`: workspace-internal manifest-to-manifest dependency edges. v5 built this for Cargo.toml only and called it crate_edge; that name is a rust leak. Generic shape, arms for Cargo.toml (port v5 rust.rs:468-520, kinds normal/dev/build), package.json (dependencies/devDependencies/peerDependencies over workspace-local packages), go.mod (replace/require pointing inside the workspace). One arm per manifest kind under a `manifests/` module or similar; state file ownership in the PR.
4. Bindings: confirm the `specifier` record already carries v5's module_binding content (local name, imported name, kind); if a field is missing (e.g. imported-vs-local name for `import {a as b}`), add it to `Specifier`, do not add a record.
5. Update `types.rs:12-19` roster and `types.rs:1018-1029` sketch text: ModuleF stays collapsed by Chris's word (fork C), delete the "flagged for human review" wording. Card `extract-modulef-collapse` closed with receipts, `extract-module-plane-non-ts` note.

## Tests
Fail-pre-fix tests with sabotage receipts in the header per record. Fixture-driven: extend `tests/fixtures/deps` for kind + unresolved; a small workspace fixture with Cargo.toml + package.json + go.mod for package_edge. Gate: `cd v6/sprefa-extract && cargo test --features cli` three times, `cargo test -p sprefa-engine-rs` twice from `v6/sprefa-engine-rs`, `python3 v6/scripts/soopy-lockstep.py` if present (find the recipe in `v6/justfile`). Report numbers.

## Laws
Comment budget. No em dashes. No eprintln. Banned words: provenance, substrate, load-bearing, regime, refusal, support (say refCount). Surrogate-key law if any DDL is touched (read .claude/skills/sql-relational-design). N+1: collect rows, one insert. Descriptive dl variable names.

## Reporting
`boop beep agent register modulef-driver --parent sprefa-coordinator`. PR to hafley66/sprefa with gate numbers, sabotage receipts, and the record table. Merge only if green (standing word: always merge fixes graded green); hail `boop beep hail sprefa-coordinator --from modulef-driver --body "..."` on merge or blocker. `boop beep agent done modulef-driver` at the end.
