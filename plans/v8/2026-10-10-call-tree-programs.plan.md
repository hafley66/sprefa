# Call-tree programs in sprefa over ryi rows: scope

Status: scoping with Chris, nothing built.

Goal (Chris, 2026-10-10): write sprefa programs that join ryi's rows, start from a root or listed
entrypoints, and aim at one part of the static call tree: what the main call path reaches, and
what lies between an entrypoint and a target.

## Layers

| layer | what | state |
|---|---|---|
| 1 ryi rows in | `(extract Root Family Kind Payload)` | built: `src/_9_runtime/_3_executors/extract.rs`, `fixtures/hosts/3_extract.dl7` |
| 2 payload fields | `caller_path`, `callee_start`, ... as columns | gap; payload is one JSON string |
| 3 call graph rules | function def, enclosing function of a call site, call edge | to write; SQL prior art `tests/it/_9j_no_recursion.rs:15-29` |
| 4 aim | reach from entrypoints, on-path to a target | to write; recursion is sprefa's (boundary skill, ownership table) |
| 5 output | tree, file list, rename tsv | choose |

## Layer 2 options

- (a) a string JSON reader, the `fs.json` shape (`std/fs.dl7:17`) over a payload string.
- (b) typed products generated from `hafley-rs` `crates/sprefa-extract/schema/generated/5_facts.json`
  (every record with its columns). The boundary skill plans the same path for the `.tsp`.

## Layer 4 sketch (unbuilt, not compiled)

```lisp
(<- (Reaches ?Entry ?Function)
    (Entry ?Entry)
    (Calls ?Entry ?Function))

(<- (Reaches ?Entry ?Callee)
    (Reaches ?Entry ?Caller)
    (Calls ?Caller ?Callee))

(<- (OnPath ?Entry ?Target ?Function)
    (Reaches ?Entry ?Function)
    (Reaches ?Function ?Target))
```

TS twin:

```ts
class Reaches { entry: Fn; fn: Fn }
// reaches = closure of calls from entries
// onPath  = reaches(entry, fn) && reaches(fn, target)
```

## Decisions (Chris, 2026-10-10)

| question | answer |
|---|---|
| first query | reach set from listed entrypoints |
| node | function (definition span plus name) |
| payload to rules | typed products generated from ryi's `5_facts.json` |
| output | diagnostics |

## Precedent in ryi

| ryi has | where (`hafley-rs` `crates/sprefa-extract/`) |
|---|---|
| `ryi graph --from <PATH#NAME>`: reach set, first discovery, depth, grade, line | `src/0_graph.rs:252,332` |
| reach as a recursive CTE capped at 32 hops | `src/bin/ryi/0_sqlite.rs:113` `reach_walk_sql`, `REACH_DEPTH_CAP` |
| `caller_name` / `callee_name` on `resolved_edge` | `--resolve` output |
| byte offset to line via `line_start` | `src/bin/ryi/0_sqlite.rs:95` `span_lines` |
| grade `+` oracle or module plane, `~` syntax guess, `-` unresolved | `src/bin/ryi/0_sqlite.rs:105` `grade_sql` |

The boundary skill names ryi's `graph` as a thin loop to be replaced by "a canned sprefa program
over the providers". This program is that replacement.

## Steps

1. Built on branch `feat/std-ryi`. `@std/ryi` (`std/ryi.dl7`) holds one product per ryi record,
   `root` first; `src/_9_runtime/_3_executors/ryi_schema.rs` holds their columns. Both are rendered
   from `5_facts.json` by `_8_contracts::std_ryi_matches_the_schema`. The served `ryi.resolve`
   (`ryi_resolve.rs`) answers typed rows; a null column is `none`. `extract` keeps its
   payload-string rows. Receipt: `_5_store_host::ryi_resolve_answers_typed_rows_over_the_corpus`
   over `fixtures/hosts/3b_ryi_resolve.dl7`.
2. Built: `fixtures/hosts/3c_ryi_reach.dl7` (`e6ba9454d`). `Calls` over `ryi.resolved_edge`,
   `ReachedAt` by `int.add` under `(int.lt ?Depth 32)`, `Reaches` at `(min ?Depth)`.
3. Oracle held: `_5_store_host::ryi_reach_equals_ryi_graph_over_the_corpus` and `_over_src` run
   `ryi graph --from` live. From `bin/dl8.rs#main` over `src/`: 999 of 999 rows identical, 2.0 s.
4. Diagnostics: one `lsp.diagnostic` row per reached function (`std/lsp.dl7:29`), severity
   information (3), message `reached from <entry> at depth <N>`. Blocked: `lsp.diagnostic` wants
   line and column, and ryi's `--lines` columns (`callee_line`, `callee_col`) are not in
   `5_facts.json`, so `@std/ryi` cannot type them. Filed as hafley-rs issue
   `lines-decorations-schema`.

Oracle: `ryi graph --from <PATH#NAME>` on `fixtures/extract/corpus`. Example, `ts/report.ts#reportTeam`:
`formatTeam` depth 1 grade `+` line 7; `formatUser` depth 2 grade `~` line 3. The dl7 rows must equal
ryi's `graph_node` rows.

## Open

- Diagnostic message text.
