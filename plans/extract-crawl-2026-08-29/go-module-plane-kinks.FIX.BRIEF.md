# Brief: go module plane kinks (lane `fix-extract-go-module-kinks`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-crawl-2026-08-29/go.REPORT.md` "Fixes 3" (PR #559). Two
measured defects after the go module plane (#558), both on
/Users/chrishafley/projects/typescript-go (read-only).

## First action
```
git merge --ff-only ab463868deb7a2d166bb69f83d0d48d6d7041769
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-go-module-kinks sprefa-coordinator "<one line>"`.

## Defect 1: 342 imports under-reported (150 files)
`resolved_import` misses an import that is literally on the file's import
line: `internal/api/proto.go:19` (`packagejson`, the 12th of 14 corpus
imports) and `internal/ast/utilities.go:11` (`debug`, 1 of 3). Reproduce
with `extract --resolve` over the whole corpus, filter `resolved_import`
for those two src paths, list what is missing. Find the drop in
`src/lang/go_modules.rs` / `src/lang/go.rs` import walk (candidates: a map
keyed by package NAME so two packages with one name collide, e.g. two
`debug` packages; a directory index that stops at the first `go.mod`;
`_test.go` siblings). Cite the line, fix it, fail-first test in
`tests/62_go_module_plane.rs` with a fixture that has two packages sharing
a name.

## Defect 2: reachability 4,832 -> 4,580 after #558
Build TWO binaries: origin/main and the #554 merge commit
(`git log --oneline origin/main | grep 'go type plane'` gives the sha; a
second worktree `git worktree add ../base <sha>`). Whole-corpus
`--resolve` with each, normalize both to (caller_path, caller_name,
callee_path, callee_name), `comm` the sets: edges lost at #558, edges
gained. Classify the lost edges by callee package and by the kind they had
(`name_resolve` before) and show 10 examples with file:line. Then fix the
cause in the module-plane leg of `Resolve<CallF>` (most likely: an
import-qualified site whose import could not be bound now returns NOTHING
instead of falling to `call_name_match_in_package`; the brief for #558 said
the name match stays the last leg). Fail-first test.

## Ownership
`src/lang/go_modules.rs`, `src/lang/go.rs`, `tests/62_go_module_plane.rs`,
`tests/fixtures/go_modules/**`, `go.REPORT.md` (append "Fixes 4").
Forbidden: everything else under `src/`.

## Receipt
Rerun `go.crawl.py`: reachable 4,580 -> n (must be >= 4,832 or say why
not); `resolved_import` 9,410 -> n; the 342 -> n oracle-only rows via
`bench.py` against `plans/extract-bench-2026-08-29/go.oracle.module.tsv`.
Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-module-kinks sprefa-coordinator "go module kinks: PR #N, oracle-only 342-><n>, reachable 4,580-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, every
extract call under timeout 10, never --no-verify.
