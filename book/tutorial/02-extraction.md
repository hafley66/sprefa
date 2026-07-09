# 2. First extraction

> `match` with a regex capture, then the same thing with `ast`/`sg`; when to use which; metavars and `$$$`.

**Goal:** pull structured facts out of file content two ways, regex and syntax
tree, and learn which one to reach for.

A `scan` gives you files. To get facts *about the code inside* a file you extract
them. There are two families of extractor: regex (`match`) and syntax tree
(`ast`, `sg`). The lesson is the contrast between them.

## Extract with a regex

Save as `02-match.dl`:

```dl
rel fn_def(name: text, path: file, line: int).
fn_def(name, path, line) <-
    scan("src/**/*.rs", path, rev),
    match(path, rev, /fn (?<name>\w+)/, line).

? fn_def(name, path, line).
```

`match(path, rev, /regex/, line)` runs a regex over each scanned file and emits
one row per matching line. A named group `(?<name>\w+)` binds a dl variable of
the same name. The trailing `line` binds the 1-based line number of the match.

The `scan` now takes a third argument, `rev`. `match` needs the revision to know
which version of the file to read, so the scan binds it and passes it along.

```sh
dl 02-match.dl --no-daemon
```

```
? fn_def => name	path	line
  log_note	src/app.rs	26
  main	src/main.rs	6
  new	src/app.rs	8
  new	src/note.rs	7
  parse	src/app.rs	18
  run	src/app.rs	12
  save	src/app.rs	22
  unused_helper	src/app.rs	30
  (8 rows)
```

Eight function definitions. Good enough, because `fn NAME` is an unambiguous
pattern. Regex earns its place when the thing you want has no clean parse handle:
a marker in a comment, a line in a log, text in a file no parser understands.

## Where regex breaks

Now ask a harder question: what does each function *call*? Try the naive regex
"a word followed by an open paren." Save as `02-calls-regex.dl`:

```dl
rel call_by_regex(callee: text, path: file, line: int).
call_by_regex(callee, path, line) <-
    scan("src/**/*.rs", path, rev),
    match(path, rev, /(?<callee>\w+)\(/, line).

? call_by_regex(callee, line).
```

```sh
dl 02-calls-regex.dl --no-daemon
```

```
? call_by_regex => callee	line
  drop	27
  log_note	23
  log_note	26
  main	6
  new	8
  new	9
  new	19
  new	7
  parse	13
  parse	18
  run	12
  run	8
  save	14
  save	22
  to_string	8
  unused_helper	30
  (16 rows)
```

Sixteen rows, most of them noise. The regex cannot tell a call from a
definition, so `fn parse(` at line 18 shows up as a "call" to `parse`. It grabs
`new` from `Vec::new()`, `to_string` from a method call, and the function headers
themselves. A paren after a word is not a call.

## Extract with the syntax tree

Ask the same question of the parse tree instead. Save as `02-calls-ast.dl`:

```dl
rel call_by_ast(callee: text, path: file, line: int).
call_by_ast(callee, path, line) <-
    scan("src/**/*.rs", path, rev),
    ast(path, rev, :rust, "(call_expression function: (identifier) @callee)", line).

? call_by_ast(callee, line).
```

`ast(path, rev, :lang, "query", line)` runs a tree-sitter query against the
parsed file. The string is an S-expression pattern matched against the syntax
tree. A `@capture` binds a dl variable of that name. This query says: match a
`call_expression` whose `function` is a bare `identifier`, and capture that
identifier as `callee`. A method call or a `Type::assoc()` call is a different
node shape, so it does not match.

```sh
dl 02-calls-ast.dl --no-daemon
```

```
? call_by_ast => callee	line
  drop	27
  log_note	23
  parse	13
  save	14
  (4 rows)
```

Four rows, all real calls (`drop`, `log_note`, `parse`, `save`). The regex found
sixteen. The twelve it added were definitions, method calls, and associated-
function calls. The tree knows what the regex could only guess at.

## Metavars: sg and `$$$`

`ast` uses tree-sitter S-expressions. `sg` uses ast-grep patterns, which read
like the code with holes punched in. Save as `02-sg.dl`:

```dl
rel to_string_call(receiver: text, path: file, line: int).
to_string_call(RECEIVER, path, line) <-
    scan("src/**/*.rs", path, rev),
    sg(path, rev, :rust, "$RECEIVER.to_string()", line).

? to_string_call(receiver, line).
```

The pattern `$RECEIVER.to_string()` matches any expression followed by
`.to_string()`. `$RECEIVER` is a **metavar**. Two rules about metavars, both
worth burning in:

- A metavar is `$` plus an **ALL-CAPS** name (`$RECEIVER`, `$X`). It binds a dl
  variable of that exact name, so the head reads `to_string_call(RECEIVER, ...)`.
  A lower-case name in the pattern is not a metavar, it is literal code.
- A single `$X` matches one node. A triple `$$$X` matches a whole list of nodes
  (zero or more), so `panic!($$$ARGS)` matches `panic!()` and `panic!("x", y)`
  alike. Reach for `$$$` when you want "the whole argument list."

```sh
dl 02-sg.dl --no-daemon
```

```
? to_string_call => receiver	line
  text	8
  (1 rows)
```

One hit: `text.to_string()` on line 8 of `note.rs`, with `text` as the receiver.

## The rule

Reach for `match` when the target has no parse tree to stand on: comment markers,
log lines, free text. Reach for `ast` or `sg` for anything the language's parser
understands. The tree cannot be fooled by a paren in a string or a keyword in a
comment.

## Exercise

Write an `sg` rule that finds every `Vec::new()` call and binds the line. Then
write the `ast` form of the same query. (Hint: `dl docs syntax` shows the exact
argument list for both, and `Vec::new()` has no metavar to bind, so pick a
capture that gives you the line.)
