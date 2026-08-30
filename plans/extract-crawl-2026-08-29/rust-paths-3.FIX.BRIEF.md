# Lane `fix-extract-rust-paths-3` (glm53f): rust classes 8, 11, 9, 7 from the census

Read `plans/extract-crawl-2026-08-29/rust.REPORT.md` section 18 (the
15,444-row census, table 18.2). After #576 one-process `ambiguous` is 13,532.
Classes 3a, 3b, 5, 10 (about 8,030 rows) name external types and have no
correct corpus edge; they are the ceiling, leave them. Four classes are ours:

| # | class | rows | leg |
|---|---|---:|---|
| 8 | `T::f()`, T a corpus struct/enum, 0 or 2+ corpus impls | 1,366 | `rust.rs` `assoc_path_type` + `impl_target` |
| 11 | module-qualified `mod::f()` | 1,367 | `rust.rs` `call_name_match_in_module` |
| 9 | free fn / bare name / struct literal | 1,178 | module plane glob leg (`rust_modules.rs` `wildcard_scope`) |
| 7 | named receiver, 2+ corpus impls of (T, m) | 815 | `rust_modules.rs` `impl_target` |

## First action
```
git merge --ff-only 5ea4c683910aa354616b93e91f994332a98f5912
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/rust-analyzer`, `find crates -name '*.rs' -path '*/src/*'`,
ONE process, `--resolve --project-root <corpus>`, `timeout 30`, background, log
(3.01 s measured at #576).

## Rules, per class, fail-first each (tests in `tests/68_rust_receivers.rs`
or a new `tests/7N_rust_paths.rs`, fixtures under `tests/fixtures/rust_findings/`,
HEAD failure pasted in each test header)
- 8 and 7, "2+ impls": Rust's own rule is inherent impl before trait impl,
  and among trait impls the one whose trait is in scope (imported by the
  caller's module through the module plane, or in the prelude). Bind when
  exactly one survives; keep `ambiguous` otherwise. "0 impls": the def is
  a variant or a unit struct constructor; bind to the type def when the
  path's last segment is a variant name of that enum.
- 11: resolve the path prefix through `rust_modules.rs` (`use` bindings,
  `self::`, `super::`, `crate::`, and a sibling module file); then a name
  match inside that module only. `std::mem::take` and other external
  prefixes stay `External` (the `UnresolvedReason` exists; add none).
- 9: a bare name binds through the caller's module scope in this order:
  local item, `use` binding, glob import with a single source, prelude
  (external). Cite what `wildcard_scope` already does and what you add.

## Receipt
Single-process rerun; `plans/extract-bench-2026-08-29/bench.py <yours normalised> rust.oracle.call.tsv`
(tsvs under `plans/extract-crawl-2026-08-29/`). Re-run the census script
from section 18 and paste the before/after per class. PR body: overlap
18,243 -> n, ambiguous 13,532 -> n, per-class table, wall, gate.

## Ownership
`v6/sprefa-extract/src/lang/rust.rs`, `rust_receivers.rs`, `rust_modules.rs`,
the rust test files and fixtures, `plans/extract-crawl-2026-08-29/rust*`.
NOT `src/types.rs`, `go*.rs`, `ts*.rs`, NOT `plans/extract-bench-2026-08-29/`.
No `cargo fmt` on files you do not own. Gate `cargo test --features cli
--no-fail-fast` in background with a log; wall-ratio flakes rerun 3x
isolated, say so. No file over 1 MB.

Push `fix/extract-rust-paths-3`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-paths-3 sprefa-coordinator "rust paths 3: PR #N, ambiguous 13532->n, overlap 18243->n, gate a/b"`.
Laws: no em dashes anywhere including test headers, no eprintln, descriptive
names, comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".

## Reclaimed worktree
A previous agent left partial work in this worktree's stash
(`git stash list`, "rust-paths-3 opus partial"). After the ff-only merge run
`git stash show -p stash@{0} | head -200` and decide per hunk: keep what
matches the rules above, drop the rest. Say in the PR body what you kept.

## Second reclaim
The worktree already carries an untracked `tests/71_rust_paths.rs` and
`tests/fixtures/rust_findings/paths3/` from the previous agent. Read them
first, keep them if they match the rules, and continue from there; the
src changes are still to do. Commit early and often (every passing test).

## Third run
`origin/fix/extract-rust-paths-3` now carries two real commits (impl
tiebreak, module-qualified prefixes, variant ctors; use-binding cycle guard)
plus `tests/71_rust_paths.rs`. `git reset --hard origin/fix/extract-rust-paths-3`
after the ff-only merge, run the test file, then finish the receipt and
post the PR. Commit after every green test; the previous two agents died
mid-turn with work uncommitted.
