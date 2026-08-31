# Lane `fix-extract-go-field-shadow` (opus): a field named like its type binds the type ref to the wrong file

Read `plans/extract-bench-2026-08-29/ORACLES.REPORT.md` section 13 finding 2.
Minimal case `/Users/chrishafley/projects/typescript-go/internal/ast/ast.go:154-157`,
a struct field `ModifierFlags ModifierFlags`: the `type` family ref for
the type `ModifierFlags` gets `dst_path` = `ast.go` (the field's file)
instead of `modifierflags.go` (the type decl). 5 go rows in the bench; the
rust twin (`Resolver` bound to `hir-ty/src/infer/unify.rs`, 17 rows) is
NOT yours: `src/lang/rust*.rs` belongs to a live lane.

## First action
```
git merge --ff-only 0192e4d28f546a254eca76009f96e21e1eeafe61
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Task
1. Fail-first test in `tests/62_go_module_plane.rs` (or a new
   `tests/6N_go_type_refs.rs`): fixture with `a.go` declaring
   `type ModifierFlags int` and `b.go` declaring
   `type Node struct { ModifierFlags ModifierFlags }`; assert the type ref
   from `Node` targets `a.go`. Confirm it fails on HEAD; paste the
   failure in the test header.
2. Find the resolve arm: `src/lang/go.rs` `Resolve<TypeF>` (grep
   `TypeF`) and the def index it consults. A field def and a type def
   share a name; the arm must filter candidates by def kind (type decls
   only) before the name match. Cite the line you changed.
3. Receipt on the corpus, one process: `timeout 10 extract --resolve --family type <internal/**/*.go cmd/**/*.go>`
   then `plans/extract-bench-2026-08-29/bench.py <yours> go.oracle.type.typedecl.tsv`:
   the 5 misbound rows go to 0, recall/precision before and after in the
   PR body.

## Ownership
`v6/sprefa-extract/src/lang/go.rs`, `src/lang/go_modules.rs`, the test
file. Nothing under `src/lang/rust*`, `src/lang/ts*`, `src/types.rs`.
No `cargo fmt` on files you do not own. Gate: `cargo test --features cli
--no-fail-fast` in background with a log, full counts in the PR body.

## Receipt
Push `fix/extract-go-field-shadow`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-field-shadow sprefa-coordinator "go field shadow: PR #N, misbound 5->n, gate x/y"`.
Laws: no em dashes, no eprintln, descriptive names, comments only for what
code cannot show, no words provenance/substrate/load-bearing/regime/refusal.
