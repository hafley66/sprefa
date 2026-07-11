# Quickstart

Ten minutes, your own repo, no fixture. This page is the shortest path from
`install` to "a query answered and a CI gate wired." The [hands-on
tutorial](tutorial/README.md) then teaches the language properly, one lesson at
a time.

## 1. Install

From a clone of this repo (the crate lives at the root):

```sh
cargo install --path . --force
```

Or grab a prebuilt macOS binary:

```sh
curl -fsSL https://raw.githubusercontent.com/hafley66/sprefa/main/install.sh | sh
```

Confirm:

```sh
dl --help | head -3
```

```
datalog over files in repo/rev/time space

Usage: dl [OPTIONS] [PROGRAMS]...
```

## 2. First program: every TODO in your repo

`dl` treats a codebase as a database. Files are rows, extraction operators turn
their contents into more rows, and rules join rows into answers. Save this as
`todos.dl` anywhere, in or out of the repo:

```dl
rel todo_comment(source_path: file, source_line: int, todo_text: text).
todo_comment(source_path, source_line, todo_text) <-
    scan("src/**/*.{rs,ts}", source_path, source_rev),
    match(source_path, source_rev, /TODO: (?<todo_text>.+)/, source_line).

rel todo_count(source_path: file, total: int).
todo_count(source_path, count(source_line)) <-
    todo_comment(source_path, source_line, _).

? todo_comment(source_path, source_line, todo_text).
? todo_count(source_path, total).
```

Read it bottom-up: `?` prints a relation, `todo_count` aggregates, and
`todo_comment` is filled by two chained operators. `scan` selects files by
glob (against the repo at the current directory), and `match` runs a regex over
each one; the named capture group
`(?<todo_text>...)` binds the variable of the same name.

Run it against a repo of yours (adjust the glob to your layout):

```sh
dl todos.dl --no-daemon
```

```
? todo_comment => source_path	source_line	todo_text
  src/greet.ts	1	support a locale-aware greeting
  src/main.rs	4	read count from argv instead of hardcoding
  src/math.rs	1	replace naive fibonacci with a memoized version
  (3 rows)

? todo_count => source_path	total
  src/greet.ts	1
  src/main.rs	1
  src/math.rs	1
  (3 rows)
```

Human query output is a TSV block: each data row starts with two spaces, and
cells in the header and rows are separated by tabs. Use `--query-json` when a
machine-readable JSON-lines result is preferable.

(Output shown from a 3-file demo repo. A `[tick]` status line with timings
also prints; it varies per machine and is elided here, as it is throughout the
tutorial.)

`--no-daemon` forces an isolated one-shot run. Without it, `dl` keeps a warm
per-repo daemon so repeat queries answer in milliseconds and rules re-run on
file changes; that story is [lesson 12](tutorial/12-effects-and-the-daemon.md).

## 3. The part you could not have written yourself

The regex above is the smallest extraction. The engine also parses your code:
add a bare `scan` over source files and relations like `call_edge`,
`type_entity`, `df_node` (who-calls-whom, every declared type and function,
where values flow) fill themselves for Rust, TypeScript, and Kotlin. One
example, blast radius, "what is downstream of this function":

```dl
rel calls(caller_name: text, callee_name: text).
calls(caller_name, callee_name) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller_name),
    call_name(callee_sym, callee_name).

rel src_file(source_path: file).
src_file(source_path) <- scan("src/**/*.rs", source_path).

rel reaches(from_name: text, to_name: text).
reaches(from_name, to_name) <- closure(calls).

? reaches("main", to_name).
```

That `closure(...)` is a real transitive closure with a real fixpoint under
it. [Lesson 4](tutorial/04-graphs-for-free.md) runs this against a fixture;
[chapter 2 of the book](02-recursion-and-fixpoint.md) explains why it
terminates.

## 4. Make it a gate

Any relation can become a diagnostic. Add this to `todos.dl`:

```dl
diag(path: source_path, line: source_line, severity: "error",
     code: "no-todo", msg: "TODO in a changed file: ${todo_text}") <-
    todo_comment(source_path, source_line, todo_text),
    changed(source_path).
```

`changed` is built in (worktree vs HEAD), and `diag` is the reserved sink that
three renderers share:

```sh
dl todos.dl --check   # exit 2 if any error rows: CI, pre-commit
dl todos.dl --lsp     # the same rows as live editor squiggles
```

A TODO in a file you have touched now fails the build, points at its exact
line in the editor, and vanishes from both the moment you fix it. One rule,
every surface.

## 5. Where everything else lives

```sh
dl docs            # embedded reference, book, and tutorial index
dl docs syntax     # every operator; dl docs relations = every built-in
dl examples        # 100+ real programs, searchable; --show <name> prints one
dl setup --project # wire --check into a repo's hooks (prompts first)
```

From here, do the [tutorial](tutorial/README.md) in order. It is fourteen
short lessons, each a complete program you type and run against a fixture you
build in lesson 0, and it ends with you shipping a lint rail, a codemod, a
taint query, an effectful poller, and a JSON-RPC server, all in this one
language.
