# v9

A datalog program written as Rust calls, checked into a stratified plan, run by a
differential-dataflow engine over a SQLite store, served over HTTP. Design:
`plans/v9/2026-10-25-v9-ends.plan.md`. Commands below run inside `v9/` with `RCARGO_OFF=1`.

## Ends

| end | input | output | trait or entry | the one impl |
| --- | --- | --- | --- | --- |
| front | Rust calls | `Program` (names) | builder fns `src/_2_ir.rs` (`rel`, `derived`, `request`, `answer`, `rule`, `atom`, `pos`, `neg`, `call`, `v`, `text`, `int`) | Rust builder |
| middle | `Program` | `Plan` (ids, strata) | `check` `src/_3_plan.rs:73`, `stratify` `src/_3_plan.rs:410` | pure functions |
| back | `Vec<Change>` per event | `Delta` | `Engine` `src/_5_ends.rs:49` | `Dd` `src/_6_back/_1_dd.rs` |
| store | `Delta`, fresh dict bytes | rows, SQL answers | `Store` `src/_5_ends.rs:55` | `Sqlite` `src/_7_store/_1_sqlite.rs` |
| runtime | commits, service answers | deltas, query tables | `Core` `src/_8_runtime/_3_core.rs`, `IService` `src/_5_ends.rs:70` | `Ryi` `src/_8_runtime/_2_ryi.rs` |
| wire | HTTP | runtime calls | `router` `src/_9_wire/http_auto.rs:73`, generated from `schema/0_wire.tsp` | axum, served by `Handle` `src/_8_runtime/_4_handle.rs` |

The recipe picks each impl: `AnyEngine` and `AnyStore` in `src/_8_runtime/_1_any.rs`, matched in
`build` (`src/_8_runtime/_3_core.rs`).

## The `V9_SYSTEM` recipe

`V9_SYSTEM` is read by `v9_system` (`src/_1_effects.rs:53`) and parsed by `Recipe::parse`
(`src/_4_recipe.rs:39`). Unset is the default recipe.

| key | allowed values | default |
| --- | --- | --- |
| `engine` | `dd` | `dd` |
| `store` | `sqlite` | `sqlite` |
| `wire` | `http` | `http` |

- Parts split on `,` or `;`, each trimmed and lowercased; empty parts are skipped; a missing key
  takes its default (`src/_4_recipe.rs:39`).
- Canonical form: every key, sorted, with its value: `engine=dd,store=sqlite,wire=http`
  (`Recipe::canonical`, `src/_4_recipe.rs:101`). `GET /recipe` returns it.
- A bad recipe is `Diag::Recipe` naming the problem and the allowed set: unknown key
  (`src/_4_recipe.rs:90`), unknown value (`src/_4_recipe.rs:73`, `:79`, `:85`), duplicate key
  (`:58`), no `=` (`:50`). The process prints it and exits 1:

  ```sh
  V9_SYSTEM=engine=duckdb cargo run -q --example 0_reach -- r.db
  # 0_reach: [Recipe { problem: "unknown value", found: "engine=duckdb", allowed: ["dd"] }]
  ```

- The store records the canonical recipe and the plan hash in `meta`. Reopening a db under another
  recipe or another program is `Diag::Mismatch` and creates nothing (`src/_7_store/_1_sqlite.rs:47`):

  ```sh
  cargo run -q --example 1_negation_add -- r.db   # r.db written by 0_reach
  # 1_negation_add: [Mismatch { stored_recipe: "engine=dd,store=sqlite,wire=http", ... }]
  ```

## Writing and running a program

A program is a `fn program() -> Program` in any Rust file plus the services its Request rels name.
The entry is `src/_8_runtime/_5_run.rs`:

| fn | does |
| --- | --- |
| `run(program, services, db)` | recipe from `V9_SYSTEM`, `build` with `BOUNDS` (`_6_back::_1_dd::MAX_ROUNDS`); returns the `Core` |
| `serve(name, program, services, db, port)` | the same build on the Core thread, then the wire `recipe.wire` names; `wire=http` serves the HTTP router on `127.0.0.1:port`; `v9 serve` calls it (`src/bin/v9.rs:48`) |
| `views(core)` | every `<rel>_text` view as `rel(cell, ..)` lines |
| `main(name, program, services)` | `<DB> [--port N]`: with a port, `serve`; without, `run` and print `views` |

The examples are complete programs. Each Rust form sits under the lisp form it stands for; no text
front reads the lisp forms yet.

