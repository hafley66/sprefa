---
created: 2026-09-10
updated: 2026-09-10
type: improvement
status: open
priority: normal
related: ['@rename-redundant-alias']
labels:
- size:small
---

# extract rename has no --verify, so a rename that breaks the build stays broken

_Source: v6/sprefa-extract/src/0_rename.rs_

## Description

`extract move` takes `--verify <CMD>`: it runs the command in the move root after
`--commit`, and a non-zero or timed-out run rolls every touched path back to its
pre-run state.

`extract rename` takes no such flag. Its only verify is `--verify-scip <INDEX>`,
which is report-only. So a rename that breaks the build leaves the tree broken,
and the caller has to notice and repair by hand.

## Why it matters

Renaming is the operation more likely to break a build than moving. A move
rewrites specifiers, which either resolve or do not. A rename changes a binding,
and a binding can collide with one already in scope. `rename-redundant-alias`
is exactly that case, and a `--verify 'npx tsc --noEmit'` would have caught it and
rolled back before the caller saw it.

## Repro

```
$ extract rename src/8_grid.ts#Sig Signal --root . --commit --verify 'npx tsc -p tsconfig.json --noEmit'
error: unexpected argument '--verify' found

  tip: a similar argument exists: '--verify-scip'

Usage: rename --root <ROOT> --commit --verify-scip <INDEX> <TARGET> <NEW>
```

## Suggested behaviour

Give `rename` the same `--verify <CMD>` and `--verify-cwd <DIR>` pair `move`
already has, with the same rollback semantics. The staging and rollback machinery
exists in the move path; this is wiring rather than new mechanism.

The two verbs sharing one flag also means one thing to learn. Today a caller who
knows `move --verify` reasonably assumes `rename --verify`, tries it, and gets a
clap error.

## Adjacent ask (boop report 8, 2026-09-11)

The same migration ran sequential single `--commit` renames after a batch dry
run stopped: two applied, the third stopped at exit 6, and the tree held a
partially migrated module set. `--list --commit` is all-or-zero (every row is
planned before any stage commits; pinned by
`tests/5_rename_rust.rs:list_commit_is_atomic_across_rows`), but a caller doing
one-rename-at-a-time has no machine-readable rollback plan. A `--verify`
rollback or an emitted undo plan answers it; tracked here rather than as a
second issue.
