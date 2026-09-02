# REPORT.ts-census

## TOC

1. [Method](#method)
2. [Repo tables](#repo-tables)
3. [Summary: fixture percent vs score](#summary-fixture-percent-vs-score)
4. [Fixture sanity probe](#fixture-sanity-probe)
5. [Per-repo verdicts](#per-repo-verdicts)

## Method

- Clones at the exact SCORES.tsv shas, verified by `git rev-parse HEAD` after checkout:
  - `/tmp/ts-census/umami-software_umami` `ca661c7057984aa98ed4f7083d84dae2f65bfcb0`
  - `/tmp/ts-census/vitejs_vite` `f40efefbb3630cdb7235286bc2b51673d9fbfc27`
  - `/tmp/ts-census/trpc_trpc` `4f5315219f947589c9ef0f67b7aa551243312d61`
- Tuning corpus: `/Users/chrishafley/projects/TypeScript-5.9` at `7e133bea1` (matches SCORES.tsv sha).
- File set: `.ts .tsx .mts .cts` minus `.d.ts` (same as `run.py` `source_files`, run.py:233-242).
- Walk excluded: `.git node_modules dist build` and dot-dirs (subset of `run.py` SKIP_DIRS; none of the three repos has files under the other SKIP_DIRS entries).
- Fixture rule, identical for all three repos and the tuning corpus: a file is FIXTURE if any ancestor directory (relative to repo root) is named one of `test tests __tests__ spec e2e fixtures examples playground`. Otherwise PRODUCT. Stated plainly: name-only rule, no judgment calls per directory.
- SCORES.tsv "files" column (1044 / 555 / 864) is slightly below this census total (1044 / 562 / 950); run.py may drop a few files downstream. The walk-level census is reported here.

## Repo tables

### umami-software/umami (total 1044, fixture 15 = 1.4%)

| dir | files | percent |
|---|---|---|
| src/app | 508 | 48.7% |
| src/components | 281 | 26.9% |
| src/queries | 105 | 10.1% |
| src/lib | 87 | 8.3% |
| src/permissions | 18 | 1.7% |
| scripts | 12 | 1.1% |
| src/store | 10 | 1.0% |
| tests | 9 | 0.9% |
| src/test | 6 | 0.6% |
| root configs + docker + misc | 8 | 0.8% |

### vitejs/vite (total 562, fixture 384 = 68.3%)

| dir | files | percent |
|---|---|---|
| playground | 268 | 47.7% |
| packages/vite/src | 242 | 43.1% |
| packages/create-vite | 22 | 3.9% |
| docs | 7 | 1.2% |
| packages/plugin-legacy | 7 | 1.2% |
| scripts | 6 | 1.1% |
| packages/vite misc (configs, scripts) | 4 | 0.7% |
| root vitest configs | 2 | 0.4% |

Fixture dirs inside vite: `playground` 268, `__tests__` 116, `test` 0. The 116 `__tests__` files sit under `packages/**` and `playground/**`.

### trpc/trpc (total 950, fixture 672 = 70.7%)

| dir | files | percent |
|---|---|---|
| examples | 307 | 32.3% |
| packages/openapi | 176 | 18.5% |
| packages/server | 109 | 11.5% |
| packages/react-query | 84 | 8.8% |
| packages/tests | 82 | 8.6% |
| www | 54 | 5.7% |
| packages/upgrade | 51 | 5.4% |
| packages/client | 45 | 4.7% |
| packages/tanstack-react-query | 24 | 2.5% |
| packages/next | 15 | 1.6% |
| scripts + root config | 3 | 0.3% |

Fixture dirs inside trpc: `examples` 307, `test` 276 (largest single: `packages/openapi/test/routers` 153), `tests` 82, `__tests__` 7. Note `packages/openapi` itself is 176 files of which 153+ test files, so its 18.5% slice is mostly fixture too; product fraction is smaller than the table row suggests.

### TypeScript-5.9 tuning corpus (total 19944, fixture 19343 = 97.0%)

| dir | files | percent |
|---|---|---|
| tests | 19343 | 97.0% |
| src/testRunner | 279 | 1.4% |
| src/services | 168 | 0.8% |
| src/compiler | 77 | 0.4% |
| src/harness | 38 | 0.2% |
| src/server + other src | 17 | 0.1% |
| scripts | 1 | 0.0% |

(Receipt earlier said 19,612 total / 18,715 in tests/cases at 95.4%; this walk counts all of `tests/` not just `tests/cases/`, hence 97.0% vs 95.4%. Same disease, wider boundary.)

## Summary: fixture percent vs score

| repo | total | product | fixture | fixture % | recall | precision |
|---|---|---|---|---|---|---|
| umami-software/umami | 1044 | 1029 | 15 | 1.4% | 25.92 | 60.60 |
| vitejs/vite | 562 | 178 | 384 | 68.3% | 15.62 | 25.77 |
| trpc/trpc | 950 | 278 | 672 | 70.7% | 8.61 | 11.07 |
| TypeScript-5.9 (tuning) | 19944 | 601 | 19343 | 97.0% | 1.61 | 28.24 |

Fixture percent is monotone with (1 - recall) across the three held-out repos: 1.4% / 68.3% / 70.7% against recall 25.92 / 15.62 / 8.61. Precision drops the same direction: 60.60 / 25.77 / 11.07.

## Fixture sanity probe

Probe repo: vitejs/vite, largest fixture dir `playground` (265 walk-reachable files after config exclusions).

| metric | value |
|---|---|
| files | 265 |
| median lines | 17 |
| mean lines | 70.0 |
| files with a relative import | 66 (24.9%) |

Contrast probe, trpc product-side test dir `packages/openapi/test` (168 files):

| metric | value |
|---|---|
| median lines | 140 |
| mean lines | 174.1 |
| files with a relative import | 93 (55.4%) |

Fixture suite confirmed on vite: median 17 lines, only a quarter of files carry a sibling import. trpc's test dir is denser (median 140) but still import-heavy, which caps how much product signal it carries.

## Per-repo verdicts

- umami-software/umami: score plausibly NOT explained by fixture composition (1.4% fixture); its 60.60 precision is the clean-corpus number and recall 25.92 is measured on real product code.
- vitejs/vite: composition plausibly explains the precision gap to umami (68.3% fixture vs 1.4%); recall 15.62 is scored over 43% product, so a product-only rescore would move it.
- trpc/trpc: composition plausibly explains most of the outlier numbers (70.7% fixture, plus `packages/openapi` mostly test files); recall 8.61 over 29% product files is the weakest-signal corpus of the three.
- TypeScript-5.9 tuning corpus: 97.0% fixture under the same rule, confirming the tuning receipt (95.4% for `tests/cases` specifically).
