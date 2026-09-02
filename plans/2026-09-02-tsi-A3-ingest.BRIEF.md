# brief: TSI A3, the relation registry and the `--ingest` reverse door

Lane: `feature/tsi-a3-ingest`. Base: the `origin/main` sha AFTER the A1 PR merges (coordinator states it).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, section `## Decisions`: the 18 `tsi.*` relations and identity rule 5.
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` sections 4 (registry table), 6 (the step trace), 7 (signatures), 10.
- A1 landed: `src/tsi/types.rs` (`Arg`, `FactOut`, ...), `FlatFact: Deserialize`, `--witness`.

This arc delivers acceptance criteria 2 (decode + validate) and 3 (reverse door).

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/tsi/registry.rs` (new) | `ArgKind`, `Relation { name, args }`, `pub const REGISTRY: &[Relation]`, `pub fn relation(name) -> Option<&'static Relation>` |
| `v6/sprefa-extract/src/tsi/sink.rs` (new) | `TsiSink` per PLAN.md section 7; `fact()` checks arity and arg kinds against `REGISTRY` under `debug_assert!` |
| `v6/sprefa-extract/src/tsi/ingest.rs` (new) | `IngestError` (thiserror, one variant per step), `pub fn ingest(lines) -> Result<Vec<String>, IngestError>` |
| `v6/sprefa-extract/src/tsi/mod.rs` | add the three modules |
| `v6/sprefa-extract/src/bin/extract.rs` | `--ingest <PATH>...` (`conflicts_with_all` every other mode flag, `required_unless_present` joins `schema`); reads lines, prints `ingest()` output, exits 1 with the error's Display on `Err` |
| `v6/sprefa-extract/src/schema.rs` | `--schema` appends the registry: one line per relation `relation=<name> args=[<kind>...]`, printed from `REGISTRY`, never hand-typed |
| `v6/sprefa-extract/tests/97_ingest.rs` (new) | tests below |
| `v6/sprefa-extract/tests/fixtures/tsi/foreign_probe.jsonl` (new) | a hand-written foreign stream: protocol, run `mode=semantic tool=probe`, `tsi.type` x3, `tsi.product`, two `tsi.edge`, one `ts.readonly`, coverage complete for `tsi.edge`, witnesses |

Forbidden: `src/types.rs`, `src/wire.rs`, `src/tsi/types.rs` (A1's, merged; if a type is missing, STOP and report the gap), `src/project.rs`, `src/lang/**`, `tests/fixtures/resolve/**`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## Registry rows (verbatim; arity and kinds are the contract)

```text
tsi.type        [Id]
tsi.denotes     [Id, Id]                       symbol, type
tsi.has_type    [Span, Id]
tsi.origin      [Id, Atom, Span]               type, language, range
tsi.product     [Id]
tsi.sum         [Id]
tsi.callable    [Id]
tsi.primitive   [Id, Atom]
tsi.edge        [Id, Id, Text, Id, Int]        edge, owner, label, target, position
tsi.parameter   [Id, Id, Int, Atom]            param, callee, position, variance
tsi.called      [Id, Id, Id]                   result, callee, argument list
tsi.argument    [Id, Int, Id]                  list, position, type
tsi.input       [Id, Int, Id]
tsi.output      [Id, Int, Id]
tsi.subtype     [Id, Id, Atom]
tsi.assignable  [Id, Id, Atom]
tsi.conforms    [Id, Id, Atom]
tsi.equivalent  [Id, Id, Atom]
ts.interface    [Id]
ts.conditional  [Id, Id, Id, Id, Id]           result, check, extends, true, false
ts.mapped       [Id, Id, Id, Id]               result, key param, constraint, template
ts.readonly     [Id]                           edge
ts.optional     [Id]                           edge
rust.trait      [Id]
rust.impl       [Id, Id, Id]                   impl symbol, type, trait
rust.lifetime   [Id, Atom]                     param, name
rust.ownership  [Id, Atom]                     edge, shared|exclusive|owned
rust.assoc      [Id, Text, Id]                 owner, name, target
go.interface    [Id]
go.type_set     [Id, Id]
go.embedding    [Id, Id]
```

## `ingest` step trace (PLAN.md section 6; each step is one function, each error names its line)

```text
step 0  decode      serde FlatFact per line              -> IngestError::Decode { line, detail }
step 1  registry    Fact rows: name known, arity, kinds  -> IngestError::Relation { line, relation, detail }
step 2  id closure  every Arg::Id is declared by a tsi.type / tsi.edge (pos 0) / tsi.called (pos 2, the list) row
                    fixpoint over the row set, cycles are one pass   -> IngestError::Dangling { line, id }
step 3  coverage    Coverage complete with zero Fact rows for that relation -> IngestError::Coverage { run, relation }
step 4  renumber    ids in first-appearance order of the sorted fact rows; facts renumbered the same way
step 5  re-emit     Protocol first, then sorted_lines-equivalent ordering (key-sorted JSON, then line sort);
                    every ingested Fact gains one Witness { method: Foreign } for the ingest run
steady state: ingest(ingest(x)) == ingest(x)
```

`sorted_lines` lives in `src/project.rs` (forbidden file): call it through its public path if exported, else duplicate its two-line body in `ingest.rs` with a comment naming the origin.

## Tests, `tests/97_ingest.rs`

| case | input | expected |
|---|---|---|
| foreign accepted | `extract --ingest tests/fixtures/tsi/foreign_probe.jsonl` | rc=0; line 1 is protocol; every fact row has a `method=foreign` witness |
| bad arity | the fixture with one `tsi.edge` cut to 4 args | stderr contains `tsi.edge` and the line number; rc=1 |
| bad kind | `tsi.edge` position arg as `{"text":"0"}` | `IngestError::Relation`, detail names position 4 and `int` |
| unknown relation | `tsi.frobnicate` | `IngestError::Relation`, detail `not in registry` |
| dangling | `tsi.edge` naming id 9, never declared | `IngestError::Dangling { id: 9 }` |
| cycle ok | `tsi.edge` whose target is its owner | rc=0, one pass, no hang (10s cap on the test) |
| empty complete | `coverage complete tsi.sum`, zero `tsi.sum` rows | `IngestError::Coverage { relation: "tsi.sum" }` |
| idempotent | ingest output piped into ingest | byte-identical |
| renumber | fixture with ids 40, 7, 19 | output ids 0, 1, 2 in first-appearance order |
| schema | `extract --schema` | one line per REGISTRY row; count equals `REGISTRY.len()` |
| A1 stream ingests | `extract --witness --family type tests/fixtures/resolve/0_caller.ts | extract --ingest /dev/stdin` | rc=0 |

Test header carries a SABOTAGE RECEIPT: `--ingest` is an unknown flag on the base sha (clap rc=2).

## Build-vs-buy (decided in PLAN.md section 6)

serde derive (in tree); `thiserror` for `IngestError` if already a dep, else `std::fmt::Display` by hand. `schemars` and `jsonschema` are out of this arc.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test 97_ingest 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test golden_parity 2>&1 | tail -3
```

## Style laws

- No `eprintln!` in `src/**`; `tracing` only. The bin's error print goes through the existing `emit`/exit path in `extract.rs`.
- Comments: constraints only. No dates, no arc names.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- No per-row allocation of the registry: `REGISTRY` is a `const` slice, `relation()` is a linear scan over 31 rows or a `phf`-free match; no HashMap built per call.
- dl and doc variable names descriptive, never single letters.

## Done

PR titled `extract: TSI relation registry and --ingest reverse door (TSI A3)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A3 PR #<n>: 97_ingest N tests, registry 31 rows, idempotent"`.
