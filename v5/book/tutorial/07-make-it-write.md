# 7. Make it write

> a `gen` splice into a markdown file, and the marker discipline.

**Goal:** splice a generated table into a markdown file between comment markers,
and learn the discipline that keeps it safe to rerun.

So far programs read and query. `gen` writes. Its splice form replaces the lines
*between* a pair of comment markers with rows you generate, and leaves the rest
of the file untouched. It is how a repo keeps a hand-written doc's machine-owned
section fresh.

## Set up the target

Give the fixture a doc file with an empty marked region. From inside `notes-app`:

```sh
printf '# Notes API\n\nPublic functions in this crate:\n\n<!-- BEGIN: fns -->\n<!-- END: fns -->\n' > API.md
```

`API.md` now holds a `BEGIN`/`END` marker pair with nothing between them. That
region is what `gen` will own.

## The program

Save as `07.dl` inside the fixture (run it from `notes-app`):

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel block(path: file, l0: int, l1: int, marker_name: text).
block(path, l0, l1, marker_name) <-
    scan("API.md", path, rev),
    comment(path, rev, /BEGIN: $marker_name -->/, /END:/, l0, l1, marker_name).

gen(path, l0, l1, "- `{name}` (src/app.rs:{line})") <-
    block(path, l0, l1, "fns"),
    type_entity(_, _, name, "function", _, "src/app.rs", line).
```

Three moving parts:

- The first `scan` seeds the Rust files so `type_entity` is populated. Without it
  the type graph is empty and `gen` has no rows.
- `block` finds the marker pair. `comment(path, rev, /open/, /close/, l0, l1,
  label)` locates a `BEGIN`/`END` region and binds its boundary line numbers `l0`
  and `l1`, plus the captured `label`. The open pattern is
  `/BEGIN: $marker_name -->/`. `$marker_name` is a capture hole that binds the
  `marker_name` variable. The literal ` -->` after it bounds the capture to the
  word `fns`. (Gotcha: a bare `$` inside one of these patterns is a regex
  end-of-line anchor. Write `$marker_name -->`, not `$marker_name$ -->`; the
  second `$` would anchor to end of line and match nothing.)
- `gen(path, l0, l1, "template")` renders one line per body row, between `l0` and
  `l1`. The `{name}` and `{line}` in the template are the row's columns. (Note
  the sigil: `gen` uses `{var}`, unlike `diag`'s `${var}` from lesson 6.)

## Run it

```sh
dl 07.dl --no-daemon
```

```
[gen] wrote API.md
```

Now look at `API.md`:

```
# Notes API

Public functions in this crate:

<!-- BEGIN: fns -->
- `log_note` (src/app.rs:26)
- `parse` (src/app.rs:18)
- `save` (src/app.rs:22)
- `unused_helper` (src/app.rs:30)
<!-- END: fns -->
```

The four functions landed between the markers. Everything outside them, the
heading and the sentence, is untouched.

## Rerun is safe: it converges

Run the exact same command again:

```sh
dl 07.dl --no-daemon
```

This time there is no `[gen] wrote API.md` line. `gen` is convergent: it skips
the write when the bytes already match. So it is safe to run on every tick, in a
pre-commit hook, or under the file-watching daemon. It writes only when the
generated content actually changed.

## The marker discipline

One hard rule. **Never hand-edit the text between a `BEGIN`/`END` marker pair.** A
program owns that region and will overwrite your edit on the next run. If the
content is wrong, fix the *generator* (the `.dl` program), not the rendered lines.
The markers are the contract: outside them is yours, inside them is the program's.
This repo's own README and reference docs are spliced this way.

## Exercise

Add a `<!-- BEGIN: types -->` / `<!-- END: types -->` pair to `API.md`, and a
second `gen` rule that lists every `struct` (from `type_entity`, kind `struct`)
in that region. Run it, then run it again and confirm the second run writes
nothing.
