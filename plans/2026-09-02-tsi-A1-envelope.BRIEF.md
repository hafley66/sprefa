# brief: TSI A1, the witness envelope on the extract wire

Lane: `feature/tsi-a1-envelope`. Base: `origin/main` `15e95de833c1d5aef122d507a001968e1ff469f5`.
FIRST ACTION: `git merge --ff-only 15e95de833c1d5aef122d507a001968e1ff469f5`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, section `## Decisions` (the contract).
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` sections 4, 7, 10.

This arc delivers acceptance criteria 1, 2 (serialize half), and the `run mode=syntax` half of 4.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/tsi/mod.rs` (new) | `pub mod types;` and re-exports |
| `v6/sprefa-extract/src/tsi/types.rs` (new) | `Mode`, `Arg`, `RunOut`, `FactOut`, `WitnessOut`, `CoverageOut`, `DiagnosticOut`, `Method`; all `Serialize + Deserialize + Debug + PartialEq` |
| `v6/sprefa-extract/src/types.rs` | `FlatFact` derives `Deserialize`; six new arms `Protocol { version: u32 }`, `Run(RunOut)`, `Fact(FactOut)`, `Witness(WitnessOut)`, `Coverage(CoverageOut)`, `Diagnostic(DiagnosticOut)`; `fact: Option<u32>` with `#[serde(skip_serializing_if = "Option::is_none", default)]` on every existing arm |
| `v6/sprefa-extract/src/wire.rs` | `flatten_each` takes a `witness: bool`; when true, emits `Protocol` first, one `Run` (mode syntax, tool `extract`, version `env!("CARGO_PKG_VERSION")`, scope = the file digests), numbers every row's `fact`, emits one `Witness` per row with `method=parse`, and one `Coverage partial` per relation family present. When false, output is byte-identical to today |
| `v6/sprefa-extract/src/schema.rs` | six new `record=` lines in `SCHEMA`, spelled as PLAN.md section 4 |
| `v6/sprefa-extract/src/bin/extract.rs` | `--witness` bool flag, threaded into the flatten call sites; `--schema` prints the new lines |
| `v6/sprefa-extract/src/lib.rs` | `pub mod tsi;` |
| `v6/sprefa-extract/tests/96_witness_wire.rs` (new) | tests below |

Forbidden: `src/tsi/registry.rs`, `src/tsi/sink.rs`, `src/tsi/ingest.rs` (A3), `src/project.rs`, `src/lang/**`, `tests/fixtures/resolve/*.jsonl` (goldens stay byte-identical), `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## Wire spelling (verbatim from PLAN.md section 4)

```text
record=protocol  version=<u32>
record=run       run=<u32> mode=syntax|semantic tool=<slug> version=<string> scope=[<digest>...]
record=fact      fact=<u32> relation=<ns.name> args=[<arg>...]
record=witness   fact=<u32> run=<u32> method=<slug>
record=coverage  run=<u32> relation=<ns.name> coverage=partial|complete
record=diagnostic run=<u32> relation=<ns.name> detail=<string>
```

`<arg>` is a tagged object: `{"id":u32}` | `{"span":[digest,start,end]}` | `{"text":s}` | `{"int":n}` | `{"atom":s}`. Use `#[serde(rename_all = "lowercase")]` on `Arg` with externally tagged variants so those exact shapes come out. `Mode` serializes as `syntax` / `semantic`. `CoverageOut.complete: bool` serializes as `coverage: "partial" | "complete"` (custom `Serialize`/`Deserialize` on a `Coverage` newtype, or `#[serde(with)]`). Object keys stay sorted: run every new row through the same key-sort `wire.rs` already applies (`wire.rs:82` onward).

`Method`: the existing `ResolutionOrigin` variants (`src/types.rs:1522`) plus `Parse`, `CheckerWalk`, `Foreign`. Serialize as the snake_case slug (`parse`, `checker_walk`, `foreign`).

Protocol version is the integer `1`. One `pub const PROTOCOL_VERSION: u32 = 1;` in `src/tsi/types.rs`.

## Tests, `tests/96_witness_wire.rs`

| case | input | expected |
|---|---|---|
| protocol first | `extract --witness --family type tests/fixtures/resolve/0_caller.ts` | line 1 decodes to `FlatFact::Protocol { version: 1 }` |
| run second | same | line 2 is `Run` with `mode=syntax`, `tool=extract`, non-empty `scope` |
| flag off | `extract --family type tests/fixtures/resolve/0_caller.ts` with and without `--witness`, stripped of `fact`/envelope rows | identical row sets; and with the flag off no row carries a `fact` key |
| round trip | every line of every `tests/fixtures/resolve/*.jsonl` and every `--witness` line | `serde_json::from_str::<FlatFact>` then `to_string` equals the input after key sort |
| witness per row | `--witness` output | count(Witness rows) == count(rows carrying `fact`); every witness `fact` names a row that exists; every witness `method` is `parse` |
| coverage partial | `--witness` output | every `Coverage` row is `partial`; zero `Diagnostic` rows (syntax runs emit none) |
| schema | `extract --schema` | contains the six new `record=` lines verbatim |

Header of the test file carries a SABOTAGE RECEIPT like `tests/91_origin_column.rs:1-14`: name the assertion that fails on `origin/main` before your change (round trip fails to compile: no `Deserialize`).

## Gate (all three must be run; paste output in the PR)

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo run --features cli --bin extract -- --resolve --family type --project-root tests/fixtures/resolve tests/fixtures/resolve/0_caller.ts | diff - tests/fixtures/resolve/2_resolved_edges.jsonl; echo rc=$?
cd v6/sprefa-extract && cargo test --features cli --test golden_parity 2>&1 | tail -3
```

The diff prints nothing and `rc=0`. If a golden moves, you changed the flag-off path; revert.

## Style laws

- No `eprintln!` in `src/**`; `tracing` only.
- Comments state constraints the code cannot show; no change-log narrative, no dates, no arc names.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- Every new pub type is declared in `src/tsi/types.rs`, never inline in `wire.rs`.
- Build-vs-buy: serde derive only; no hand-written serializers beyond the `coverage` newtype.

## Done

PR against `main` titled `extract: --witness envelope, protocol 1, FlatFact deserialize (TSI A1)`.
`git diff --stat origin/main...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A1 PR #<n>: 96_witness_wire N tests, goldens byte-identical, diff rc=0"`.
