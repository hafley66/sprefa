# Brief: OpenAPI documents load into the dl8 type graph

## 0. Contents
1. Job
2. Base and first action
3. Ownership
4. What exists, with lines
5. Design (decided)
6. Deliverables in order
7. Validation
8. Style laws
9. Reporting

## 1. Job
`dl8 compile <file.dl7> --openapi <doc.json>` installs an OpenAPI 3.x document into the program graph the same way `--tsi` installs a typed symbol index: every `components.schemas` entry becomes a named type node (product, sum, option, list, scalar), every path operation becomes an `openapi.route` row with its request and response types, so a program can conform-check, join, and derive over an API contract at comptime. Inbound only; `dl8 emit openapi` (the v6 `5_emit_openapi.pl` direction) is named as the next lane, not built here.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `feat/v8-openapi-load-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: `v8/src/_4_comptime/_0_load/_8_openapi.rs` (new), `v8/src/_4_comptime/_0_load/mod.rs` (one `pub mod` and one `pub use`), `v8/src/_8_driver/_4_project.rs` and `v8/src/lib.rs` (threading the new stream kind next to `--tsi`), `v8/src/bin/dl8.rs` (the `--openapi` flag on `Compile` only), `v8/Cargo.toml` (`serde_json` is present; add nothing else without a candidate table), `v8/tests/_21_openapi.rs` (new), `v8/fixtures/openapi/**` (new).
Forbidden: `v8/src/_4_comptime/_0_load/{_3_wire,_4_identity,_5_graph}.rs` (read them, change nothing; a needed change is a reported fork), `v8/src/_6_eval/**`, `v8/src/_9_runtime/**`, `v8/src/_5_reify/**`, `v8/oracle/**`, `v7/`, `v6/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| `install_tsi_graph(u, rows, basements, origins) -> Installed`, `tsi_expression_environment` | `v8/src/_4_comptime/_0_load/_5_graph.rs:16`, `:101` |
| the 36 TSI row kinds, newest-run-wins | `_0_load/_3_wire.rs` |
| ids to names, primitives borrow the prelude, call results become applications | `_0_load/_4_identity.rs` |
| `Loaded`, `Installed` | `_0_load/api.rs` |
| `--tsi` threading from the CLI through `compile_project` | `v8/src/bin/dl8.rs` `Compile { tsi }`, `v8/src/lib.rs:161` `compile_project`, `_8_driver/_4_project.rs` |
| the load oracle cases and the TSI contract fixture a program reads structurally | `v8/oracle/load/`, `v8/oracle/compile/sources/test/fixtures/tsi_project/0_contract.dl7`, `v8/fixtures/extract/main.dl7` |
| the type plane: product `(* ...)`, sum, option, list wrappers | `v6/prolog/compile/0_type_plane.pl:153-157` (wrapper inventory), v8 `_2_lower` colon rows |
| absence: option spells value-or-none, JSON schema renders option columns required-and-nullable | `v6/prolog/compile/4_emit_jsonschema.pl:121-146`, `CLAUDE.md` Open: "Absence must be expressible" |
| the outbound direction, for later | `v6/prolog/compile/5_emit_openapi.pl` |
| the target contract, for the fixture | `~/projects/hafley-tsp/examples/todo-app.tsp` (Todo, Filter, Priority, Status, query and mut ops) |

## 5. Design (decided)
```rust
// v8/src/_4_comptime/_0_load/_8_openapi.rs
/// Read one OpenAPI 3.x JSON document into load rows in the SAME row vocabulary
/// `_3_wire.rs` accepts, so `_4_identity.rs` and `_5_graph.rs` install them unchanged.
pub fn openapi_rows(u: &mut Universe, document: &serde_json::Value, origin: TermId) -> Result<Vec<TermId>, Diagnostic>;
```
- Mapping, one row per line, written as a table in the PR and mirrored in a test:
  | OpenAPI | graph |
  |---|---|
  | `components.schemas.X` object with `properties` | product type node named `X`; each property a `(: X prop Type)` column; `required` absent means `option(Type)` |
  | `oneOf` / `anyOf` with a discriminator | sum type node, one variant per branch |
  | `enum` of strings | sum of atoms |
  | `type: array` | `list(Items)` |
  | `string`, `integer`, `number`, `boolean` | prelude primitives `text`, `int`, `float`, `bool` (the `_4_identity.rs` borrow) |
  | `$ref` | the named node; a dangling ref is `openapi_unresolved_ref(path)` |
  | `paths./p.<method>` | `openapi.route(Path, Method, OperationId, RequestType, ResponseType)`; params become `openapi.param(OperationId, Name, In, Type, Required)` |
  | `nullable: true` | `option(T)`; note in the PR that key-absent stays unspellable, per the open row in `CLAUDE.md` |
- The rows enter through the existing `install_tsi_graph` path with a distinct `origin` atom `openapi`; nothing in `_5_graph.rs` changes. If a wire row kind is missing for routes and params, the lane adds `openapi.route` and `openapi.param` as ordinary seed relations on the basement (the `basement_program` door at `_5_graph.rs:82`) and says so.
- Instance lifetime: rows live in the `Universe` for the compile. Storage: none. Uniqueness: two schemas with one name across two documents is `openapi_duplicate_schema(name)`.

## 6. Deliverables in order
1. `openapi_rows` with the scalar, product, option, list arms; `--openapi` threaded; fixture `v8/fixtures/openapi/todo.json` written by hand from `todo-app.tsp` (Todo, Filter, Priority, Status, the query ops); a program `todo.dl7` that declares `(: TodoShape ...)` and derives `(route_returns_todo ?Path ?Method)` by joining `openapi.route` with `Conforms`.
2. Sum arms (`oneOf`, `enum`), `$ref` resolution, the two diagnostics.
3. Real-binary test `_21_openapi.rs`: compile the fixture, assert the route rows and the conform rows byte-exactly against a frozen expected JSON; a second document with a dangling `$ref` asserts the diagnostic.
4. README section.

## 7. Validation
```bash
cd v8 && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src/ | wc -l
git diff --stat origin/main -- v8/oracle | tail -1     # empty
```
Batteries in the background, per-case cap 10 s.

## 8. Style laws
Comment budget. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support. No em dashes. Type names say what they are. Language vocabulary: rxjs, prolog, SQL words. Lang design stays with Chris: a mapping row you cannot place is a fork in the PR, never an invented construct.

## 9. Reporting
PR title `feat(v8): OpenAPI documents load into the type graph`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "openapi load: PR #<n>, cargo test <pass>/<total>, clippy 0, mapping rows <k>, forks <n>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
