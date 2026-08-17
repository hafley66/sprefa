# OpenAPI spec -> dl6 -> clap CLI + HTTP-over-UDS server

Research lane, no code landed. Epic card `issues/openapi-clap-uds-lab/item.md`.
Tree `7d22a3cbf5ca95bce62680e0c52593c13b033486`, measured 2026-08-17.

## TOC

1. [Receipts](#1-receipts)
2. [Card corrections](#2-card-corrections)
3. [Q1 candidates: OpenAPI parsing in Rust](#3-q1-candidates-openapi-parsing-in-rust)
4. [Q1b candidates: YAML to JSON](#4-q1b-candidates-yaml-to-json)
5. [Q2 candidates: clap tree from a spec](#5-q2-candidates-clap-tree-from-a-spec)
6. [Q3 candidates: HTTP over a Unix domain socket](#6-q3-candidates-http-over-a-unix-domain-socket)
7. [Q4: where dl6 sits](#7-q4-where-dl6-sits)
8. [Prior art in tree](#8-prior-art-in-tree)
9. [The dl6 rel schema](#9-the-dl6-rel-schema)
10. [Construct census per rule](#10-construct-census-per-rule)
11. [Pipeline](#11-pipeline)
12. [Minimal lab plan](#12-minimal-lab-plan)
13. [Open forks for Chris](#13-open-forks-for-chris)

## 0. Decision 2026-08-17: dl6 is the thing (F0, chosen)

User decision, supersedes F1 and F5 below and the lab binary in section 12.
No clap generation, no OpenAPI crate, no new emitter, no `sprefa-openapi-lab`
crate. The dl6 program IS the API and the CLI is a generic client of it.

| layer | how |
|---|---|
| spec -> rels | the dl6 rules of sections 9-10 (compile today, receipts in section 1) |
| API | `sprefa-engine-rs` grows a `serve` seam: axum 0.8 router on `tokio::net::UnixListener`, copied from v5 `src/daemon/shell/http.rs:117-142`; `GET /rel/<name>` reads rows typed by the emitted `.types.rs`, `POST /arrive` folds a tick through `driver.rs` |
| CLI | one generic binary that reads `cli_verb`/`cli_arg` rows from the running socket and dispatches; `--help` is a rel read |
| upstream calls (`pokemon get 25` reaches pokeapi) | one host row per operation, from->to like every `sh` host |
| yaml -> json | new extract family `yaml`, hosted through the existing extract executor on both doors (user 2026-08-17) |

rx lowering of the server: `arrivals$.pipe(concatMap(batch => driver.tick(batch)))`; reads are a `latest` projection.

Order: (1) all 342 `.types.rs` compile (audit finding 2, S); (2) `serve` seam (M); (3) Rust golden: pokeapi program on a socket, `curl --unix-socket` one rel, byte-diff (M). Card: `issues/engine-rs-serve-uds`.

## 1. Receipts

Every row below was produced by the command in it, in this worktree, on this tree.

| fact | command | result |
|---|---|---|
| spec size | `wc -l v6/dl/fixtures/pokeapi.openapi.yml` | 9839 lines, 274745 bytes, `openapi: 3.1.0` |
| operations in spec | `grep -c operationId:` | 100, all `get` |
| parameters in spec | `grep -c '\bin: '` | 196 (146 query, 50 path) |
| `$ref` occurrences | `grep -c '\$ref:'` | 355 |
| component schemas | json1 `json_each($.components.schemas)` | 212 |
| yaml -> json | new extract family `yaml` (`lang/mod.rs sources()`), hosted on both doors through the existing extract path (TS: spawn per digest, Rust: in-process) | user 2026-08-17 |
| fixture compile coverage | `python3` over `v6/prolog/compile/out/manifest.json` | `compiled 342`, `unsupported 110`, total 452 |
| proposed rel schema compiles | `bash v6/prolog/compile/scripts/compile_dl6.sh /tmp/oa-research/openapi_spec.dl6 out.ts` | rc=0, `total=108/724001` (108ms) |
| risky-construct probe compiles | same, `/tmp/oa-research/probe2.dl6` | rc=0, `total=78/900993` (78ms) |
| emitted plan run on the real spec | emitted `spec_parameter` SQL against `pokeapi.json` in sqlite3 3.43.2 | 196 rows, first row `('/api/v2/ability/','get','limit','query','integer')` |
| 200-response `$ref` rows | emitted `status_schema` SQL | 98 of 100 operations |
| the target operation | same | `('/api/v2/pokemon/{id}/','get','#/components/schemas/PokemonDetail')` |
| every emitted relation table | `grep 'CREATE TABLE' out.ts` | every table opens `"__id" INTEGER PRIMARY KEY`; every `text` column lowers to `INTEGER NOT NULL` into `__str` |

`/tmp/oa-research/` holds the two probe `.dl6` files, the converted json, and the
SQL receipt script. Nothing was written under the repo except the two plan docs.

## 2. Card corrections

Four claims on the epic card and in tree docs do not hold on this tree.

| claim | where | correction |
|---|---|---|
| prior art at `sprefa-lanes/pokeapi.openapi.yml` and `dl/fixtures/pokeapi_shape.dl6` | epic card | both live under `v6/`: `v6/dl/fixtures/pokeapi.openapi.yml`, `v6/dl/fixtures/pokeapi_shape.dl6`. `sprefa-lanes/` holds only BRIEF files |
| "`5_emit_openapi.pl` already emits OpenAPI from dl6 programs" | epic card | half. `v6/prolog/compile/5_emit_openapi.pl:88-99` hard-codes the served engine's own six routes as `api_route/5` facts. Only `components.schemas` comes from the program, through `4_emit_jsonschema.pl:module_defs/4`. Paths are a fixed table, not derived from any rel |
| "`docs/effect-inventory.md` (uds.rs sites)" | epic card | `src/daemon/shell/uds.rs` no longer exists. `ls src/daemon/shell/` is `http.rs mod.rs timers.rs watch.rs`; the UDS listener folded into `src/daemon/shell/http.rs:117-142` (`spawn_uds`). `docs/effect-inventory.md:90,354,356` still names the deleted file |
| "There is no string aggregate in the registry (count/sum/min/max/avg only), so the N lines of a zone cannot be folded into one column and handed to one command" | `v6/tsv2/labs/staged-writes/2-apply.dl6:20-26` | stale. `group_concat/1` and `group_concat/2` are `live` at `registry.pl` (CONSTRUCT-REFERENCE row), `ordered_group_concat_value` and `ordered_group_concat_ordinal` are `compiled` in the manifest, and `v6/dl/typegen/render_rust.dl6` uses `group_concat(LineText, '\n', Ordinal)` today. Probe F below compiles the fold-to-one-column-then-one-write-host shape rc=0 |
| "string split/substr primitive" awaiting the user | `CLAUDE.md`, Awaiting user word | stale. `split/2` at `registry.pl:294`, `substr/2` at `:285`, `substr/3` at `:286`, `instr/2` at `:287`, `length/1` at `:288`, all with `expression/5` rows and typed lowering. Ten `split_*` manifest fixtures, all `compiled` |

## 3. Q1 candidates: OpenAPI parsing in Rust

Requirement set: read OpenAPI **3.1** (the corpus spec is `3.1.0`), preserve
document order, expose paths x methods x parameters x responses x component
schemas, and round-trip if the loop is to close both directions.

| crate | latest (date) | license | what it gives | what it lacks | verdict |
|---|---|---|---|---|---|
| `oas3` | 0.22.0 (2026-05-06) | MIT | `oas3::from_yaml(String) -> Result<Spec, Error>` and `oas3::from_json`; 3.1.x model; order-preserving `oas3::Map` for paths, component maps, schema properties, responses, callbacks; `yaml-spec` feature parses YAML directly; MSRV 1.87; 429k recent downloads | smaller ecosystem than `openapiv3`; README warns 3.0.x specs "may have trouble" parsing | **WINNER** if a typed model is wanted at all. Only candidate that reads the corpus spec's own version |
| `openapiv3` | 2.2.0 (2025-06-02) | MIT/Apache-2.0 | `serde_json::from_str::<openapiv3::OpenAPI>(data)`; clean enum-per-schema-kind model; 2.87M recent downloads, the field's default | README: "Note this does not cover OpenAPI v3.1 which was an incompatible change." Non-goal, verbatim: "Deserialization and subsequent re-serialization are 100% the same" | rejected on the corpus. The spec in tree is 3.1.0 |
| `progenitor` | 0.14.0 (2026-04-24) | MPL-2.0 | full client codegen; `Generator::generate_tokens(&spec)`; `Generator::cli()` emits a clap CLI; `Generator::httpmock()`; 1.30M recent downloads (Oxide) | spec type is `openapiv3::OpenAPI`, so 3.0.x only. Generated client is `reqwest`-based, and `cargo info reqwest` 0.13.4 lists no unix feature (`socks` yes, unix absent), so the generated client cannot reach a UDS socket. MPL-2.0 | rejected for this epic on two independent counts. Its `cli` generator is the shape to copy, not the crate to take |
| `utoipa` | 5.5.0 (2026-05-04) | MIT/Apache-2.0 | 12.9M recent downloads; `#[derive(ToSchema)]` + `#[utoipa::path]` -> an `OpenApi` document at compile time; adapters for axum/actix/rocket | emit side only. There is no spec-in path: the Rust types are the source and the document is the output. Exactly backwards for "spec in" | rejected for parsing. Candidate for the OTHER direction if the epic ever wants a second emitter beside `5_emit_openapi.pl` |
| `paperclip` | 0.9.7 (2026-04-20) | MIT/Apache-2.0 | 72k recent downloads; v2 (Swagger) + v3 models, actix macros, a client generator | anchored on actix; 43 deactivated features; the v3 half trails the v2 half; no UDS story anywhere | rejected. Framework-anchored to a stack this repo does not run |
| `okapi` | 0.7.0 (2024-01-14) | MIT | plain structs for OpenAPI documents, `schemars`-backed | dormant 2+ years; Rocket-oriented (`rocket_okapi`); 3.0 model | rejected on cadence |
| `apistos` | 1.0.0-pre-release.14 (2026-06-08) | MIT/Apache-2.0 | actix-web OAS3 documentation generator | pre-release for 14 iterations; actix-anchored; emit side | rejected |
| `openapi` | 0.1.5 (2017-04-30) | n/a | n/a | last release 2017; `openapiv3`'s own README lists it as the "similar crate" it replaced | rejected on cadence |
| `openapi-generator` (java) | 7.x | Apache-2.0 | ~50 language generators, mature templates | a JVM in the build path, Mustache templates as the extension surface, and it generates a whole client crate that the dl6 rules would then have to read back. Contradicts "one spec, rules derive everything" | rejected. A second code generator beside dl6 is a second source of truth |

**Verdict.** For the dl6 route, no OpenAPI model crate is on the critical path.
The spec IS a JSON document and dl6's json plane reads it directly
(section 7). `oas3 0.22.0` is the pick for the one job a model crate is
still good for: a **validation gate** in front of the pipeline that answers
"is this a well-formed 3.1 document" before any rule runs. Its exact call is
`oas3::from_yaml(std::fs::read_to_string(path)?)`, one line, `yaml-spec`
feature on.

## 4. Q1b candidates: YAML to JSON

The dl6 route needs only this. yaml is a sprefa-extract family, hosted on
both doors through the existing extract executor (user 2026-08-17).

| candidate | latest (date) | what it gives | what it lacks | verdict |
|---|---|---|---|---|
| `serde-saphyr` 1.1.0 (2026-08-15) inside sprefa-extract as family `yaml` | YAML 1.2, serde, aliases; one `doc` record per file; hosted on both doors through the existing extract executor (TS spawn per digest, Rust in-process) | a new family in `lang/mod.rs` | **WINNER** (user 2026-08-17: sprefa-extract hosts file-type work in every env) |
| system `ruby -ryaml -rjson` | ships with macOS today | one line, zero crate | an OS interpreter as a runtime dep of a Rust engine; Apple has deprecated scripting runtimes and CI images differ | rejected (user decision 2026-08-17) |
| `serde_yaml` | 0.9.34**+deprecated** (2024-03-25) | 86.8M recent downloads, dtolnay | the version string is literally `+deprecated`; upstream retired it | rejected. Adding a self-declared deprecated crate is a defect |
| `serde_norway` | 0.9.42 (2024-12-21) | maintained fork of `serde_yaml`, 2.58M recent, API-compatible | fork governance; a Rust dep for a job a one-line host does | second choice if `serde-saphyr` misbehaves on the corpus; API-compatible with the retired crate. Only if the pipeline moves inside a Rust binary |
| `serde_yaml_ng` | 0.10.0 (2024-05-26) | second maintained fork, 4.84M recent | two competing forks of the same dead crate is an ecosystem fork risk | third choice |
| `yq` (CLI) | n/a | `yq -o=json` | not installed on this machine (`which yq` empty); two incompatible `yq` projects (python and go) share the name | rejected on availability |
| `python3 -c 'import yaml'` | n/a | matches the existing `toml_json` host body style | `python3 -c "import yaml"` fails on this machine: PyYAML is not stdlib. The toml case works only because `tomllib` IS stdlib | rejected, measured |

## 5. Q2 candidates: clap tree from a spec

Requirement set: one verb per operation, args from parameters, help text from
`summary`/`description`, the tree shaped by DATA that arrives at runtime (spec
rows), no second parser, clap 4 (the version this repo already runs at
`v6/sprefa-extract/Cargo.toml:123-126` and root `Cargo.toml:94`).

| candidate | latest (date) | what it gives | what it lacks | verdict |
|---|---|---|---|---|
| `clap` 4 **builder** API at runtime | 4.6.6 (2026-08-06), 214.9M recent | `Command::new(name)`, `Command::subcommands(impl IntoIterator<Item = impl Into<Command>>)` (`clap_builder-4.6.6/src/builder/command.rs:558`), `Command::args(impl IntoIterator<Item = impl Into<Arg>>)` (`:207`), `Arg::new(id).long(..)` (`arg.rs:228`) `.required(bool)` (`:755`) `.value_parser(..)` (`:1048`) `.action(..)` (`:986`), `Command::try_get_matches_from` (`command.rs:813`), `ArgMatches::subcommand() -> Option<(&str, &ArgMatches)>` (`arg_matches.rs:922`), `ArgMatches::get_one::<T>(id)` (`:118`) | nothing. The two iterator-taking builders ARE the fold over spec rows | **WINNER.** The library is bought; the code that remains is one `.fold()` over `cli_verb` rows and one over `cli_arg` rows. No codegen, no build script, no second crate |
| `clap` 4 **derive** + generated Rust | same crate | compile-time checked structs, the shape `v6/sprefa-extract/src/bin/extract.rs:40` uses today | the verb set becomes a compile-time constant. A spec edit needs a regenerate-and-rebuild cycle, and the generated file becomes a second artifact to keep in sync. It also needs a NEW dl6 emitter to write the `#[derive(Parser)]` text | runner-up. Take it only if a static binary with no spec at runtime is a requirement |
| `clap-serde` | 0.5.1 (**2022-10-01**) | `serde_json::from_str::<clap_serde::CommandWrap>(json)?.into() -> clap::Command`: the literal "a JSON document IS the command tree" API | `cargo` dependency list for 0.5.1: `clap ^3.2.16`. It would pull a SECOND clap major into a clap-4 tree. Dormant 3 years and 10 months, 19.9k recent downloads | rejected with evidence. Same class as the `rouille`/`tiny_http` rejections in `plans/2026-07-18-infra-library-adoption.md:181-186` |
| `clap_complete` | 4.6.9 (2026-08-06), 25.1M recent | `clap_complete::aot::generate<G: Generator, S: Into<String>>(generator: G, cmd: &mut Command, bin_name: S, buf: &mut dyn Write)` (`clap_complete-4.6.9/src/aot/generator/mod.rs:284`) | nothing; it takes the same `&mut Command` the builder produced | **ADOPT alongside.** Shell completions for the generated verb tree cost one call. `clap_mangen` 0.3.3 is the man-page twin |
| `progenitor` `Generator::cli()` | 0.14.0 | a whole generated CLI crate from the spec | build.rs only; `openapiv3` 3.0.x model; `reqwest` client with no unix transport | rejected, same two counts as section 3 |
| a maintained serde-to-clap-4 crate | none found | n/a | crates.io search for `clap dynamic runtime command` returned `veks-completion`, `clap-dyn-autocomplete`, `flag-rs`, `rclap`, `abscissa`; none deserializes a clap 4 `Command` | no candidate exists. The builder fold is the answer |

## 6. Q3 candidates: HTTP over a Unix domain socket

**This question is already answered and shipped in this repo.** The written
candidate-by-candidate analysis is `plans/2026-07-18-infra-library-adoption.md`
section 2 (ten candidates: axum, actix-web, poem, warp, rouille, tiny_http,
hyper direct, salvo, tarpc, jsonrpsee), the verdict is section 2.4, and the code
is `src/daemon/shell/http.rs`. Re-measured 2026-08-17:

| leg | crate | latest (date) | the exact call in tree | verdict |
|---|---|---|---|---|
| server | `axum` 0.8 | 0.8.9 (2026-04-14), 106.5M recent | `axum::serve(tokio::net::UnixListener::from_std(std_listener)?, app).with_graceful_shutdown(async move { cancel.cancelled().await })` at `src/daemon/shell/http.rs:128-137`. One `Router` (`:63-71`), two thin listeners, one `CancellationToken` | **HOLDS.** `tokio::net::UnixListener` implements `axum::serve::Listener` in 0.8 |
| path syntax | `axum` 0.8 | same | axum 0.8 path parameters are `"/users/{id}"` (`axum-0.8.9/src/extract/path/mod.rs:610,630`), byte-identical to OpenAPI's own path template. `/api/v2/pokemon/{id}/` needs ZERO transform | new finding. Removes a whole rewrite rule from the pipeline |
| client | `hyper` 1 + `hyper-util` | 1.11.0 (2026-07-20) / 0.1 | `hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(tokio::net::UnixStream::connect(sock).await?))` at `src/daemon_client.rs:47-53`, then `SendRequest::send_request(req)` at `:83` | **HOLDS.** One non-pooled connection per client needs no connector or URI layer |
| client, rejected | `hyperlocal` | 0.9.1 (2024-07-22), 15.0M recent | the `Uri::new(socket, path)` + connector layer | rejected in the 2.5 open question and again here: the connector/URI layer buys nothing for one connection, and 2024-07 is the last release |
| client, rejected | `reqwest` | 0.13.4 (2026-05-25) | n/a | `cargo info reqwest` feature list has `socks` and no unix/uds feature at all. It cannot reach a UDS socket. This is also what disqualifies progenitor's generated client |
| listener flavor | `tokio-listener` | 0.5.2 (2025-09-11), 17.3k recent | `axum08` feature, one runtime-selectable listener across tcp/unix/inetd/systemd | OPTIONAL. Named as optional in the 2.4 verdict and still optional. 17.3k recent downloads is thin; two `axum::serve` calls cost less than the dependency |
| middleware | `tower` / `tower-http` | 0.5.3 / 0.6 | `tower_http::trace::TraceLayer` for the request span, already wired | **HOLDS** |
| v6 TS side | node `http` | n/a | `v6/tsv2/serve/4_http.ts:493` calls `server.listen(port, ...)`. Node's `net.Server.listen` takes a PATH for a UDS socket with no new dependency | one-line change if the tsv2 runtime is to serve over UDS too |

**Verdict.** Nothing to decide. axum 0.8 server + raw hyper 1 client over
`UnixStream`, exactly as `src/daemon/shell/http.rs` and `src/daemon_client.rs`
already do it. The v6 lab copies those two files' shape.

## 7. Q4: where dl6 sits

The card proposes `sh`/`json_each` for the EDB rows. **`json_each/2` is not the
door.** `registry.pl:86` reads:

```prolog
surface(json_each/2,    guard,     no_refs,  wrapper(expr_pair, refuse(goal)),  refused).
```

and the manifest agrees: fixture `json_each_fans_out` is `unsupported` with
reason `level_body_goal(repo_lang(A),json_each(B,A))`.

The door that IS live is `decode/2` (`registry.pl:85`) with the json value axis
(`'{}'/1` at `:129`, `spread/1` at `:131`, `'**'/0` at `:133`), and its lowering
IS `json_each`: `v6/prolog/conformance/fixtures/json_arm.pl:94` states "the
lowering is json_each's own (key,value) columns". The user surface is `decode`;
`json_each` is what the compiler writes.

The corpus already holds the exact fixture this epic needs, and it is
`compiled`:

```prolog
% v6/prolog/conformance/fixtures/json_arm.pl:105-119
% Two key holes nested: v4/examples/openapi-cardinality-markdown.sprf, the
% path x method fan-out.
(operation(Path, Method, Id) <-
   spec(Body),
   decode(Body, {paths: {$Path: {$Method: {operationId: Id}}}}))
```

The four planes, and where each already lives:

| plane | mechanism | status |
|---|---|---|
| spec file -> json | `sh yaml_json(path, digest) -> (doc: json)` | live. Same shape as `toml_json` in the config golden |
| json -> spec rows | `decode/2` + key capture `$Var` + `spread/1` | live, `compiled`, and the fixture is literally the OpenAPI fan-out |
| spec rows -> clap tree, router table | ordinary level rules, `replace/3`, `concat/1`, `instr/2` | live |
| rows -> emitted text | `field_line` -> `group_concat(..., ordinal)` -> `concat` wrap, the `v6/dl/typegen/render_rust.dl6` three-rule IR shape | live, gated by `v6/prolog/compile/test/typegen_golden.sh` |

**No new construct is needed for the dl6 half of this epic.** That is a measured
statement, not an assertion: sections 9 and 10 compile.

## 8. Prior art in tree

| path | what it already does | what this epic takes |
|---|---|---|
| `v6/dl/fixtures/pokeapi.openapi.yml` | the corpus spec, 3.1.0, 100 operations, 196 parameters, 212 component schemas | the lab input, unchanged |
| `v6/dl/fixtures/pokeapi_shape.dl6` | 216 lines of `rel <name>_detail(...)`/`_summary(...)` hand-derived from the spec's component schemas; nested/recursive shapes fall back to `json` columns | the target shape the schema plane must reproduce, and the evidence that the component half is already writable as rels |
| `v6/dl/typegen/render_rust.dl6` | dl6 renders Rust structs from `type_row/7` arrivals: `leaf_type` -> `field_line` -> `body_text(group_concat(...))` -> `rendered_type` | the emitter pattern, verbatim. A `render_clap.dl6` is the same three strata |
| `v6/dl/typegen/render_ts.dl6` | the TS twin | same |
| `v6/prolog/compile/typegen_export.pl` | `dump_type_rows/2`, `row/11 -> type_row/7`, the JSONL IR the render doors consume | the IR-export pattern if the clap tree ever needs compile-time rows |
| `v6/prolog/compile/test/typegen_golden.sh` | runs a `.dl6` renderer on the real tsv2 runtime with JSONL arrivals, assembles `rendered_type` rows into a file, diffs against goldens | the gate shape for `render_clap.dl6` |
| `v6/prolog/compile/5_emit_openapi.pl` | emits OpenAPI 3.1.0 for the served engine's six hard-coded routes; schemas from the program's types | the OTHER direction, and the loop-closing diff target |
| `v6/prolog/compile/4_emit_jsonschema.pl` | `module_defs/4`; option columns render as `anyOf` with null (`:121-146`) | the schema plane's existing renderer |
| `src/daemon/shell/http.rs` | ONE axum `Router` over TCP and UDS, SSE, `TraceLayer`, one shutdown token | the server half, copied into the v6 lab |
| `src/daemon_client.rs` | hyper 1 client-conn over `UnixStream`, sync wrapper, watched wait | the client half |
| `plans/2026-07-18-infra-library-adoption.md` section 2 | the ten-candidate HTTP/UDS analysis and the axum 0.8 verdict | Q3, closed |
| `v6/tsv2/labs/staged-writes/2-apply.dl6` | staged edits as rows, `armed(zone)` as the human yes, one `sh put_line` per line | the write seam, with its "no string aggregate" limit corrected |
| `docs/bootstrap-typegen-lab-vs-typespec.md` | the retired bootstrap lab emitted Rust structs, a std-only Rust HTTP server, typed runtime path matchers, and a JS fetch client from one schema | proof the one-spec-many-emitters shape ran end to end once; its server was hand-written std, which is what "infra is bought" now forbids |
| `plans/2026-08-16-typespec-parity-typegen.PLAN.md` | the gap census; confirms the dl6 render doors and the `type_row/7` IR | the arc-sequencing precedent |

## 9. The dl6 rel schema

Declarations only. Compiles rc=0 (receipt row 8 in section 1).

**Design correction, measured.** The surrogate-key law does not need a
hand-declared `_id: int` column and does not want `key(...)` on a derived rel.
Two receipts:

1. Every emitted relation table already opens `"__id" INTEGER PRIMARY KEY`, and
   every `text` column lowers to `INTEGER NOT NULL` referencing
   `__str ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`. The
   emitted DDL for the schema below carries **zero TEXT columns in any relation
   table**. The dictionary encoding the law asks for is the lowering's own work.
2. `key(...)` on a rel that a level rule derives is not built yet:
   `keyed_level_head(spec_doc/2)` came back from
   `compile_dl6.sh` on the first draft, and the manifest carries the same name
   for `current_value/2`. `key(...)` belongs on the EDB and dictionary rels.

```dl6
# ── the source ──────────────────────────────────────────────────────────────
rel spec_file(spec_id: int, path: text, digest: text) key(1).

sh extract(path: text, digest: text) -> (doc: json) =
  `"$DL_EXTRACT_BIN" --family yaml {path}`.
# hosted extract on both doors: TS spawns it (serve/1_hosts.ts:253
# runSprefaExtract -> runShellLine), Rust links it (hosts.rs
# SprefaExtractExecutor). yaml is a new extract family (lang/mod.rs
# sources()); userland never calls a CLI, this is the engine's own host spelling.
