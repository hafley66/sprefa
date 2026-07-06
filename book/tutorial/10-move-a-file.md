# 10. Move a file

> the `--move` codemod: use-path rewrite, physical rename, and `mod`-decl surgery, always dry-run first.

**Goal:** move `src/note.rs` to `src/model/note.rs` and let `dl` rewrite every
import, rename the file, and fix the module declarations.

Everything so far *read* code or wrote docs. This lesson rewrites code. The
byte-span spine you have been riding implicitly (every `match`/`ast`/`sg`
capture knows exactly which bytes it came from) is also an edit coordinate, and
`--move` is the built-in refactor that uses it.

## Dry run

No program file this time; `--move` is a flag that takes `OLD=NEW` paths,
relative to the repo root:

```sh
dl --move src/note.rs=src/model/note.rs --root notes-app --no-daemon
```

```
src/app.rs: crate::note::Note -> crate::model::note::Note
[move] 1 edit(s) across 1 file(s) (dry run; pass --fix to apply)
src/note.rs -> src/model/note.rs (rename)
src/main.rs: - mod note;
create src/model.rs with `pub(crate) mod note;`
src/main.rs: + mod model;
```

(You will also see a note that a Kotlin scan matched 0 files; the move
machinery checks every language it knows, and the fixture has no `.kt` files.
Harmless.)

Nothing was written. The plan has three parts, and each is real Rust surgery,
not a string replace:

1. **Import rewrite.** `src/app.rs` says `use crate::note::Note;` today. The
   module path `crate::note` becomes `crate::model::note`.
2. **The rename itself.**
3. **`mod`-declaration surgery.** `src/main.rs` declares `mod note;`, which
   must go; a new `src/model.rs` must declare `pub(crate) mod note;`; and
   `main.rs` must gain `mod model;`. Note the visibility promotion: `note` was
   private to the crate root, and from its new home it must be reachable
   through `model`, so it becomes `pub(crate)`.

## Apply it

```sh
dl --move src/note.rs=src/model/note.rs --root notes-app --no-daemon --fix
```

Same plan, ending in `applied`. Inspect what changed:

```sh
cd notes-app && git diff --stat && head -1 src/app.rs && cat src/model.rs
```

```
 src/app.rs  |  2 +-
 src/main.rs |  2 +-
 src/note.rs | 10 ----------
 3 files changed, 2 insertions(+), 12 deletions(-)
use crate::model::note::Note;
pub(crate) mod note;
```

`git status` shows `src/note.rs` deleted and `src/model/note.rs` plus
`src/model.rs` untracked; `mod model;` was appended to `main.rs`.

## What it refuses to do

`--move` is deliberately loud about its limits rather than approximately
right. A brace-group head it cannot cleanly rewrite, a `mod.rs`/`lib.rs`/
`main.rs` move (which changes module *identity*, not location), and child
`mod` declarations inside a moved file are all counted and reported as skips,
never silently mangled. When you see a skip line, that edit is yours to make.

Two more flags matter at real scale: `--repo <slug>` aims the move at one
configured repo (or `--repo "*"` for all), and `--verify '<command>'` applies
the edits, runs your command (`cargo check`, a test suite), and rolls
everything back if it fails.

## Put it back

The fixture's later state matters to nothing (each lesson re-scans), but keep
your tree clean:

```sh
git checkout . && git clean -fd && cd ..
```

## Exercise

Dry-run a move of `src/app.rs` to `src/core/app.rs` and read the plan. It is
bigger than this lesson's: `app.rs` is *imported* by `main.rs` (`use app::App;`)
and itself *imports* (`use crate::note::Note;`). Find both kinds of rewrite in
the plan, then decide: does the moved file's own import need to change, and why
not?
