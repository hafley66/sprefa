# Lane C — unread-rel skip contract rails (REPORT)

Lane C of `plans/2026-08-06-unread-rel-skip-contract.md` §8. Base `e1a9696f`
(ff-only, clean). Owns two files: `v6/tsv2/scripts/nolisten-text-audit.mjs`
(RAIL A) and `v6/tsv2/tests/nolistenCounts.test.ts` (RAIL C). No runtime, `.pl`,
`gen_emitted`, or existing test was edited by hand.

## Gates

| gate | command | result |
|---|---|---|
| RAIL A audit | `node scripts/nolisten-text-audit.mjs` (from `v6/tsv2`) | exit 0; `207 modules, 280 unobserved rels (of 752), 2568 staging refs, 0 violations` |
| test suite | `pnpm test` (from `v6/tsv2`) | 157 tests, 155 pass, 0 fail, 2 skipped |
| typecheck | `pnpm exec tsgo --noEmit` (from `v6/tsv2`) | 0 errors |

The 2 skipped tests are pre-existing conditional skips, not added here:
`crawlOrg.test.ts` (no in-tree release extractor check) and
`serveLeak.test.ts` (soak flag `DL_PERF_LOG`). `nolistenCounts.test.ts` runs and
passes with 0 skips.

## RAIL A allowlist as landed

For each relation whose `ruleObservers` is empty, the audit scans its three
staging tables (`__delta_<rel>`, `__frontier_<rel>`, `__next_frontier_<rel>`) in
every SQL statement of the module text and requires each match to fall in this
set:

| category | test | note |
|---|---|---|
| DDL | statement begins `CREATE` | tables exist whatever the observer set says |
| writer | statement begins `INSERT` | the copy statements the skip drops stay in text |
| clear | statement begins `DELETE` | whole-table clears still render |
| boundary SELECT | statement begins `SELECT` and reads the rel's own `__delta_<rel>` with `" _sign" IN (-1, 1)` | the rel's own `boundarySql` read, always live |

Any other statement that reads a skipped rel's staging exits 2, printing the
module, the rel, the table, and the offending statement. Table names match by
token boundary (`__frontier_head` does not match inside `__frontier_head_move`).

## Sabotage receipt (fail-first)

Scratch copy of `bool_identity_comparison_filters.ts` (not in `gen_emitted`)
with one planted line: `` `const FAKE_DELTA_READ = `SELECT "name" FROM "__delta_enabled_name"`;` ``
where `enabled_name` has empty `ruleObservers`. `node scripts/nolisten-text-audit.mjs <scratch>` output:

```
[violation] module=bool_identity_comparison_filters.ts rel=enabled_name table=__delta_enabled_name
  SELECT "name" FROM "__delta_enabled_name"
nolisten text audit: 1 modules, 1 unobserved rels (of 2), 12 staging refs, 1 violations
```

exit 2. The receipt is recorded in the test-header comment
(`tests/nolistenCounts.test.ts`), which is exempt from the comment budget, and
reproduces by planting the same fake read.

## nolistenCounts.test.ts (RAIL C)

Two-relation fixture `bool_identity_comparison_filters`: `flag` (observed,
`ruleObservers: ["enabled_name/1"]`) and `enabled_name` (derived,
`ruleObservers: []`, skip-able). The test builds the seam with
`seam.unobservedRels = { enabled_name }`, wraps the real runner to count SQL per
statement (the recordingRunner pattern), and pins:

- statements-per-tick with the skip active (12) is exactly 8 fewer than with it
  inactive (20), flat at 5 and 100 source rows;
- the final `enabled_name` head is byte-identical in both runs;
- non-vacuity: the head derives 5 and 100 rows respectively.

## Deviations

- Contract §6 RAIL C prose prints "exactly ten fewer per skipped rel". The
  measured per-tick count for this fixture is 8. The contract §4b set as it
  fires here: `prepareTick` clears (2), `stageEvents` boundary+frontier stage
  (2), `readBoundary` boundary read (1), `promoteFrontiers` (3). `mergeNextIntoCurrent`
  (`#9`) does not fire on this tick path and retention (`#10`) is absent; the
  retraction-guard and merge terms are combined-SQL text changes, not statement
  count changes. Pinned to the measured 8 rather than the prose 10 so the test
  asserts what the runtime does.
- The pre-commit comment rail demanded a missing cargo binary. Built it as
  directed: `cargo build --release --features cli --bin extract` in
  `v6/sprefa-extract` (`target/release/extract`, gitignored).
- Fresh checkouts have no `node_modules`; installed with `pnpm install
  --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js`. No lockfile was
  modified.
- The sabotage receipt lands in the exempt test header (the `.mjs` rail script
  is not exempt from the comment budget, so its header is held to two comment
  lines).
