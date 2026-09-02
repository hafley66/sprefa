# brief: TSI A2, the witness envelope over `--resolve` and the multi-witness fold

Lane: `feature/tsi-a2-multi-witness`. Base: the `origin/main` sha AFTER the A3 PR merges (coordinator states it; A3 edits `extract.rs` too).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, `## Decisions`: identity rule 5 ("Fact IDs derive from relation plus canonical arguments, allowing syntax and semantic runs to witness the same fact").
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` sections 4, 5 (rule 5 row), 7 (`WitnessOut`), 10 ("two witnesses" case).
- Landed: A1 (`src/tsi/types.rs`: `RunOut`, `WitnessOut`, `CoverageOut`, `Method`; `--witness` on the per-file stream, `extract.rs:181-189`), A3 (`src/tsi/{registry,sink,ingest}.rs`).

Delivers the read side of criterion 9 and the `run` row for the resolve path.

## What is wrong today

Every resolve fold short-circuits on the checker: `ts.rs:3493-3502` (type), `ts.rs:4212-4222` (call), `rust.rs:385-393` (type), `rust.rs:1150-1173` (call). The first leg to answer is the only leg recorded, as `ProjectEdge.origin` (`types.rs:1595`), one `ResolutionOrigin` per edge. A consumer cannot tell "the checker and the same-file leg agree" from "only the checker answered". `--witness` `conflicts_with` `resolve` (`extract.rs:184`), so no resolve row is numbered.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/types.rs` | `ProjectEdge<F>` gains `pub witnesses: Vec<ResolutionOrigin>`; `ProjectEdge::new` keeps its signature and sets `witnesses = vec![origin]`; a new `ProjectEdge::witnessed_by(mut self, extra: ResolutionOrigin) -> Self` pushes when absent. `origin` stays the top rank |
| `v6/sprefa-extract/src/lang/ts.rs` | at the four fold sites: when `cx.witness` is on, the syntax legs still run after the checker answers; a leg whose `(blob, span)` equals the chosen target calls `witnessed_by(leg)`; a leg that disagrees pushes its own `ProjectEdge` with `origin = leg`, `witnesses = vec![leg]`. When the flag is off the code path is today's, byte for byte |
| `v6/sprefa-extract/src/lang/rust.rs` | same, both sites |
| `v6/sprefa-extract/src/project.rs` | `ResolveRequest` gains `pub witness: bool`; `ProjectCx` carries it; `resolve_project` under the flag prepends `Protocol`, one `Run` per tier that ran (`mode=syntax tool=extract` always; `mode=semantic tool=tsc` / `tool=rust-analyzer` when that index loaded, `project.rs:294-312`), numbers every `ResolvedEdge` / `ResolvedTypeEdge` row with `fact`, emits one `Witness` per entry of `witnesses` (method = that origin's slug; the checker origin's witness carries the semantic run id, every other origin the syntax run id), and one `Coverage partial` per family (`extract.type`, `extract.call`) for the syntax run. No `Coverage complete` anywhere in this arc: the checker tier answers per site, it does not enumerate (`Method::CheckerWalk` is A5/A6) |
| `v6/sprefa-extract/src/bin/extract.rs` | remove `"resolve"` from the `--witness` `conflicts_with_all`; thread `cli.witness` into `ResolveRequest` |
| `v6/sprefa-extract/src/schema.rs` | the `TSI ENVELOPE` paragraph: `--resolve` is now covered; state the two-run shape |
| `v6/sprefa-extract/tests/98_resolve_witness.rs` (new) | tests below |
| `v6/sprefa-extract/tests/fixtures/tsi/agree.ts`, `agree_callee.ts` (new) | one call and one `extends` where the same-file leg and the checker name the same target |

Forbidden: `src/tsi/**` (A1/A3, merged; missing type = STOP AND REPORT), `src/wire.rs`, `src/lang/ts_checker.mjs`, `src/lang/rust_checker_ra.rs`, `src/lang/{go,kotlin,python,prolog,dl6,markdown}/**` (no fold change there; their `ProjectEdge::new` calls compile unchanged), `tests/fixtures/resolve/**`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

