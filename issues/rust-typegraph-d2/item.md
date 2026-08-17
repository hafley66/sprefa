---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
closed: 2026-08-16
---

## Description

Draw the rust type graph the extractor already produces: `TypeF` nodes and
`TypeEdgeKind` edges over a rust tree, cut to what is reachable from a named
entrypoint, rendered as a compiling `.d2` board.

## What exists to draw

| piece | receipt |
|---|---|
| node kinds | `v6/sprefa-extract/src/types.rs:196-224` `TypeEntityKind` (struct, enum, trait, class, interface, alias, function, method, const) |
| edge kinds | `v6/sprefa-extract/src/types.rs:226-253` `TypeEdgeKind` (field, variant, impl, generic, param, returns, uses) |
| the rust arm that emits candidates | `v6/sprefa-extract/src/lang/rust.rs:670` `impl Resolve<TypeF> for RustSource` |
| the resolved wire row | `v6/sprefa-extract/src/schema.rs:35` `record=resolved_type_edge  owner_path  owner_name  owner_start  owner_end  target_path  target_name  kind` |
| the phase-1 node row | `src/schema.rs:20` `record=node  family=type  span  kind  name` |
| how to produce both | `extract --resolve <paths> --family type` (`src/schema.rs:181-187`) |

Note the shape constraint that falls out of the wire: `resolved_type_edge`
carries `owner_path` + `owner_name` and `target_path` + `target_name`, so a node
id is `(path, name)`, not a bare name. Two `Config` structs in two files are two
nodes.

## Deliverable

A tool that takes a rust tree plus an entrypoint and writes one or more `.d2`
files:

1. Run `extract --resolve <every .rs under the tree> --family type` and read the
   JSONL.
2. Build the graph: nodes from `record=node family=type`, edges from
   `record=resolved_type_edge`.
3. Reachability from the entrypoint, entrypoint given as `path::name`. Directed,
   following edges out. Emit the reachable set with hop distance.
4. Render `.d2`: one shape per node labelled `name` with the kind as the shape
   class, one connection per edge labelled with `kind`.

## Board budget, non-negotiable

- The file must compile: `d2 <file> <scratch>/out.svg` exits 0.
- Aspect: the rendered `viewBox` width must exceed its height.
- Shape count per board: at most 24, counted by
  `grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:' <file>`.
- Over budget means SPLIT into multiple boards (by hop distance, or by owner
  path), never shrink labels and cram.

Read `.claude/skills/i-d2-authoring` (the `i:d2-authoring` skill) before writing
the first line of d2.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
d2 <each emitted board> /private/tmp/.../out.svg    # rc=0 each
```

## Comments

### 2026-08-16T17:29:31Z · @extract-closeout-driver

PR #303, gate green twice, taken by the coordinator after the lane produced nothing. direction: down is what makes the board wide: the same seven-node star renders 619x898 with direction: right and 1052x455 with down. Not covered: the over-budget chunk split, since no hop band in src reaches 24 nodes.
