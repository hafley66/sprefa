# Lane brief: rust typegraph as a compiling d2 board (issue rust-typegraph-d2)

First action: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

## What you are building

One `cargo` example that reads the rust type graph the extractor already
produces, cuts it to what is reachable from a named entrypoint, and writes one
or more `.d2` files that compile.

You are NOT designing a graph. Every node kind and edge kind already exists and
is enumerated in the code. Your job is the reachability cut, the board split and
the d2 text.

## The data, all of it already there

| piece | file:line |
|---|---|
| node kinds (9) | `v6/sprefa-extract/src/types.rs:196-224` `TypeEntityKind`: struct, enum, trait, class, interface, alias, function, method, const |
| edge kinds (7) | `v6/sprefa-extract/src/types.rs:226-253` `TypeEdgeKind`: field, variant, impl, generic, param, returns, uses |
| the API you call | `sprefa_extract::resolve_project_jsonl(&ResolveRequest { .. })`, `v6/sprefa-extract/src/project.rs:233` |
| the request struct | `v6/sprefa-extract/src/project.rs:68-81` |
| the arms flag | `v6/sprefa-extract/src/project.rs:42-50` — set `call: false, types: true` |
| the phase-1 node row | `record=node family=type span kind name`, `v6/sprefa-extract/src/schema.rs:20` |
| the resolved edge row | `record=resolved_type_edge owner_path owner_name owner_start owner_end target_path target_name kind`, `v6/sprefa-extract/src/schema.rs:35` |

Set `scip: ScipMode::Off` and `project_root: None`. No indexer runs.

**Node identity is `(path, name)`, not `name`.** The resolved edge row carries
`owner_path` and `target_path` separately for exactly that reason. Two `Config`
structs in two files are two nodes. Getting this wrong collapses the graph and
is the single most likely way this deliverable comes back.

## The example, exactly

New file `v6/sprefa-extract/examples/typegraph_d2.rs`. Cargo auto-discovers
`examples/`; do NOT edit `Cargo.toml`.

Invocation:

```
cargo run --example typegraph_d2 -- --root <DIR> --entry <PATH::NAME> --out <DIR>
```

Steps:

1. Walk `<DIR>` for every `*.rs` file. Skip anything under `target/`.
2. Call `resolve_project_jsonl` with those paths, `arms = { call: false, types: true }`.
3. Parse the JSONL with `serde_json` (already a dependency). Keep two maps:
   - nodes: `(path, name) -> kind`, from `record=node` rows with `family=type`
     and a non-null `name`. The path for a phase-1 node row is the input file it
     came from; if the flat row does not carry it, key nodes off the
     `resolved_type_edge` endpoints alone and say so in the header comment.
   - edges: `((owner_path, owner_name), (target_path, target_name), kind)` from
     `record=resolved_type_edge` rows.
4. BFS out from `--entry`, following edges in the owner-to-target direction.
   Record each reached node's hop distance. An entry that matches no node is an
   error with a nonzero exit and a message listing the five nearest names.
5. Split into boards by hop distance: board 0 is hops 0 through 1, board 1 is
   hop 2, and so on, splitting further whenever a board would exceed the budget
   below. Write `<out>/typegraph.0.d2`, `<out>/typegraph.1.d2`, ...
6. Print one line per board: file path, shape count, edge count.

## The d2 text, exactly

Read the `i:d2-authoring` skill before writing the first line of d2.

Per board:

```
direction: right

<id>: <name> {
  shape: rectangle
  class: <kind>
}
...
<src_id> -> <dst_id>: <edge_kind>
```

- `<id>` is a sanitized `(path, name)` key: replace every character outside
  `[A-Za-z0-9_]` with `_`. Ids must be unique across the board.
- `direction: right` is not optional. It is what makes the rendered board wider
  than it is tall, which is a hard requirement below.
- Declare one `classes:` block per board mapping the nine `TypeEntityKind` slugs
  to a fill, so the kinds read apart. Keep it to fill and stroke; no icons, no
  3d, no shadows.
- Do not emit a `label` longer than 40 characters. Truncate with an ellipsis.

## Board budget, hard gate, not advice

- **Compiles.** `d2 <file> <scratch>/out.svg` exits 0, for every emitted board.
- **Aspect.** The rendered SVG's `viewBox` width must exceed its height. Read it
  with `grep -o 'viewBox="[^"]*"' out.svg`.
- **Shape count.** At most 24 per board, counted by
  `grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:' <file>`.
- **Over budget means SPLIT.** Never shrink a font, never drop `direction`,
  never cram. A board over 24 shapes gets divided and both halves get gated.

## The test

New file `v6/sprefa-extract/tests/15_typegraph_d2.rs`. It must:

1. Run the example over a small committed rust fixture tree. Reuse the rust
   fixtures already under `v6/sprefa-extract/tests/fixtures/`; read what is
   there before adding anything. If you add fixtures, put them under
   `tests/fixtures/typegraph_d2/`.
2. Assert at least one board was written and that the entry node appears in it.
3. Assert every emitted board passes all four budget checks above, by SHELLING
   OUT to `d2` and reading rc and the viewBox. If `d2` is not on PATH the test
   FAILS with a message naming the missing binary. Do not skip to green; the
   ratchet never fakes green.

Write the test and see it red before the example exists. Paste that red output
in the commit body, then the green.

## Gate, run each twice, read rc explicitly, never pipe through tail

```bash
cd /path/to/your/worktree/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo run --example typegraph_d2 -- --root src --entry 'src/types.rs::TypeF' --out /tmp/tg; echo "RUN rc=$?"
for f in /tmp/tg/*.d2; do d2 "$f" /tmp/tg/out.svg; echo "$f d2 rc=$?"; grep -o 'viewBox="[^"]*"' /tmp/tg/out.svg; grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:' "$f"; done
```

Paste that whole for-loop's output into the commit body. It is the receipt.

`cargo build` ALWAYS runs before any binary gate. Baseline at the base sha is
rc=0 with every leg green, so any red is yours.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/examples/typegraph_d2.rs` (new)
- `v6/sprefa-extract/tests/15_typegraph_d2.rs` (new)
- `v6/sprefa-extract/tests/fixtures/typegraph_d2/**` (new, only if needed)

FORBIDDEN, do not open to edit:
- every file under `v6/sprefa-extract/src/` without exception
- `v6/sprefa-extract/Cargo.toml`
- every existing file under `v6/sprefa-extract/tests/`
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

If the example cannot be written without a `src/` change, STOP AND REPORT what
the change would be. Do not make it. Three concurrent lanes own those files.

## Laws that bind you

- Never spawn a subagent. Fan-out is the coordinator's call.
- Mermaid over ascii art; d2 where the board carries structure. No ascii boxes
  anywhere in this deliverable.
- Comment budget: comments state constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- No em dashes. Banned words in prose and identifiers: provenance, substrate,
  load-bearing, regime.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before you report done. An uncommitted
  deliverable is an undelivered one.
- Do not push. Do not open a PR. Do not merge. The coordinator lands it.