| example | shows | run |
| --- | --- | --- |
| `examples/0_reach.rs` | Base `edge`, recursive `reach`; rows come from `POST /commit` | `cargo run --example 0_reach -- DB --port 7801` |
| `examples/1_negation_add.rs` | negation of a Base rel and of a Derived rel (two strata), `add`, `lt`, `neq` | `cargo run --example 1_negation_add -- DB --port 7802` |
| `examples/2_service.rs` | Request `ask_len`, Answer `told_len`, an in-file `IService` with `Cadence::Once` and `seed` | `cargo run --example 2_service -- DB --port 7803` |

`tests/8_examples.rs` runs each example's program through `run` and snapshots its views
(`tests/snapshots/8_examples__*.snap`): `cargo test --test 8_examples`.

### Services

`IService` (`src/_5_ends.rs:70`): `call` takes every request change of one event, `poll` returns
answer changes and never blocks. Two methods have defaults:

- `seed(answers)`: the stored Answer rows, once, at open (`src/_8_runtime/_3_core.rs:141`). A
  service that answers a re-ask with a difference starts its memory here (`Ryi`, `Len` in
  `examples/2_service.rs`).
- `errors()`: drained after every `poll`; the first error stops the commit with `Diag::Service`
  (`src/_8_runtime/_3_core.rs:348`).

`Core::refresh(service)` (`src/_8_runtime/_3_core.rs:226`) re-asks every stored Request row of a
Once service and commits what it polls; `None` when nothing changed. `v9 reach` calls it on a
rerun of a stored root (`src/bin/v9.rs:196`).

### Built-in programs

`v9 serve [PROGRAM] DB --port N` serves a program from `PROGRAMS`
(`src/_8_runtime/_0_programs.rs:8`; only `reach`). `v9 reach ROOT DB [--check]` runs `reach` with
the `ryi` service (`V9_RYI`, `src/_1_effects.rs:58`) and checks it against a plain Rust closure:

```sh
cargo run -q --bin v9 -- reach ~/projects/hafley-rs/crates/hafley_scm scm.db --check
```

## HTTP routes

Contract: `schema/0_wire.tsp:104`-`111`; generated OpenAPI: `schema/openapi.yaml`; generator:
`schema/1_generate.mjs` (`pnpm generate` in `schema/`). The generated Rust is `src/_9_wire/`.

| route | body | answer | client error |
| --- | --- | --- | --- |
| `POST /commit` | `[{"rel", "cells", "diff"}]`, Base rels only | `Delta { event, changes }` over every rel | 400 `CommitRefused`, 422 `BodyRejected` |
| `POST /query` | `{"sql"}`, one read-only SELECT over the rel tables and `<rel>_text` views | `Table { columns, rows }` | 400 `SqlRejected`, 422 `BodyRejected` |
| `GET /plan` | | `Plan { relations }` | |
| `GET /recipe` | | `Recipe { canonical }` | |

Every route answers a server failure with 500 `ServerError { error }`. The Diag to model map is
`commit_failed` (`src/_8_runtime/_4_handle.rs:143`) and `query_failed` (`:163`): on a query every
`Diag::Store` is 400 `SqlRejected`, a SQL value with no wire form (a blob) included; on a commit
`Diag::Commit` and the check Diags are 400 `CommitRefused`. A body the JSON extractor cannot decode
is 422 `BodyRejected` with axum's rejection text (`src/_9_wire/http_auto.rs`, generated).
`tests/6_wire.rs` hits each model over a real port.

```sh
curl -s -X POST localhost:7801/commit -H 'content-type: application/json' \
  -d '[{"rel":"edge","cells":["a","b"],"diff":1},{"rel":"edge","cells":["b","c"],"diff":1}]'
curl -s -X POST localhost:7803/query -H 'content-type: application/json' \
  -d '{"sql":"SELECT * FROM long_text"}'
```

## Not built yet

| what | where it stops |
| --- | --- |
| text front (lisp forms to `Program`) | no parser; programs are Rust (`src/_8_runtime/_0_programs.rs:1`, `src/_2_ir.rs:1`) |
| a second engine | `Recipe::parse` takes only `dd` (`src/_4_recipe.rs:73`) |
| a second store | only `sqlite` (`src/_4_recipe.rs:79`) |
| a second wire | only `http` (`src/_4_recipe.rs:85`) |
| a program by name not in `PROGRAMS` under `v9 serve` | `src/bin/v9.rs:69`; write it as an example-shaped file instead |
| `Core::refresh` over HTTP | no route in `schema/0_wire.tsp` |
| a Continuing service | none exists; `Core::tick` has no ticker (`plans/v9/SLOP.md`, P4 rows) |