`ProjectEdge` is constructed at 22 sites (`grep -rc 'ProjectEdge::new(' src`); the new field is set inside `new`, so none of them changes.

## Ordering law

`origin` ranks as today: `Checker` beats every syntax leg, `Scip` beats name match, the rest in today's fall-through order. `witnesses` is sorted by `ResolutionOrigin`'s derived `Ord` before emission so the wire is stable. `hosts.rs` in `sprefa-engine-rs` keeps reading `resolution_origin`; that column's value is unchanged for every row that exists today.

## Tests, `tests/98_resolve_witness.rs`

Runs under `--features cli` for the syntax-only cases, `--features cli,ts-checker` for the checker cases (`#[cfg(feature = "ts-checker")]` per test, the way `tests/92_ts_checker.rs` does).

| case | input | expected |
|---|---|---|
| flag off unchanged | `extract --resolve --family call --project-root tests/fixtures/resolve tests/fixtures/resolve/0_caller.ts tests/fixtures/resolve/1_callee.ts` | equals `tests/fixtures/resolve/2_resolved_edges.jsonl` byte for byte |
| protocol first on resolve | same with `--witness` | line 1 `protocol`, line 2 `run mode=syntax`; no `run mode=semantic` (no checker flag) |
| one witness per leg, syntax | same | every `resolved_edge` carries `fact`; witness count == number of rows; every method equals the row's `resolution_origin` |
| two witnesses | `--witness --ts-checker --project-root tests/fixtures/tsi tests/fixtures/tsi/agree.ts tests/fixtures/tsi/agree_callee.ts` | the agreeing call row: `resolution_origin=checker`, exactly 2 witness rows on its `fact`: `method=checker` on the semantic run, `method=same_file` (or `corpus_unique`, whichever leg answers the fixture; assert the one you built) on the syntax run |
| two runs | same | exactly two `run` rows: `mode=syntax tool=extract`, `mode=semantic tool=tsc` |
| disagreeing leg | a fixture where the corpus-unique leg names a different def than the checker | two `resolved_edge` rows for the site, distinct `fact` ordinals, one witness each; `hosts.rs` consumers see the checker row's `resolution_origin=checker` and the other row's `corpus_unique` |
| coverage | any `--witness --resolve` stream | `coverage partial` for `extract.call` and `extract.type` on the syntax run; zero `coverage` rows on the semantic run; zero `diagnostic` rows |
| ingest round trip | `--witness --resolve` stream piped into `extract --ingest /dev/stdin` | rc=0, idempotent |

Header carries a SABOTAGE RECEIPT: on the base sha `--witness --resolve` is a clap conflict (rc=2), and `ProjectEdge` has no `witnesses` field.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli,ts-checker --test 98_resolve_witness --test 92_ts_checker 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test golden_parity --test 1_resolve_cli 2>&1 | tail -3
```

The `rust-checker` feature is a 380s cold build (`Cargo.toml:217-219`); do not add it to the gate. The rust fold sites get the same edit as ts and are covered by `1_resolve_cli` flag-off byte identity; state in the PR that the rust checker witness case is untested in this arc.

## Cost law

With `--witness` off, no extra leg runs: the short-circuit `continue` / `return` stays exactly where it is, behind `if !cx.witness`. Bench (`tests/bench`, `plans/extract-bench-2026-08-29/RATCHET.tsv`) must not move; do not run the bench, the ratchet test runs in `cargo test`.

## Style laws

- No `eprintln!`; `tracing` only.
- Comments: constraints only. No dates, no arc names, no "A2".
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- No per-site allocation when the flag is off: `witnesses` is `vec![origin]` (one alloc, same as today's edge push would be if boxed; if the bench ratchet moves, switch to `SmallVec<[ResolutionOrigin; 2]>` only if `smallvec` is already in the tree, else an inline `[Option<ResolutionOrigin>; 3]`).

## Done

PR titled `extract: --witness over --resolve, every leg is a witness (TSI A2)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A2 PR #<n>: 98_resolve_witness N tests, goldens byte-identical, ratchet unmoved"`.
