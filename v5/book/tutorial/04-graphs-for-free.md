# 4. Graphs for free

> `type_entity` and `call_edge` off the same scan; blast radius with a recursive rule; why you seed a recursive rule instead of reading `closure()` unpinned.

**Goal:** query the resolved call graph and type graph the engine builds off your
scan, then compute blast radius with a recursive rule.

Lesson 3 built a call graph by hand and it could only see bare-identifier calls.
The engine already builds a better one. The moment a program references a built-in
graph relation, the engine extracts that graph over whatever the scan selected.
No enable step. A `scan` plus a mention of `call_edge` is the whole opt-in.

## The graphs the scan gives you

Save as `04-probe.dl`:

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

? call_edge(caller, callee, kind).
? call_name(sym, name).
? type_entity(repo, sym, name, kind, parent, file, line).
```

```sh
dl 04-probe.dl --no-daemon
```

```
? call_edge => caller	callee	kind
  notes-app::src/app.rs::function::save	notes-app::src/app.rs::function::log_note	call
  notes-app::src/app.rs::method::App.run	notes-app::src/app.rs::function::parse	call
  notes-app::src/app.rs::method::App.run	notes-app::src/app.rs::function::save	call
  notes-app::src/main.rs::function::main	notes-app::src/app.rs::method::App.run	call
  (4 rows)

? call_name => sym	name
  notes-app::src/app.rs::function::log_note	log_note
  notes-app::src/app.rs::function::parse	parse
  notes-app::src/app.rs::function::save	save
  notes-app::src/app.rs::function::unused_helper	unused_helper
  notes-app::src/app.rs::method::App.new	new
  notes-app::src/app.rs::method::App.run	run
  notes-app::src/main.rs::function::main	main
  notes-app::src/note.rs::method::Note.new	new
  (8 rows)

? type_entity => repo	sym	name	kind	parent	file	line
  notes-app	notes-app::src/app.rs::function::log_note	log_note	function		src/app.rs	26
  notes-app	notes-app::src/app.rs::function::parse	parse	function		src/app.rs	18
  notes-app	notes-app::src/app.rs::function::save	save	function		src/app.rs	22
  notes-app	notes-app::src/app.rs::function::unused_helper	unused_helper	function		src/app.rs	30
  notes-app	notes-app::src/app.rs::method::App.new	new	method	notes-app::src/app.rs::struct::App	src/app.rs	8
  notes-app	notes-app::src/app.rs::method::App.run	run	method	notes-app::src/app.rs::struct::App	src/app.rs	12
  notes-app	notes-app::src/app.rs::struct::App	App	struct		src/app.rs	3
  notes-app	notes-app::src/main.rs::function::main	main	function		src/main.rs	6
  notes-app	notes-app::src/note.rs::method::Note.new	new	method	notes-app::src/note.rs::struct::Note	src/note.rs	7
  notes-app	notes-app::src/note.rs::struct::Note	Note	struct		src/note.rs	1
  (10 rows)
```

Read what this graph knows that lesson 3's did not. `call_edge` resolved
`main -> App.run` (a method call through a receiver) and `App.run -> parse`. Each
node is a **sym**: a stable identity like `notes-app::src/app.rs::method::App.run`
that names the repo, file, kind, and name. `call_name` maps a sym back to its
bare name, so you can join names when you want readable output. `type_entity`
lists every declared type and callable, and its `parent` column links a method to
the struct that owns it (`App.run`'s parent is `struct::App`).

## Blast radius with a recursive rule

The call graph is edges. "What does `main` reach, directly or through any chain"
is the transitive closure of those edges. Write it as a recursive rule: base case
plus a rule that references itself. Save as `04-reaches.dl`:

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel calls(caller_name: text, callee_name: text).
calls(caller_name, callee_name) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller_name),
    call_name(callee_sym, callee_name).

rel reaches(caller_name: text, callee_name: text).
reaches(caller_name, callee_name) <- calls(caller_name, callee_name).
reaches(caller_name, callee_name) <-
    reaches(caller_name, mid_name), calls(mid_name, callee_name).

? calls(caller_name, callee_name).
? reaches("main", callee_name).
```

`calls` turns the sym-keyed `call_edge` into a readable name-to-name graph by
joining `call_name` on both ends. `reaches` has two rules with the same head, so
they union: a direct call reaches, and if you reach `mid_name` and `mid_name`
calls `callee_name`, you reach that too. The engine runs this to a fixpoint
(chapter 2 of the book).

```sh
dl 04-reaches.dl --no-daemon
```

```
? calls => caller_name	callee_name
  main	run
  run	parse
  run	save
  save	log_note
  (4 rows)

? reaches => callee_name
  log_note
  parse
  run
  save
  (4 rows)
```

`main` reaches all four: `run` directly, then `parse`, `save`, and `log_note`
through the chain. That is the blast radius of `main`.

## Why the recursive rule, and not `closure()`

The engine has a `closure` operator that computes transitive closure in one line.
As a direct query it works:

```dl
reaches(caller_name, callee_name) <- closure(calls).
? reaches(caller_name, callee_name).
```

But `closure` builds a **view**, not a stored relation, and the moment you try to
*consume* that view inside another rule without pinning an endpoint, the engine
stops you. Save as `04-closure-read.dl`:

```dl
rel seen(path: file).
seen(path) <- scan("src/**/*.rs", path).

rel calls(caller_name: text, callee_name: text).
calls(caller_name, callee_name) <-
    call_edge(caller_sym, callee_sym, _),
    call_name(caller_sym, caller_name),
    call_name(callee_sym, callee_name).

rel reaches(caller_name: text, callee_name: text).
reaches(caller_name, callee_name) <- closure(calls).

rel two_hop(caller_name: text, callee_name: text).
two_hop(caller_name, callee_name) <-
    reaches(caller_name, mid_name), calls(mid_name, callee_name).

? two_hop(caller_name, callee_name).
```

```sh
dl 04-closure-read.dl --no-daemon
```

```
Error: rule 'two_hop' reads closure relation 'reaches' in its body in an unpinned shape; reading a closure from a rule body is only supported when one endpoint is pinned to a literal (seeded reachability), e.g. `h(b) <- reaches(a, b), a = "X".`. An unpinned read would materialize the full closure; query 'reaches' directly instead.
```

An unpinned read of a closure would materialize every reachable pair, which does
not scale. So the engine allows a closure only as a direct query or a
pinned-endpoint read. The recursive rule in `04-reaches.dl` has no such limit: it
is an ordinary stored relation, so `two_hop` could join it freely. Reach for
`closure(edges)` for a one-shot query; write the two-rule recursive form when
downstream rules need to consume the result.

## Exercise

Add a query `? reaches("run", callee_name)` to `04-reaches.dl`. Which functions
does `run` reach? Then flip a query to `? reaches(caller_name, "log_note")` (the
other endpoint pinned): who reaches `log_note`?
