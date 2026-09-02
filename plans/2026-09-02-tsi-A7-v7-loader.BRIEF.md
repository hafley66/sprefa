# brief: TSI A7, the v7 loader for accepted TSI rows

DISPATCH GATE: Chris's word. This arc decides how foreign type facts become v7 nodes; that is language design (CLAUDE.md "Lang design happens with Chris in the room"). The mapping below is the merged plan's section 7; the lane implements it verbatim and files every fork it hits as a `diagnostic`, never a design choice of its own.

Lane: `feature/tsi-a7-v7-loader`. Base: the `origin/main` sha AFTER the A3 PR merges (coordinator states it; only the wire spec is needed, fixture streams are hand-written).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, `## Decisions`: the relation list; rule 5; "Recursive graphs close through IDs rather than bounded expansion".
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` section 4 (wire), section 7 (Prolog signatures), section 8 (uniqueness: newest complete semantic run per `(scope, relation)`), section 10 (cases "accepted view", "newest run wins").
- The `accepted/1` rule, verbatim from the reference (`/private/tmp/sprefa-v7-value-nodes/.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md:183-195`; the same text is reproduced below so the lane needs no lab worktree):

```text
accepted(?Fact) <-
    extract.witness(?Fact, ?Run, ?Method),
    extract.run(?Run, semantic, ?Tool, ?Version, ?Scope).

accepted(?Fact) <-
    extract.fact(?Fact, ?Relation, ?Arguments),
    extract.witness(?Fact, ?SyntaxRun, ?Method),
    extract.run(?SyntaxRun, syntax, ?Tool, ?Version, ?Scope),
    not semantic_complete(?Scope, ?Relation).
