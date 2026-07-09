# 6. Make it a rail

> `diag` rows, `--check` exit codes, the `--lsp` one-liner.

**Goal:** turn a query into a `diag` rule, run it as a `--check` gate with a real
exit code, and see the same rule become an editor diagnostic.

A `?` query prints rows for a human. A rail *fails a build*. The bridge is the
built-in `diag` relation. A rule that heads `diag` becomes a diagnostic, and
`dl --check` renders it and sets an exit code CI can read.

## The program

Flag every function that is defined but never called. Save as `06.dl`:

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel called(callee_name: text).
called(callee_name) <-
    call_edge(_, callee_sym, _),
    call_name(callee_sym, callee_name).

diag(path: file, line: line, severity: "warning", code: "unused-fn",
     msg: "function `${name}` is defined but never called") <-
    type_entity(_, sym, name, "function", _, file, line),
    call_name(sym, name),
    name != "main",
    !called(name).
```

`diag` is a fixed-schema built-in with nine columns (path, line, col, end_line,
end_col, severity, code, msg, hint). You name only the columns you use; the rest
default. Here the rule fires for a `type_entity` of kind `function` whose name is
never a callee in `call_edge`, skipping `main` (an entry point nobody calls). The
`${name}` in the message interpolates the bound variable. (Note the `$` sigil:
`diag` messages use `${var}`, while `gen` templates in lesson 7 use `{var}`.)

## Run it as a check

```sh
dl 06.dl --no-daemon --check
```

```
src/app.rs:30: warning[unused-fn]: function `unused_helper` is defined but never called
```

One finding: `unused_helper` at `src/app.rs:30`. `--check` renders `diag` rows to
stderr in a compiler-style format. The exit code is 0, because the severity is
`warning`. Warnings are advisory. They do not fail the gate.

## Make it fail CI

Change the severity to `"error"` and rerun:

```dl
diag(path: file, line: line, severity: "error", code: "unused-fn",
     msg: "function `${name}` is defined but never called") <-
    type_entity(_, sym, name, "function", _, file, line),
    call_name(sym, name),
    name != "main",
    !called(name).
```

```sh
dl 06.dl --no-daemon --check
echo "exit: $?"
```

```
src/app.rs:30: error[unused-fn]: function `unused_helper` is defined but never called
1 error-severity diagnostic(s) found
exit: 2
```

`--check` exits 2 when any `error`-severity row exists, 0 when clean. That exit
code is what a pre-commit hook or a CI step reads. An `error` fails the build; a
`warning` reports without failing.

## The same rule, live in your editor

The `diag` relation feeds two renderers off the one rule. `--check` is the CI
one. `--lsp` is the editor one:

```sh
dl 06.dl --lsp
```

runs `dl` as a language server over stdio, and every `diag` row becomes a squiggle
on the offending line as you edit. One rule, a CI gate and a live lint. You do not
write the diagnostic twice. See `dl docs` and the `--lsp` help for wiring it into
an editor.

## Exercise

Add a second `diag` rule to the same file that flags any `struct` (from
`type_entity`, kind `struct`) whose name is longer than four characters, as a
`warning` with code `"long-type-name"`. Run `--check` and confirm both findings
print. Which structs in the fixture trip it?
