# Import arc brief

Lane branch `feat/import-std-alias`. Base `origin/main` `81af4484081308a1ac2f8c8bf00a8b40cbc1f9fe`
(or newer; coordinator states the sha at spawn). First action:
`git merge --ff-only <sha>`; failure = stop and report.

## Contents

1. Goal
2. Decisions this lane implements (read `AGENTS.md:76-107` first)
3. Before / after
4. Files owned
5. Files forbidden
6. Steps, in order, each with its receipt
7. Gate
8. Laws
9. Report format

## 1. Goal

`(api: (import "@std/oai"))` binds a module node; `api.route` walks into it by
dot; the loader-minted `openapi.*` names are gone; `tests/_21_openapi.rs` is
green; `dl8 compile --openapi` no longer exists.

## 2. Decisions (all in `AGENTS.md`, dated 2026-09-17 unless noted)

| row | decision |
|---|---|
| `:80` | import form is `(<name>: (import "<path>"))` only; `import` is a comptime call returning a module node; no bare-name splice |
| `:96` | `@std/<name>` is the one alias; every other path is ordinary, from the compiler cwd |
| `:97` | std modules own their seeds: `(oai.document "todo.json")` inside `@std/oai.dl7`; no CLI flag; namespace `oai` |
| `:98` | `import_cycle` is a diagnostic row |
| `:101` | hosted namespaces: `fs.`, `git.`, `oai.`, `env.`; no user-facing `soopy` |
| `:82` (09-16) | `module(name)` is an ordinary node, members are `:` edges |
| `:81` (09-16) | `pub` is an annotation edge; export is a prelude rule |
| `:79` (09-16) | loaders never mint namespace owner nodes |

## 3. Before / after

Before, `fixtures/openapi/todo.dl7:12` and the flag:

```
dl8 compile fixtures/openapi/todo.dl7 --project fixtures/openapi --openapi fixtures/openapi/todo.json

(<- (route_returns_todo ?Path ?Method)
    (openapi.route ?Path ?Method ?OperationId ?RequestType ?ResponseType)
    (Conforms ?ResponseType TodoShape ?Proof))
```

After:

```
dl8 compile fixtures/openapi/todo.dl7 --project fixtures/openapi

(api: (import "@std/oai"))
(api.document "todo.json")

(<- (route_returns_todo ?Path ?Method)
    (api.route ?Path ?Method ?OperationId ?RequestType ?ResponseType)
    (Conforms ?ResponseType TodoShape ?Proof))
```

`std/oai.dl7` (new, shipped in the binary like the prelude):

```
(: document (* (: path text)))
(: route    (* (: path text) (: method any) (: operation_id text) (: request type) (: response type)))
(: param    (* (: path text) (: method any) (: name text) (: location text) (: type type)))
(pub route) (pub param) (pub document)
```

Rows the executor fills come from the existing `openapi_rows`
(`src/_4_comptime/_0_load/_9_openapi.rs:26`), keyed by every `document` seed.

## 4. Files owned

| file | change |
|---|---|
| `src/_0_read/` | only if `(x: y)` infix does not already read as `(: x y)`; verify with `dl8 read` first |
| `src/_2_lower/_12_units.rs:310-342` | `install_module_aliases`: stop aliasing program units into each other; the prelude (`:475-484`) stays the one implicit exporter |
| `src/_2_lower/_8_express.rs` | `import` in value position resolves as a comptime call whose `return` is a module node; the dot walk (`lower_path_walk:79`) needs nothing |
| `src/_4_comptime/_0_load/_9_openapi.rs` | `OPENAPI_RELATIONS` (`:13`) renamed to `oai.*`; rows land under the `std/oai.dl7` module node instead of a minted owner |
| `src/_4_comptime/_0_load/_5_graph.rs:349-353` | drop the loader owner mint |
| `src/_8_driver/_4_project.rs:82-121,152-190` | `load_openapi_documents` driven by `oai.document` seeds, not by the flag; alias loop at `:184` runs for the prelude only |
| `src/_8_driver/_0_read.rs:8-18` | add `STD: [&str; N]` with `include_str!("../../std/*.dl7")` |
| `src/bin/dl8.rs` | remove `--openapi`; `--project` stays |
| `src/_9_runtime/_3_executors/{soopy_refs,soopy_history,repo_at}.rs` | `RELATION` consts to `git.refs`, `git.history`, `fs.at`; `ERROR` consts follow (`git.refs_error`) |
| `src/_9_runtime/_3_executors/mod.rs`, `src/bin/dl8.rs:91` | roster names and the docstring |
| `std/oai.dl7`, `std/env.dl7`, `std/fs.dl7`, `std/git.dl7` | new; `*` product decls only, every hosted rel declared here |
| `prelude/` | `import_path` alias rule (`@std/` prefix via two-way `str.cons`), `export`, `imports`, `reaches`, `import_cycle` rules from `plans/v8/2026-09-16-modules-future-sight.md:66-74` |
| `fixtures/openapi/todo.dl7`, `tests/_21_openapi.rs` | after-form above; the test drops `--openapi` |
| `fixtures/hosts/*.dl7`, `tests/_20_hosts.rs` | executor renames |
| `book/src/` pages that name `--openapi`, `soopy_refs`, `openapi.route` | rename; `grep -rn "openapi\.\|soopy_refs\|soopy_history\|repo_at\|--openapi" book/src` must return zero |
| `.github/CI-KNOWN-RED.md` | remove the `_21_openapi` rows |