```

`semantic_complete(Scope, Relation)` holds when some `extract.run(Run, semantic, _, _, Scope)` has `extract.coverage(Run, Relation, complete)`.

Delivers criterion 9.

## Where v7 is today

- `v7/src/0_reader/` is the text boundary and performs no binding (`0_reader/README.md:1-8`); the loader does NOT live there.
- `v7/src/2_comptime/0b_filesystem_grapher.pl` (`install_project_graph/6`) is the shape to mirror: it hands the compiler a `module_basement(Owner, basement_program(root_graph(Nodes, Edges), datalog_program(Relations, Seeds, Rules)))` where `Nodes` are `node(Id)`, `module(Id)`, `product(Id)`, `sum(Id)` and `Edges` are `pending_edge(Owner, Label, target(Target), Index)` (`0b_filesystem_grapher.pl:196`), plus `module_origins(Owner, [origin(edge(Owner, Label, Index), <source>) ...])`.
- `2_compiler.pl:999-1011` turns those into kernel seeds: `node_seed/2`, `edge_seed/2` (`:/4`).
- Structural conformance already exists in the prelude: `v7/prelude/4_type_algebra.dl7` (`Conforms`, `conformance_candidate`, `matching_contract_edge`).
- The test runner: `swipl -q -g "load_files([...test.pl],[silent(true)]),run_tests,halt"` (`0_reader/README.md:197`).

## Files you own

| file | change |
|---|---|
| `v7/src/2_comptime/0c_extract_loader.pl` (new) | `load_tsi_stream/3`, `accepted_rows/2`, `install_tsi_graph/6` |
| `v7/prelude/5_tsi_primitives.dl7` (new) | one product node per primitive class the loader can target: `string`, `number`, `boolean`, `bigint`, `symbol`, `void`, `undefined`, `null`, `never`, `unknown`, `any`, and the rust builtins `i8..i128, u8..u128, f32, f64, bool, char, str, usize, isize`. Declared as `(: <name> (* ))`-shaped empty products the way `0_constructors.dl7` declares its kernel names; read that file first and copy its form exactly |
| `v7/test/4_extract_loader.test.pl` (new) | tests below |
| `v7/test/fixtures/tsi/` (new) | hand-written `.jsonl` streams: `0_syntax_user.jsonl`, `1_semantic_user.jsonl`, `2_semantic_user_v2.jsonl`, `3_recursive.jsonl`, each valid under `extract --ingest` (run it; the binary on `origin/main` has the flag) |
| `v7/src/2_comptime/2_compiler.pl` | ONE hunk: `compile_dl7_project/5` accepts an optional `tsi_streams([Path...])` in `ProjectContext` and calls `install_tsi_graph/6` after `install_project_graph/6`. If `ProjectContext` has no such slot, add the term and say so in the PR |

Forbidden: everything under `v6/`, `v7/src/0_reader/**`, `v7/prelude/{0,1,2,3,4}_*.dl7`, `v7/src/2_comptime/{0_lowerer,0a_module_lowerer,0b_filesystem_grapher,1_checker,1a_generated_program_assembler,1b_compiler_tracer,1c_compiler_cacher}.pl`, the issue file.

## Signatures (PLAN.md section 7, restated)

```prolog
%% load_tsi_stream(+JsonlPath, -Rows, -Diagnostics) is det.
%  json_read_dict/3 per line (library(http/json), already used by
%  test/3_compiler_trace.test.pl). Rows are the wire records as terms:
%    extract_protocol(Version)
%    extract_run(Run, Mode, Tool, Version, Scope)
%    extract_fact(Fact, Relation, Args)          Args: id(N) | span(Digest,S,E) | text(T) | int(N) | atom(A)
%    extract_witness(Fact, Run, Method)
%    extract_coverage(Run, Relation, Coverage)
%    extract_diagnostic(Run, Relation, Detail)
%  A protocol other than 1 is diagnostic tsi_protocol(Version) and zero rows.
%  A malformed line is diagnostic tsi_line(Path, LineNo, Why); loading continues.

%% accepted_rows(+Rows, -Accepted) is det.
%  The two accepted/1 clauses above over Rows, as plain Prolog. Accepted is a
%  list of extract_fact/3 terms. Newest run wins: when two semantic runs
%  share Scope and both are complete for Relation, the higher Run number's
%  facts are accepted and the lower's are not (PLAN.md section 8).

%% install_tsi_graph(+Rows, +Basements0, +Origins0, -Basements, -Origins, -Diagnostics) is det.
%  accepted rows only. Owner = module(tsi(Tool, Scope)).
%    tsi.type(Id)                  -> node(tsi_node(Owner, Id))
%    tsi.product(Id)               -> product(tsi_node(Owner, Id))
%    tsi.sum(Id)                   -> sum(tsi_node(Owner, Id))
%    tsi.edge(E, Own, Label, Tgt, Pos) -> pending_edge(tsi_node(Owner, Own), Label, target(tsi_node(Owner, Tgt)), Pos)
%    tsi.called(R, Callee, L) + tsi.argument(L, Pos, A) ... -> the existing application identity:
%                                     the same term 2_compiler.pl's source_application_edges/2 mints for a written call
%                                     (read it; do not invent a second application shape)
%    tsi.primitive(Id, Class)      -> node identity is the prelude product named Class (5_tsi_primitives.dl7), no new node
%    tsi.origin(Id, Lang, span(D,S,E)) -> origin(node(tsi_node(Owner, Id)), extract(Lang, D, S, E)) in Origins
%    tsi.callable/input/output, tsi.parameter, tsi.denotes, tsi.has_type, tsi.conforms,
%    tsi.subtype/assignable/equivalent, every ts.*, rust.*, go.* row
%                                  -> a comptime relation of the same name in the basement's datalog_program
%                                     Relations/Seeds: relation name = the wire name with '.' kept, arity = args length,
%                                     seed args = id(N) as ref(tsi_node(Owner, N)), everything else as const(_).
%  Recursion: a tsi.edge whose target is its owner is one pending_edge; nothing expands.
```

The loader never decides a language question. Any row it cannot place (an unknown relation, an id with no `tsi.type`, a `tsi.primitive` class absent from the prelude) is a `diagnostic` term and the row is skipped; the test asserts the diagnostic.

## Tests, `v7/test/4_extract_loader.test.pl`

Run: `swipl -q -g "load_files(['v7/test/4_extract_loader.test.pl'],[silent(true)]),run_tests,halt"` from the repo root.

| case | input | expected |
|---|---|---|
| syntax stream alone | `0_syntax_user.jsonl` (`User` product, two edges, `ts.readonly`, `coverage partial`) | `product(tsi_node(_, User))`, two `pending_edge` rows with positions 0 and 1; `ts.readonly` is a comptime relation with one seed |
| semantic replaces syntax | `0_syntax` then `1_semantic_user.jsonl` (`complete` for `tsi.edge`, edges with resolved targets) | the syntax run's `tsi.edge` facts are not in `accepted_rows`; the semantic ones are; other syntax relations without a complete claim stay |
| newest run wins | `1_semantic` then `2_semantic_user_v2.jsonl` (run 2, one field renamed) | only run 2's edges are accepted |
| recursion | `3_recursive.jsonl` (`Node.next -> Node`) | one `pending_edge` whose target is its owner; `install_tsi_graph/6` terminates (plunit `timeout(10)`) |
| primitive | an edge targeting `tsi.primitive(_, string)` | the edge's target is the prelude `string` product; no `tsi_node` for it |
| conforms proves | the semantic `User` stream plus a `Mapper` product with one edge | compiling a `.dl7` fixture that asks `Conforms(User, Mapper, _)` over the loaded graph yields one proof (mirror `test/2_module_system.test.pl`'s use of `compile_dl7_project/5`) |
| bad protocol | a stream whose first line is `protocol version=2` | zero rows, one `tsi_protocol(2)` diagnostic |
| unknown relation | a `tsi.frobnicate` row | skipped, one diagnostic naming it |

Header comment carries a SABOTAGE RECEIPT: on the base sha `0c_extract_loader.pl` does not exist, `load_files` fails.

## Gate

```bash
swipl -q -g "load_files(['v7/test/0_reader.test.pl','v7/test/1_entrypoints.test.pl','v7/test/2_module_system.test.pl','v7/test/3_compiler_trace.test.pl','v7/test/4_extract_loader.test.pl'],[silent(true)]),run_tests,halt"
cd v6/sprefa-extract && for f in v7/test/fixtures/tsi/*.jsonl; do cargo run -q --features cli --bin extract -- --ingest ../../$f > /dev/null || echo "BAD $f"; done
```

Second command prints nothing.

## Style laws

- Language vocabulary: rxjs, prolog, SQL words only; "support" is banned (say refCount or "handles").
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, refusal, ground truth.
- Comments: constraints only. No dates, no arc names.
- No em dashes.
- Descriptive Prolog variable names (`OwnerTypeId`, never `O`).
- Follow `0b_filesystem_grapher.pl`'s module header, `must_be/2` guards and diagnostics-list style exactly (colocated consistency).

## Done

PR titled `v7: load accepted TSI rows as product nodes, colon edges and comptime relations (TSI A7)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A7 PR #<n>: 4_extract_loader N tests, whole v7 battery green"`.
