---
created: 2026-09-10
updated: 2026-09-10
type: bug
status: open
priority: normal
labels:
- size:small
---

# extract rename leaves a redundant import alias behind and breaks the build

_Source: v6/sprefa-extract/src/0_rename.rs_

## Description

`extract rename` respells every bound occurrence of a symbol and stops there. When
the target is reached through an import alias, the rename leaves the alias clause
behind and the file no longer compiles.

## Repro

`packages/signal-grid/src/8_grid.ts:5` in `hafley-rxjs`, before:

```ts
import { createSlice, isSignal, Signal, storageSignal, urlAdapter, type Signal as Sig } from "@hafley66/signals"
```

Run:

```
extract rename src/8_grid.ts#Sig Signal --root . --commit
```

After:

```ts
import { createSlice, isSignal, Signal, storageSignal, urlAdapter, type Signal as Signal } from "@hafley66/signals"
```

```
src/8_grid.ts(5,33): error TS2300: Duplicate identifier 'Signal'.
src/8_grid.ts(5,83): error TS2300: Duplicate identifier 'Signal'.
```

The same shape appears as a standalone import. `src/5_columns.ts:3-4` after the
rename:

```ts
import { Signal } from "@hafley66/signals"
import type { Signal as Signal } from "@hafley66/signals"
```

Four files in that package carried the alias. All four broke the same way, and
each needed a hand edit: collapse `type X as X` to `type X`, and drop a standalone
`import type { X as X }` line whose binding the file already has.

## What the rename got right

Every use site was found and respelled, including type positions. The dry run
printed a correct diff. The only defect is the alias clause the new name makes
redundant.

## Suggested behaviour

When the new name equals the local binding an alias introduces, the alias is
redundant and the rename should collapse it:

| before | after |
|---|---|
| `type Signal as Sig` renamed to `Signal` | `type Signal` |
| `import type { Signal as Sig }` where the file already imports `Signal` | drop the line |
| `import { Foo as Bar }` renamed to `Foo` | `import { Foo }` |

Refusing is also acceptable and matches the existing contract. `rename` already
stops on `Ambiguous`, `NotFound`, `Inexact` and `Dynamic` rather than half
applying, and "renaming this would produce a duplicate binding in FILE" belongs in
that set. A stop the caller can read beats a commit that does not compile.

## Related: `rename` has no `--verify`

`move` takes `--verify <CMD>` and rolls every touched path back on a non-zero run.
`rename` takes only `--verify-scip`, which is report-only, so a rename that breaks
the build leaves the tree broken and the caller runs the typecheck themselves. The
`--verify` plus rollback machinery already exists in `move`; wiring it to `rename`
would have caught this repro automatically.
