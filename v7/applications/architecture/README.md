# DL7 shared architecture graph

- [What this is](#what-this-is)
- [Reading order](#reading-order)
- [Run it](#run-it)
- [Source model](#source-model)
- [Generated files](#generated-files)
- [Invariants](#invariants)
- [Phase axis](#phase-axis)
- [D2 layers](#d2-layers)
- [Not here yet](#not-here-yet)

## What this is

One authored DL7 source, `0_system.dl7`, holds the shared architecture graph:
the components, their typed attachment edges, milestones, acceptance items,
evidence paths, and concrete runtime flows. A single DL7 emitter publishes those
relations as artifacts. `1_generate.pl` compiles that one source, sorts every
row by a stable id, validates the references, and writes two generated files:
`2_system.d2` (a validated D2 system map) and `2_progress.md` (counts and
per-component fractions).

Source: `v7/receipts/16_dl6_dl7_architecture_inventory.md`. One DL7 relational
kernel is applied at macrotime, comptime, and runtime. DL6 is a DL7 userland
application, not a second compiler core.

## Reading order

| file | what it holds |
| --- | --- |
| `0_system.dl7` | the whole authored graph and its emitter |
| `1_generate.pl` | compile, emit, validate, derive, write |
| `2_system.d2` | generated D2 system map, one shared model plus layers |
| `2_progress.md` | generated counts, per-component fractions, evidence paths |
| `../../test/21_architecture_generate.test.pl` | exact rows, rejection, arithmetic, determinism, drift |

## Run it

```bash
cd v7                   # from the repository root
just architecture-generate   # write 2_system.d2 and 2_progress.md
just architecture-check      # fail on generated drift, no rewrite
just architecture-render     # render the system map to SVG in a temp dir
just architecture-test       # focused PLUnit gate
```

## Source model

| relation | arity fields | authored from |
| --- | --- | --- |
| `component` | id, label, state, binding, plane | receipt 16 component rows |
| `attachment_edge` | id, src, relation, dst | receipt 16 `edge` rows |
| `milestone` | id, component, src | per-component milestone source |
| `acceptance_item` | id, milestone, status, count | per-component milestone fraction |
| `evidence` | id, component, path | receipt 16 evidence lists |
| `phase_axis` | binding, consumes, produces | the three binding times |
| `flow_step` | flow, step, src, relation, dst | concrete runtime/reference flows |

One `acceptance_item` row declares `count` items at one status. The relation
stays compact because the source states totals, not item identities:

```lisp
(milestone "m.dl7.kernel" "dl7.kernel" "v7/tasks/00_PROGRESS.md:24-34")
(acceptance_item "ai.dl7.kernel.done" "m.dl7.kernel" "completed" 10)
```

The emitter publishes all seven relations through the existing `emits/3`
protocol:

```lisp
(emits ArchEmitter "components" component)
(emits ArchEmitter "edges" attachment_edge)
```

## Generated files

`2_system.d2` quotes every generated id and label, groups components into the
`compile`, `algebra`, `runtime`, `hosts`, `effects`, and `generated` containers,
and styles each relation class separately. Every node id and edge label is
derived from a stable source id, so regeneration is byte-stable.

`2_progress.md` reports exact counts, a per-component `completed/total` and
percent, and every evidence path.

## Invariants

| check | rule |
| --- | --- |
| deterministic output | two derivations are byte-identical |
| drift gate | `architecture-check` fails when a tracked file differs from a fresh derivation, without rewriting it |
| dangling edge | every edge `src` and every non-`none` `dst` is a declared component |
| acceptance status | an item status is one of `completed`, `pending`, `failed`, `blocked` |
| component vocabulary | state, binding, and plane are closed sets |
| percentage | completed item counts over declared item counts; `unknown` when no acceptance list is declared |

Completion is never inferred from a file existing.

## Phase axis

| binding | consumes | produces |
| --- | --- | --- |
| macrotime | syntax graph | expanded syntax |
| comptime | TSI/type/module graph | checked runtime Datalog/IVM program |
| runtime | whole-language fact deltas | maintained query/effect relations |

## D2 layers

| layer | shows |
| --- | --- |
| `compiler` | the compile, algebra, and generated clusters with their edges |
| `runtime_tick` | event to effect: filesystem/Git/editor watch, hosted `sprefa-extract`, signed TSI facts, `sqlite_ivm` maintenance, DL7 query view, LSP diagnostic or shell/HTTP effect |
| `reload` | reload catalog, retention, frontier, and the HMR generation-boundary swap |
| `reference` | DL6 Prolog to `ProgramJson` to `sprefa-engine-rs`/SQLite, and the retained V7 successor mapping through `rt.executor` to `rt.v7` |

## Not here yet

| gap | state |
| --- | --- |
| physical acceptance item identities | source states totals, not item text |
| renderer pixel assertions | `architecture-render` proves the D2 compiles, not the layout |
| `v7/rust` tree | recorded `planned`; no tree exists at this base |
