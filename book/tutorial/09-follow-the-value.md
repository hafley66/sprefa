# 9. Follow the value

> the dataflow facts (`df_node`, `df_edge`), `std/flow.dl`'s cross-function `flow_edge`, and a five-line taint walk.

**Goal:** trace one value from the string literal where it is born, across three
function calls, to the sink that consumes it.

Lesson 4 gave you the call graph: *which function calls which*. This lesson is
one level finer: *which value flows where*. The engine lifts every function it
parses into dataflow facts, the same lazy way it built `call_edge`, and a
standard library module joins them across function boundaries.

## The facts

Two built-ins carry the intra-function graph:

- `df_node(id, kind, var, fn_sym, path, line)` — one node per value event. The
  `kind` vocabulary you will meet on the fixture: `param`, `let_bind`,
  `var_read`, `call_res` (a call's result), `ret`, `lit`, `new`.
- `df_edge(from, to)` — value moves from one node to the next, inside one
  function.

Look at a few before joining anything. Save as `09-peek.dl`:

```dl
rel src_file(path: file).
src_file(path) <- scan("src/**/*.rs", path).

? df_node(id, kind, var, fn_sym, path, line).
```

As in lesson 4, the bare `scan` is what triggers the extraction; the relation
holding the paths is incidental. Run it and find these rows among the ~29 (the
`run` method from `src/app.rs`):

```
  src/app.rs:13:25:lit	lit		src/app.rs::method::App.run	src/app.rs	13
  src/app.rs:13:19:call_res	call_res		src/app.rs::method::App.run	src/app.rs	13
  src/app.rs:13:12:let_bind	let_bind	note	src/app.rs::method::App.run	src/app.rs	13
```

That is line 13, `let note = parse("hello");`, exploded: the literal `"hello"`,
the result of the `parse(...)` call, and the `note` binding. The node id is a
coordinate, `path:line:col:kind`. `df_edge` chains them left to right.

## Crossing the function boundary

`df_edge` stops at the edge of each function. The hop from `parse("hello")` in
`App::run` into `parse`'s `text` parameter is a *join*: the argument node, the
call site, the resolved callee, its parameter node. `std/flow.dl` packages that
join (and the return hop back) as `flow_edge`, so you `use` it instead of
rebuilding it. Save as `09.dl`:

```dl
use "std/flow.dl".

rel src_file(path: file).
src_file(path) <- scan("src/**/*.rs", path).

rel tainted(node_id: text).
tainted(node_id) <-
    df_node(node_id, "lit", _, _, "src/app.rs", 13).
tainted(next_node) <-
    tainted(node_id), flow_edge(node_id, next_node).

rel taint_report(path: file, line: int, kind: text, fn_sym: text).
taint_report(path, line, kind, fn_sym) <-
    tainted(node_id), df_node(node_id, kind, _, fn_sym, path, line).

? taint_report(path, line, kind, fn_sym).
```

`tainted` is the recursive shape from lesson 4: seed it with one node (the
`"hello"` literal at line 13), then walk `flow_edge` until nothing new appears.

`use "std/flow.dl"` resolves against the directory next to your program first,
then `$SPREFA_STD`, then the `std/` shipped beside the binary. If the import
fails, point `SPREFA_STD` at a checkout's `std/` directory.

## Run it

```sh
dl 09.dl --root notes-app --no-daemon
```

## Expected output

```
? taint_report => path	line	kind	fn_sym
  src/app.rs	13	call_res	src/app.rs::method::App.run
  src/app.rs	13	let_bind	src/app.rs::method::App.run
  src/app.rs	13	lit	src/app.rs::method::App.run
  src/app.rs	14	call_res	src/app.rs::method::App.run
  src/app.rs	14	var_read	src/app.rs::method::App.run
  src/app.rs	18	param	src/app.rs::function::parse
  src/app.rs	19	call_res	src/app.rs::function::parse
  src/app.rs	19	ret	src/app.rs::function::parse
  src/app.rs	19	var_read	src/app.rs::function::parse
  src/app.rs	22	param	src/app.rs::function::save
  src/app.rs	23	call_res	src/app.rs::function::save
  src/app.rs	23	var_read	src/app.rs::function::save
  src/app.rs	26	param	src/app.rs::function::log_note
  src/app.rs	27	call_res	src/app.rs::function::log_note
  src/app.rs	27	var_read	src/app.rs::function::log_note
  (15 rows)
```

Read it top to bottom and you are watching the value travel: born as a literal
in `App::run` (line 13), into `parse` as a `param` (line 18), out through
`parse`'s `ret` (line 19), back into `App::run`, into `save` (line 22), and
finally into `log_note` (line 26), where line 27 hands it to `drop`. Five
functions, three call boundaries, one seed and one recursive rule.

Real taint programs are this shape plus policy: which nodes count as sources
(user input), which calls are sinks (a query, a shell), which calls sanitize.
`dl examples --show taint` is the production version, with a
`!sanitized(next_node)` guard in the walk and a `diag` head on the sink, and
`std/flow.dl` also exports `flow_summary` / `flow_sanitizer` facts to model
library functions you cannot parse.

## Exercise

Change the seed to the other literal in the fixture, the `"hello"`'s eventual
resting place: `df_node(node_id, "lit", _, _, "src/note.rs", 8)` (the `false`
in `Note::new`). Predict which functions the report will mention before you
run it, and explain why the flow is so much shorter.