## 5. Files forbidden

`src/_3_check/**`, `src/_6_eval/**`, `src/_5_reify/**`, `macrotime/**`,
`src/_2_lower/_2_declare.rs`, `oracle/**`, `v5/ v6/ v7/`, anything under
`hafley-rs/`. A needed change in a forbidden file is a stop-and-report, spelled
as a cited fork, never an edit.

## 6. Steps

| # | step | receipt |
|---|---|---|
| 1 | `cargo test --test _21_openapi` on the base sha: 2 red with `unresolved_name(openapi)` | paste the two failure lines |
| 2 | executor renames + `std/{env,fs,git}.dl7` decls; `cargo test --test _20_hosts` | test names and PASS lines; `grep -rn soopy_ src/ --include=*.rs \| grep -v "soopy::" ` = 0 hits outside module paths |
| 3 | `STD` bundle in `_0_read.rs`; `import` resolves `@std/x` to the bundled text, any other path to `cwd.join(path)`; `dl8 compile` of a probe with `(x: (import "./sibling.dl7"))` and `x.member` in a goal, rc=0 | probe under `plans/v8/probes/2026-09-17-import-sibling.dl7`, paste `compiler_rows` count |
| 4 | retire the implicit alias for program units; a two-unit probe where unit B uses unit A's name bare must now fail `unresolved_name` | probe `2026-09-17-no-bare-splice.dl7`, paste the diagnostic |
| 5 | `std/oai.dl7`; `oai.document` seed drives `openapi_rows`; drop `--openapi` | `cargo test --test _21_openapi` green, paste the PASS lines |
| 6 | prelude rules `export`, `imports`, `reaches`, `import_cycle`; a two-file cycle probe compiles rc=0 with one `import_cycle` row | probe `2026-09-17-import-cycle.dl7`, paste the row |
| 7 | book renames, CI-KNOWN-RED rows removed | the grep in section 4 returns 0 |
| 8 | full gate (section 7) | paste the summary lines |

Commit after every step. Every commit message names the step number.

## 7. Gate

```bash
cargo build --release 2>&1 | tail -3
cargo test 2>&1 | grep -E "^test result|FAILED|panicked" 
cargo test --test _21_openapi --test _20_hosts --test _16_extract_tsi 2>&1 | grep -E "^test |test result"
grep -rn "openapi\.\|soopy_refs\|soopy_history\|repo_at\b|--openapi" src/ book/src fixtures/ tests/ | grep -v "_9_openapi.rs" | wc -l   # expect 0
git diff --stat origin/main...HEAD
```

Known red on the base that is not this lane's: `_16_extract_tsi`, `_20_hosts` (1),
`_22_book::probes_compile_as_their_page_says` red only inside a nested
worktree (`.github/CI-KNOWN-RED.md`, dl8 battery section). Measure each red
leg three times before reporting it.

## 8. Laws

- Tests are integration through the real binary (`CARGO_BIN_EXE_dl8`), like
  `tests/_21_openapi.rs:31`. No unit fakes.
- No `eprintln!`; `tracing` only.
- Comments state constraints the code cannot show; no change-log narrative.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal, "ground truth". Construct names use rxjs/prolog/SQL words.
- No number in a doc sentence; numbers live in tables or pasted output.
- dl variable names descriptive, never single letters.
- Language design is closed for this lane. A question the decisions do not
  answer is a stop-and-report with the throw site cited, never a guess.
- Every `.dl7` snippet in a book page carries its rx lowering.
- Every worktree branches from `origin/main`; `git diff --stat origin/main...HEAD`
  lists only files in section 4.

## 9. Report

```bash
boop beep --no-wait --as <lane> sprefa-coordinator "import arc: PR #<n>, _21_openapi <pass>/<total>, full battery <pass>/<total>, red legs: <list or none>"
```

The PR body carries the section 6 table with every receipt pasted.
