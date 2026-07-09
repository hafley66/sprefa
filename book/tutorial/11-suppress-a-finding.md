# 11. Suppress a finding

> `std/suppress.dl`: eslint-style disable comments for your own rails, directive visibility, and the unused-suppression check.

**Goal:** give the rail you built in lesson 6 an escape hatch: an inline
comment that suppresses one finding, with a recorded reason, and a warning when
a suppression goes stale.

A rail with no escape hatch gets deleted the first time it is wrong. The
grown-up move is the eslint one: the developer silences one specific finding in
place, says why, and the linter warns when the silencing outlives the problem.
In `dl`, that whole grammar is a library, not an engine feature: `use
"std/suppress.dl"` and join.

## The rail

Save as `11.dl`. The pattern flags `.to_string()` calls (the fixture has
exactly one, in `Note::new`):

```dl
use "std/suppress.dl".

rel alloc_hit(path: file, line: int, col: int, end_line: int, end_col: int).
alloc_hit(path, line, col, end_line, end_col) <-
    scan("src/**/*.rs", path, rev),
    sg(path, rev, :rust, "$RECEIVER.to_string()", line, col, end_line, end_col).

lint_candidate(path, line) <- alloc_hit(path, line, _, _, _).
rail_finding(path, line, "no-alloc") <- alloc_hit(path, line, _, _, _).

diag(path: path, line: line, col: col, end_line: end_line, end_col: end_col,
     severity: "error", code: "no-alloc",
     msg: "allocation via .to_string()") <-
    alloc_hit(path, line, col, end_line, end_col),
    !suppressed(path, line, "no-alloc"),
    !suppressed(path, line, "*").
```

Three things beyond lesson 6's rail:

- The two `!suppressed(...)` guards. `suppressed(path, line, code)` is the
  library's export; a rail negates against its own code *and* the wildcard
  `"*"` row (a bare `dl-disable-line` with no codes suppresses everything).
- `lint_candidate(path, line)` tells the library which lines carry findings,
  so *block* directives (`dl-disable` ... `dl-enable`) can cover them. A rail
  using only single-line directives can skip it.
- `rail_finding(path, line, code)` opts into the stale-suppression check:
  the library compares directives against findings and warns on a directive
  that catches nothing.

## Run it: red

```sh
dl 11.dl --no-daemon --check; echo "exit: $?"
```

```
src/note.rs:8: error[no-alloc]: allocation via .to_string()
1 error-severity diagnostic(s) found
exit: 2
```

## Suppress it

Suppose this allocation is fine and you want to say so *at the offense*. Add a
trailing comment to line 8 of `src/note.rs`:

```rust
        Note { body: text.to_string(), pinned: false } // dl-disable-line no-alloc -- byte budget approved
```

The grammar: `dl-disable-line` scopes to this line, `no-alloc` scopes to that
one code (omit it to suppress every code), and everything after `--` is the
recorded reason. There are three siblings: `dl-disable-next-line`, and the
`dl-disable` / `dl-enable` block pair.

Run again:

```sh
dl 11.dl --no-daemon --check; echo "exit: $?"
```

```
src/note.rs:8: info[dl-directive]: suppresses no-alloc on this line (reason: byte budget approved)
exit: 0
```

The error is gone and the exit code flipped. The `info` line is the library's
directive visibility: every recognized directive announces its effect (and a
typo like `dl-disable-nextline`, which would otherwise fail silently, earns a
`dl-directive-malformed` warning). Under `--lsp` these render as subtle dots
in the editor.

## The stale-suppression warning

Directives rot. Delete the `.to_string()` call and the comment stays behind,
silencing nothing, scaring readers. Because the rail heads `rail_finding`, the
library notices. Try it: change line 8's `text.to_string()` to
`String::from(text)`, keep the directive comment, rerun:

```
src/note.rs:8: info[dl-directive]: suppresses no-alloc on this line (reason: byte budget approved)
src/note.rs:8: warn[dl-suppress-unused]: unused suppression: no `no-alloc` finding in its scope — remove the directive
exit: 0
```

The same machinery that let the developer off the hook now tells them to clean
up. Restore the fixture when you are done:

```sh
cd notes-app && git checkout . && cd ..
```

## Why this is a library

Nothing in the engine knows what `dl-disable-line` means. The engine ships one
generic fact stream, `comment_node(path, line, col, end_line, end_col, text,
kind)`, every comment in every grammar it parses, and `std/suppress.dl` is 200
lines of ordinary rules over it: regex-lex the directives, pair the blocks
with argmax (the lesson 5 shape), close ranges over `lint_candidate`. Read it
with `dl examples --std`; it is the best worked example of every technique in
this track combined.

## Exercise

Replace the line directive with the block form: put `// dl-disable no-alloc`
on the line above `Note::new` and `// dl-enable no-alloc` after the method,
and confirm the finding is suppressed (this is what `lint_candidate` is for).
Then remove the `dl-enable` line and rerun: what does an unclosed block cover,
and does anything warn?
