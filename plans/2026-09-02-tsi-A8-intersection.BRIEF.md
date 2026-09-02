# brief: TSI A8, the shared-fixture intersection test

Lane: `feature/tsi-a8-intersection`. Base: the `origin/main` sha AFTER the A6 PR merges (coordinator states it; both semantic adapters must exist).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, acceptance criterion 8: "Equivalent TypeScript and Rust fixtures produce shared TSI relations for their intersecting semantics and namespaced relations for native meaning."
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` section 9 (A8 row), section 10 (case "intersection").

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/tests/100_tsi_intersection.rs` (new) | the test below |
| `v6/sprefa-extract/tests/fixtures/tsi/probe.ts` | ONLY if a construct is missing to make the two fixtures equivalent; state every edit in the PR |
| `v6/sprefa-extract/tests/fixtures/tsi/rust_probe/src/lib.rs` | same rule |
| `v6/sprefa-extract/src/tsi/ingest.rs` | ONLY `pub fn canonical_rows(lines) -> Result<Vec<CanonicalFact>, IngestError>` if `ingest()` does not already expose the renumbered rows as data; no behavior change to `ingest()` |

Forbidden: everything else under `src/`, `tests/fixtures/resolve/**`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## The test, `tests/100_tsi_intersection.rs`

Requires `--features cli,ts-checker,rust-checker`; `#![cfg(all(feature = "ts-checker", feature = "rust-checker"))]`.

```text
step 0  ts   = extract --witness --resolve --family type --project-root tests/fixtures/tsi --ts-checker probe.ts, piped through --ingest
step 1  rust = same over rust_probe with --rust-checker, piped through --ingest
step 2  strip: drop protocol/run/witness/coverage/diagnostic rows; drop tsi.origin (spans differ by construction) and tsi.has_type (occurrence spans)
step 3  project each fact to its shape: (relation, args with every id replaced by the name of the tsi.type it denotes via tsi.denotes, or by its primitive class, or "_" for an anonymous id)
step 4  shared   = { rows with relation in tsi.* }         -> assert ts == rust as sets, print the symmetric difference on failure
step 5  native   = { rows with relation in ts.* } and { rust.* } -> assert both non-empty, assert disjoint relation names
step 6  assert every relation name in the union is in REGISTRY
```

Expected equal `tsi.*` set for the probe pair, at minimum: `tsi.product(User)`, `tsi.product(Mapper)` or its trait twin as `tsi.type(Mapper)` (a ts interface and a rust trait are both contracts; record which relation each side spells and put the discrepancy, if any, in the PR as a fork for Chris, do not paper over it), `tsi.edge(User, id, T, 0)`, `tsi.edge(User, name, _, 1)`, `tsi.parameter(T, User, 0, invariant)`, `tsi.callable(map)`, `tsi.conforms(User, Mapper, _)`.

Known asymmetries to assert as NATIVE, never as shared: ts `name?: string` is `ts.optional` while rust `name: Option<String>` is `tsi.called(_, Option, _)`; the PR lists this and any other one found.

Header carries a SABOTAGE RECEIPT: with either adapter's `tsi` rows removed the shared set is empty on that side.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli,ts-checker,rust-checker --test 100_tsi_intersection 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
```

Build the rust-checker feature in the background once (380s cold); never foreground-wait on it.

## Style laws

- No `eprintln!`; `tracing` only.
- Comments: constraints only. No dates, no arc names.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- Descriptive names in the test (`shared_rows_ts`, never `a`).

## Done

PR titled `extract: ts and rust probe fixtures share their TSI rows (TSI A8)`.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A8 PR #<n>: intersection equal, N native asymmetries listed"`.
