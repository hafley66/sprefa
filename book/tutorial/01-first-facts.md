# 1. First facts

> a bare `scan`, the `file` relation, the `(repo, path, rev)` coordinate.

**Goal:** turn the fixture's files into rows you can query, and read the
`(repo, path, rev)` coordinate that keys every fact.

Chapter 1 of the book draws the line between source facts (read from files) and
derived facts (computed from other facts). This lesson produces your first
source facts. A `scan` rule is a source rule: its body reads the filesystem.

## The program

Save this as `01.dl`:

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

? file(repo, rev, path).
? seen(path).
```

Three things are happening.

- `rel seen(path: file)` declares a relation named `seen` with one column,
  `path`, typed `file`. The `file` type is checked: a row whose `path` is not a
  real file is dropped.
- `seen(path) <- scan("src/**/*.rs", path)` is the rule. Read `<-` as "if". It
  says: for every file matching the glob, assert `seen(path)`. The glob is
  relative to `--root`.
- The two `?` lines are queries. Each prints the rows of a relation. `file` is a
  built-in relation the engine fills for you the moment any `scan` runs. A bare
  variable in a query puns to the column of the same name, so `file(repo, rev,
  path)` binds those three columns and leaves the fourth (`content`) as a
  don't-care.

## Run it

```sh
dl 01.dl --root notes-app --no-daemon
```

## Expected output

```
? file => repo	rev	path
  notes-app	WORK	src/app.rs
  notes-app	WORK	src/main.rs
  notes-app	WORK	src/note.rs
  (3 rows)

? seen => path
  src/app.rs
  src/main.rs
  src/note.rs
  (3 rows)
```

## What the coordinate means

Every source fact carries where it came from: which `repo`, which `path`, at
which `rev`. Here `repo` is `notes-app` (the basename of the root), and `rev` is
`WORK` (the working tree, the uncommitted state on disk). If you scanned a git
revision instead of the working tree, the same file at a different `rev` would be
a different set of facts. That coordinate is what lets the engine retract one
file's rows when it changes and leave the rest alone. Chapter 4 of the book is
about what that buys you.

`seen` exists only to give the `scan` a home. You did not have to declare `file`;
it is built in. Declaring your own relation and heading it with a `scan` is how
you name a file set you will reuse. Later lessons keep a `seen` rule around for
exactly that reason: it seeds the file set that the type and call graphs extract
from.

## Exercise

Change the glob to `"src/app.rs"` and rerun. How many rows does `? file` print
now? Then change it to `"src/**/*.md"` and predict the row count before you run
it.
