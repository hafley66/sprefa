# Influences

Source: chat 2026-04-18, author quote.

> "yes we are trying to be bash prolog ast-grep rxjs sql adjacent bc
> im crazy and these are my favorite things"
>
> "also css and react and redux-sagas"

Eight adjacencies. The author confirmed the set; the per-feature mapping
below is reader inference, not author statement. Preserved here so future
contributors can read the surface and recognize where each tradition shows
up.

## bash

- Sigil family with required braces (`${X}`) for disambiguation against
  adjacent text.
- Heredoc-style pattern bodies (`<<...>>` floated; not locked).
- Op-position bare idents read as command names.

## css (author-confirmed)

- Pipe operator `>` is the CSS child combinator. Reads "select within"
  rather than the bash redirect "send to." Composing op blocks reads
  like nesting selectors, not like piping bytes.

## prolog

- Term unification as the binding model. Decl introduces a term; Ref
  resolves against the in-scope decl set.
- Rules as named, reusable, composable units (`rule X = ...`).
- Cross-rule references (xref) treat rules as a query namespace.

## ast-grep

- Pattern bodies use ast-grep's metavar syntax verbatim (`$NAME`,
  `$$$NAME`) inside `ast[lang](<...>)` op bodies.
- Op blocks lower to ast-grep's existing YAML rule combinators (`all`,
  `any`, `not`, `inside`, `has`, `follows`, `precedes`).
- Sub-lang capture lift preserves arity sigil at the host boundary.

## rxjs

- Pipeline composition as a stream of cursors flowing through ops.
- Fork (`;`) as multicast: parent distributes to multiple downstream arms.
- Pipe (`>`) as serial composition.
- Op trait surface modeled around `pipe()` returning
  `BoxStream<Arc<[Cursor]>>`.

## sql

- Result store as the persistence layer; queries are first-class
  consumers of pipeline output.
- Per-rule tables (per `project_query_redesign` memory).
- "End of day creates SQLite rows" framing for `$` decls — terms are
  bindings that materialize as table columns.

## react

- Op blocks compose into a tree the way components do.
- Hover/diagnostics dispatched by node kind, similar to per-component
  rendering.

## redux-sagas

- Effect descriptions yielded by ops; an interpreter executes them
  later (per `project_v2_effects_split` memory: reads are pipe ops,
  writes/edits become deferred effects flushed after pipeline drain).
- Maps to the `sprf-effect-runtime` skill direction in the research
  notes.
