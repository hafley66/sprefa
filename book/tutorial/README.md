# The hands-on track: writing dl programs

The numbered chapters in [the book](../README.md) teach the *theory* of the
engine: fixpoints, cycles, incremental maintenance, storage. This track teaches
you to *write* dl programs. You type along against a tiny fixture repo you build
in lesson 0, run each program yourself, and check your output against the block
pasted in the lesson.

Every lesson has the same shape:

- **Goal** in one sentence.
- **The program**: one tiny, complete `.dl` file.
- **Run it**: the exact command.
- **Expected output**: pasted from a real run against the fixture.
- **One exercise**.

The lessons build a ladder. Do them in order. When a lesson leans on an idea the
book explains in depth, it points at the chapter instead of repeating it.

## Reproducibility

Two things keep your output matching the lessons:

- Build the fixture in [lesson 0](00-setup.md) exactly as written. The line
  numbers in later lessons depend on it.
- Run every program with `--no-daemon`. This gives an isolated one-shot run. The
  `[config]` and `[tick]` status lines (repo counts, millisecond timings) vary
  per machine and per run, so the lessons elide them. The `?` result blocks, the
  diagnostics, and the error messages are stable. Those are what you compare.

## The lessons

<!-- BEGIN: tutorial-index -->
0. [Setup](00-setup.md) — install `dl`, build the fixture repo, meet `dl docs` and `dl examples`.
1. [First facts](01-first-facts.md) — a bare `scan`, the `file` relation, the `(repo, path, rev)` coordinate.
2. [First extraction](02-extraction.md) — `match` with a regex capture, then the same thing with `ast`/`sg`; when to use which; metavars and `$$$`.
3. [Join and derive](03-join-and-derived.md) — two source relations joined into a third derived one, and the one-relation-one-rule-kind law shown by triggering the engine's bail.
4. [Graphs for free](04-graphs-for-free.md) — `type_entity` and `call_edge` off the same scan; blast radius with a recursive rule; why you seed a recursive rule instead of reading `closure()` unpinned.
5. [Negation and argmax](05-negation-and-argmax.md) — newest-per-group with the candidate / beaten / winner shape.
6. [Make it a rail](06-make-it-a-rail.md) — `diag` rows, `--check` exit codes, the `--lsp` one-liner.
7. [Make it write](07-make-it-write.md) — a `gen` splice into a markdown file, and the marker discipline.
8. [One winner per key](08-one-winner-per-key.md) — the `key(...) merge(MaxBy(...))` lattice declaration: lesson 5's argmax as one line, then rule priority as dispatch.
9. [Follow the value](09-follow-the-value.md) — the dataflow facts (`df_node`, `df_edge`), `std/flow.dl`'s cross-function `flow_edge`, and a five-line taint walk.
10. [Move a file](10-move-a-file.md) — the `--move` codemod: use-path rewrite, physical rename, and `mod`-decl surgery, always dry-run first.
11. [Suppress a finding](11-suppress-a-finding.md) — `std/suppress.dl`: eslint-style disable comments for your own rails, directive visibility, and the unused-suppression check.
12. [Effects and the daemon](12-effects-and-the-daemon.md) — `sh` effects, `@async`, the clock, and why results only arrive under the daemon.
13. [A server made of rules](13-a-server-made-of-rules.md) — `@in`/`@out` ports and `--mcp`: a JSON-RPC server as three routing rules and a lattice.
14. [Where to go next](14-where-next.md) — the examples browser, the book, the `std/` libs, the reference.
<!-- END: tutorial-index -->
