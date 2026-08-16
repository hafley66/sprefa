# flow-parity-rust-targets: the call-target blocker

ARCH task `flow_parity_residue` (`v6/prolog/ARCH.pl:779`).

## Contents

1. [Verdict](#verdict)
2. [The fork, priced](#the-fork-priced)
3. [Baseline (rig on base, twice)](#baseline-rig-on-base-twice)
4. [Post (rig after the change, twice)](#post-rig-after-the-change-twice)
5. [The change](#the-change)
6. [Residue: one class, 69 rows](#residue-one-class-69-rows)
7. [Validation](#validation)

## Verdict

Fork (a), the extractor's own resolution facts. Neither branch of the brief's
fork was the actual blocker: the resolver already resolved these targets and the
rows died at the typed host boundary.

| measured | value |
|---|---|
| resolved edges the extractor emits over the pinned corpus | 215 |
| of those, both caller and callee named | 166 |
| graded `call_target` rows before the change | 166 |

The equality is the finding. `sh resolve_at(...) -> (caller_name: text, ...)`
(`v6/dl/fixtures/flagship-flow.dl6:35`) types every response column `text`, and
a JSON null drops the row. A closure def carries no name
(`Node::name` is `None` for `CallKind::Lambda`), so 27 resolved edges whose call
site sits inside a closure body never reached `call_target`, and with them every
`flow_arg_edge` / `flow_ret_edge` / `flow_param_type` / `flow_node_type` row
those targets feed.

## The fork, priced

| fork | receipt | call |
|---|---|---|
| (a) typed Rust resolution facts | 27 resolved edges dropped for a null caller name; 21 of them are keys V5 also has | TAKEN |
| (b) pinned SCIP index | `rust-analyzer scip .` on the pinned corpus exits in 0.142s: `Error: no projects`. The rig copies 13 files with no `Cargo.toml`, and `ScipRust` (`src/scip.rs:142-161`) shells the same argv | REJECTED |

(b) is not a pinning job. It needs a synthesized crate manifest inside the rig,
a rust-analyzer version pin for determinism, and it buys nothing for the class
that remains (below), which is a V5 duplication habit no index can add.

## Baseline (rig on base, twice)

`DL_V5_BIN=<repo>/target/release/dl bash v6/tsv2/scripts/flagship-flow.sh`,
ports 17591 and 17592, byte-identical tables.

| rel | v5 | v6 | match | v5only | v6only |
|---|---|---|---|---|---|
| flow_edge | 2726 | 3062 | 2601 | 125 | 461 |
| flow_reach | 39246 | 27644 | 17774 | 21472 | 9870 |
| flow_param_type | 39 | 43 | 35 | 4 | 8 |
| flow_node_type | 39 | 60 | 35 | 4 | 25 |
| direct | 2387 | 2387 | 2387 | 0 | 0 |
| arg | 122 | 255 | 90 | 32 | 165 |
| ret | 219 | 423 | 126 | 93 | 297 |
| call_target | 203 | 166 | 113 | 90 | 53 |

The ARCH row's `2457/2457` direct figure was measured on an older corpus; this
tree's pinned 13 files give 2387/2387, exact on both sides.

## Post (rig after the change, twice)

Ports 17593 and 17594, byte-identical tables.

| rel | v5 | v6 | match | v5only | v6only | match delta |
|---|---|---|---|---|---|---|
| flow_edge | 2726 | 3135 | 2634 | 92 | 501 | +33 |
| flow_reach | 39246 | 29034 | 17949 | 21297 | 11085 | +175 |
| flow_param_type | 39 | 47 | 39 | 0 | 8 | +4 |
| flow_node_type | 39 | 64 | 39 | 0 | 25 | +4 |
| direct | 2387 | 2387 | 2387 | 0 | 0 | 0 |
| arg | 122 | 284 | 102 | 20 | 182 | +12 |
| ret | 219 | 467 | 147 | 72 | 320 | +21 |
| call_target | 203 | 193 | 134 | 69 | 59 | +21 |

`flow_param_type` and `flow_node_type` now have a V5-only column of zero: every
typed row V5 produces, V6 produces.

## The change

`v6/sprefa-extract/src/project.rs`, `call_facts`: the caller of a resolved call
edge is the edge's own `src` def, so its name is never absent, only unspelled.
A nameless def is named `closure@<byte_start>` in V6's own byte coordinates.
V5 spelled the same thing `root::<path>::function::<fn>::closure::<line>_<col>`
(measured on `src/wire.rs:71`).

The callee side keeps its `Option`: a resolved edge whose TARGET is a closure
stays null and stays dropped. Naming it would add 22 rows that V5 keys as
`<line>_<col>` and V6 as `closure@<byte>`, which the referee cannot join, so it
is pure V6-only noise.

## Residue: one class, 69 rows

All 69 remaining V5-only call-target rows are V5 scoring one callee at every
`call_res` node of a call chain. V6 scores it once, at the call's own position,
and already carries the same `(callee_path, callee_name)` on that line.

```
src/types.rs:926   .entry(output.strings.lookup(name).to_string())
  v5  col 25 -> types.rs::lookup      v6  col 25 -> types.rs::lookup
  v5  col 59 -> types.rs::lookup      v6  (nothing at col 59)

src/wire.rs:69     span: SpanOut::new(node.span.start, node.span.end()),
  v5  col 18 -> types.rs::end         v6  col 18 -> types.rs::new
  v5  col 58 -> types.rs::end         v6  col 58 -> types.rs::end
```

The second block is the same habit costing V5 a real target: its cross product
puts `end` on the outer `SpanOut::new` node and never emits `new` at all, which
is 17 of V6's 59 V6-only rows. Closing the 69 means emitting V5's duplicates,
which is not a resolution fact. Recommend the ARCH row treat the call-target
column as closed at 134/203 and grade the chain-duplicate class as a referee
question, not an extractor one.

## Validation

| leg | runs | result |
|---|---|---|
| `flagship-flow.sh` baseline | 2 | identical tables, rc=0 |
| `flagship-flow.sh` post | 2 | identical tables, rc=0, classifier `0 unclassified` |
| `cargo test` (sprefa-extract) | 2 | rc=0, 20 suites ok, 0 failed |
| `swipl -g go -t halt ARCH.pl` | 2 | 7 PASS, 0 FAIL |

New pin: `resolve_names_a_closure_caller` in `tests/1_resolve_cli.rs`, golden
`tests/fixtures/resolve/9_closure_resolved_edges.jsonl`. Fail-first receipt: the
pre-change release binary emits the same row with `"caller_name":null`.
