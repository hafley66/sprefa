# sprefa-extract corpus battery: TypeScript / JavaScript arm

## Contents

1. [Corpus](#1-corpus)
2. [Extension roster](#2-extension-roster)
3. [Step 1: per-file default](#3-step-1-per-file-default)
4. [Step 2: per-file by family](#4-step-2-per-file-by-family)
5. [Step 3: --resolve per directory](#5-step-3---resolve-per-directory)
6. [Step 4: --family diet_scip](#6-step-4---family-diet_scip)
7. [Step 5: --family scip](#7-step-5---family-scip)
8. [Perf and RSS](#8-perf-and-rss)
9. [Findings](#9-findings)
10. [Landed fix: module bodies](#10-landed-fix-module-bodies)
11. [What stays untested and why](#11-what-stays-untested-and-why)

Binary: `v6/sprefa-extract/target/release/extract`, `cargo build --release
--features cli`, base sha `8e946ada9`. Steps 1 and 2 ran the base binary; steps
3, 4, 5 and the perf table ran the binary with the module-bodies fix. The fix
changes no number in steps 1 through 5 (it adds fact rows on 827 files, none of
them a directory or root any later step touches).

---

## 1. Corpus

| root | ts/tsx/mts/cts/js/mjs/cjs files | not under node_modules |
|---|---|---|
| `~/projects/instant` | 1,052,877 | 18,339 |
| `~/projects/hafley-rxjs` | 1,047,351 | 9,015 |
| `~/projects/sprefa/v6/tsv2` | 4,451 | 607 |
| **raw total** | **2,104,679** | 27,961 |

The raw total is dominated by duplicated package installs: pnpm store copies,
per-worktree `node_modules`, and nested installs. The run set collapses them by
the path suffix after the last `node_modules/`, keeping one copy of each
distinct package-relative file.

| set | files |
|---|---|
| not under `node_modules` (kept whole) | 27,961 |
| under `node_modules`, deduplicated | 97,883 |
| **run set** | **125,844** |

| extension | files in run set |
|---|---|
| `.js` | 60,352 |
| `.d.ts` | 38,577 |
| `.ts` (non-declaration) | 16,175 |
| `.mjs` | 5,169 |
| `.mts` | 2,503 |
| `.tsx` | 2,094 |
| `.cjs` | 611 |
| `.cts` | 363 |

---

## 2. Extension roster

`TsSource::matches` delegates to `source_type_for`
(`src/lang/ts.rs:92-103`). Every extension the brief named is accepted; none is
rejected.

| written | accepted | oxc SourceType |
|---|---|---|
| `.tsx` | yes | `tsx()` |
| `.ts`, `.mts`, `.cts` | yes | `ts()` |
| `.js`, `.jsx`, `.mjs`, `.cjs` | yes | `jsx()` |
| `.d.ts`, `.d.mts`, `.d.cts` | yes, through the `.ts`/`.mts`/`.cts` arms | as above |
| `.kts` | routed to `KotlinSource` | n/a |

`"x.kts".ends_with(".ts")` is true, so `TsSource` would claim a Kotlin script.
`KotlinSource` precedes `TsSource` in the roster (`src/lang/mod.rs:73-86`) and
first match wins, which `src/lang/mod.rs:130-132` states as the law. Verified:
`extract --family type q.kts` emits `{"kind":"function","name":"main"}` from the
Kotlin arm.

---

## 3. Step 1: per-file default

`extract <file>`, one process per file, 12 parallel workers, every call wrapped
in `timeout 10`. Raw table: `ts.runs.tsv.gz` (`path rc ms bytes lines`, gzipped
because the plain TSV is 23 MB).

| measure | value |
|---|---|
| files | 125,844 |
| rc != 0 | **0** |
| rc = 124 (timeout) | **0** |
| non-empty stderr first line | **0** |
| total bytes read | 1,506 MB |
| total fact lines emitted | 584,383,597 |

Wall ms per file, under 12-way load:

| p | ms |
|---|---|
| min | 5 |
| p50 | 51 |
| p90 | 110 |
| p99 | 271 |
| p99.9 | 2,440 |
| max | 10,152 |

The 10,152 ms row is `mermaid.min-D8Lrnn0D.js` emitting 1,993,825 fact lines;
`rc` is 0, so `extract` itself finished inside the 10 s budget and the remainder
is the harness draining a 2 M line pipe. Re-timed serially, that file's family
sum runs in 2.57 s. No invocation in the battery broke the 10-second law.

### Files emitting zero facts

1,709 files produced an empty stream with rc=0. Every one is a gzip-compressed
blob written with a `.js`/`.mjs` name under
`instant/src-tauri/target/release/build/*/out/tauri-codegen-assets/`; the first
bytes are `1f 8b`. Zero text file in the corpus produced zero facts.

Genuinely broken TypeScript does NOT go silent: `export function broken( {` plus
garbage emits 19 rows including four `{"kind":"ERROR"}` CST nodes, so a parse
failure over text is legible in the stream. Binary input is the case with no
marker (finding F5).

---

## 4. Step 2: per-file by family

200-file sample: the 100 largest by bytes plus 100 drawn at random from the run
set. For each, `extract <f>` and `--family cst|type|call|df`.

| check | result |
|---|---|
| files | 200 |
| rows where `cst + type + call + df != default` | **0** |

The four family masks partition the default stream exactly. No family sum
exceeds the default on any sampled file.

Shape of that partition on the largest file
(`monaco-editor/dev/vs/assets/ts.worker-DrA3GP0m.js`, 12.7 MB):

| family | lines | share |
|---|---|---|
| cst | 2,597,549 | 96.2% |
| call | 101,994 | 3.8% |
| type | 160 | 0.006% |
| df | 1 | 0.00004% |
| default | 2,699,704 | 100% |

---

## 5. Step 3: `--resolve` per directory

Top 30 source directories with 2+ files at depth 1, `dist`/`target`/`build`/
worktree/generated trees excluded. All rc=0, all under 10 s.

| files | resolved_edge | unresolved | ms | directory |
|---|---|---|---|---|
| 144 | 2,828 | 0 | 272 | `instant/src` |
| 75 | 936 | 0 | 630 | `v6/tsv2/tests` |
| 33 | 125 | 0 | 174 | `hafley-rxjs/packages/signals/src` |
| 23 | 52 | 0 | 130 | `hafley-rxjs/packages/grapht/src` |
| 22 | 117 | 0 | 152 | `instant/e2e` |
| 20 | 111 | 0 | 269 | `v6/tsv2/runtime` |
| 19 | 111 | 0 | 132 | `hafley-rxjs/packages/md/src` |
| 18 | 361 | 0 | 286 | `v6/tsv2/scripts` |
| 18 | 83 | 0 | 122 | `hafley-rxjs/packages/scene/src` |
| 17 | 80 | 0 | 127 | `instant/src/lib/json-rx` |
| 16 | 82 | 0 | 124 | `hafley-rxjs/packages/marbler/src` |
| 16 | 69 | 0 | 119 | `hafley-rxjs/packages/grapht/adapters/3_render_canvaskit` |
| 15 | 128 | 0 | 125 | `instant/extension/src` |
| 15 | 92 | 0 | 121 | `hafley-rxjs/packages/grapht/tests` |
| 14 | 38 | 0 | 129 | `instant/labs/json-rx-mvp` |
| 12 | 27 | 0 | 123 | `instant/src/reactive` |
| 11 | 27 | 0 | 134 | `instant/src/plugins/metrics` |
| 11 | 59 | 0 | 146 | `hafley-rxjs/packages/boop-adapters/src` |
| 10 | 1 | 0 | 117 | `hafley-rxjs/packages/rxjs-ext/src` |
| 10 | 153 | 0 | 132 | `hafley-rxjs/packages/grid/src` |
| 10 | 108 | 0 | 118 | `hafley-rxjs/packages/grapht/adapters/2_render_cytoscape` |
| 9 | 132 | 0 | 495 | `v6/tsv2/serve` |
| 9 | 55 | 0 | 145 | `hafley-rxjs/packages/virtualizations/src` |
| 9 | 10 | 0 | 164 | `hafley-rxjs/packages/react-dock-and-flow/src` |
| 9 | 65 | 0 | 131 | `hafley-rxjs/packages/devtool-plugin/src/0_runtime_hmr` |
| 9 | 62 | 0 | 127 | `hafley-rxjs/packages/devtool-plugin/src/0_runtime` |
| 8 | 20 | 0 | 159 | `instant/src/plugins/files` |
| 8 | 95 | 0 | 128 | `instant` |
| 8 | 35 | 0 | 131 | `hafley-rxjs/packages/json-rx/src` |
| 8 | 50 | 0 | 115 | `hafley-rxjs/packages/grapht/adapters/1_layout_grid_wasm/src` |

The `unresolved` column is 0 for every directory because `--resolve` emits ONE
record kind and it is `resolved_edge` (finding F6). The `unresolved` record
exists (`--schema:26`, reasons `dynamic-import | computed-member-call |
spread-call-args`) but only per-file mode carries it.

### Unresolved ratio, computed by hand

Ratio = `(call sites in the directory - resolved_edge) / call sites`.

| files | sites | edges | ratio | directory |
|---|---|---|---|---|
| 10 | 130 | 1 | 0.992 | `hafley-rxjs/packages/rxjs-ext/src` |
| 22 | 1,855 | 117 | 0.937 | `instant/e2e` |
| 14 | 529 | 38 | 0.928 | `instant/labs/json-rx-mvp` |
| 33 | 1,668 | 125 | 0.925 | `hafley-rxjs/packages/signals/src` |
| 23 | 629 | 52 | 0.917 | `hafley-rxjs/packages/grapht/src` |
| 15 | 1,086 | 92 | 0.915 | `hafley-rxjs/packages/grapht/tests` |
| 16 | 858 | 82 | 0.904 | `hafley-rxjs/packages/marbler/src` |
| 16 | 639 | 69 | 0.892 | `.../grapht/adapters/3_render_canvaskit` |
| 20 | 970 | 111 | 0.886 | `v6/tsv2/runtime` |
| 11 | 192 | 27 | 0.859 | `instant/src/plugins/metrics` |
| 10 | 1,056 | 153 | 0.855 | `hafley-rxjs/packages/grid/src` |
| 19 | 759 | 111 | 0.854 | `hafley-rxjs/packages/md/src` |
| 11 | 402 | 59 | 0.853 | `hafley-rxjs/packages/boop-adapters/src` |
| 12 | 180 | 27 | 0.850 | `instant/src/reactive` |
| 18 | 495 | 83 | 0.832 | `hafley-rxjs/packages/scene/src` |
| 18 | 1,829 | 361 | 0.803 | `v6/tsv2/scripts` |
| 75 | 4,453 | 936 | 0.790 | `v6/tsv2/tests` |
| 15 | 505 | 128 | 0.747 | `instant/extension/src` |
| 17 | 298 | 80 | 0.732 | `instant/src/lib/json-rx` |
| 144 | 8,529 | 2,828 | 0.668 | `instant/src` |

Top of the list opened and classified. `rxjs-ext/src`, 130 sites, 1 edge:

| callee | count | class |
|---|---|---|
| `log` | 19 | builtin (`console.log`) |
| `pipe`, `map`, `subscribe`, `unsubscribe`, `scan`, `merge`, `tap`, `next`, `defer`, `complete`, `of`, `timer`, `switchMap`, `delay`, `startWith`, `isObservable`, `Observable` | 74 | external library (rxjs) |
| `keys`, `stringify`, `replace`, `startsWith`, `error` | 12 | builtin (`Object`, `JSON`, `String`) |
| rest | 25 | rxjs operators and locals in files outside the depth-1 set |

Every unresolved site in the worst directory has no definition inside the
supplied file set. The ratio is correct, not a gap: the package is a thin
wrapper over rxjs and calls almost nothing it defines. The same classification
holds for `instant/e2e` (playwright and vitest), `signals/src` (rxjs) and
`grapht/tests` (vitest). `instant/src` at 0.668 is the floor, and its remainder
is `react`, `rxjs`, `@tauri-apps/*` and builtins.

### Barrel re-exports

`a.ts` defines `deepHelper`, `index.ts` is `export * from "./a"`, `consumer.ts`
imports from `"./index"` and calls it. `--resolve a.ts index.ts consumer.ts`
emits the `consumer -> a` edge. Name matching does not have to follow the
barrel, so barrels cost nothing here.

### `--deps` and tsconfig `paths`

`extract --deps --project-root . <files>` over `instant/src` (144 files) gives
354 `file_edge` and 212 `file_unresolved`:

| reason | rows |
|---|---|
| `node_modules_boundary` | 168 |
| `relative_unresolved` | 44 |

The top unresolved modules are `vitest` (42), `react` (23), `./generated/native`
(22, a real missing file), `rxjs` (21). Every reason slug is correct for its
module. `instant/tsconfig.json` declares one `paths` entry
(`@hafley66/boop-adapters`), which no depth-1 file under `src` imports, so the
alias arm went unexercised here.

---

## 6. Step 4: `--family diet_scip`

Same 20 directories, same file sets.

| files | diet resolved_edge | step 3 resolved_edge | diet resolved_type_edge | ms | directory |
|---|---|---|---|---|---|
| 144 | 2,828 | 2,828 | 517 | 324 | `instant/src` |
| 75 | 936 | 936 | 44 | 587 | `v6/tsv2/tests` |
| 33 | 125 | 125 | 128 | 182 | `signals/src` |
| 23 | 52 | 52 | 94 | 122 | `grapht/src` |
| 22 | 117 | 117 | 4 | 165 | `instant/e2e` |
| 20 | 111 | 111 | 244 | 374 | `v6/tsv2/runtime` |
| 19 | 111 | 111 | 33 | 282 | `md/src` |
| 18 | 361 | 361 | 109 | 313 | `v6/tsv2/scripts` |
| 18 | 83 | 83 | 101 | 138 | `scene/src` |
| 17 | 80 | 80 | 114 | 125 | `instant/src/lib/json-rx` |
| 16 | 82 | 82 | 58 | 144 | `marbler/src` |
| 16 | 69 | 69 | 72 | 231 | `3_render_canvaskit` |
| 15 | 128 | 128 | 95 | 237 | `instant/extension/src` |
| 15 | 92 | 92 | 2 | 120 | `grapht/tests` |
| 14 | 38 | 38 | 75 | 123 | `instant/labs/json-rx-mvp` |
| 12 | 27 | 27 | 13 | 154 | `instant/src/reactive` |
| 11 | 27 | 27 | 18 | 155 | `instant/src/plugins/metrics` |
| 11 | 59 | 59 | 44 | 126 | `boop-adapters/src` |
| 10 | 1 | 1 | 3 | 111 | `rxjs-ext/src` |
| 10 | 153 | 153 | 46 | 116 | `grid/src` |

`diet_scip` and `--resolve` produce the **identical** `resolved_edge` set on all
20 directories. `diet_scip` additionally emits `resolved_type_edge`, which
`--resolve` emits only under `--family type`. Wall time is within noise of step
3. On the ts arm the two modes differ in emitted record kinds, not in call-edge
content.

---

## 7. Step 5: `--family scip`

`scip-typescript` 0.4.0 at `~/.nvm/versions/node/v24.15.0/bin/scip-typescript`.
Three roots, each `extract --family scip .` from the root.

| root | rc | scip_skip | reused | documents | scip_def | scip_ref | scip_fn_edge | scip_edge | scip_impl |
|---|---|---|---|---|---|---|---|---|---|
| `v6/tsv2` | 0 | 0 | false | 527 | 50,627 | 66,154 | 68,935 | 3,538 | 30 |
| `hafley-rxjs` | 0 | 0 | false | 92 | 2,852 | 2,276 | 2,223 | 173 | 18 |
| `instant` | 0 | 0 | false | 197 | 5,514 | 5,672 | 6,581 | 550 | 21 |

Every root indexed on the first try, no skip rows, no reuse.

**Coverage.** `instant` has 18,339 source files outside `node_modules`; the index
covers 197 documents. `hafley-rxjs` has 9,015; the index covers 92. The
`scip_index` row reports `documents` and nothing says which files fell outside
the root `tsconfig.json`'s include set (finding F7).

### `scip_fn_edge` against `resolved_edge`

Same package on both sides: `hafley-rxjs/packages/boop-adapters/src`, 11 files.

| relation | rows | distinct (caller, callee) pairs |
|---|---|---|
| `scip_fn_edge` (scip arm) | 209 | 184 |
| `resolved_edge` (`--resolve`) | 59 | 47 |
| pairs in both | | **0** |

Zero overlap, because the two relations name different things. Twenty sampled
from each side:

| side | sample | class |
|---|---|---|
| scip-only | `communicationOf -> id`, `-> kind`, `-> label`, `-> nodes`, `-> parentId`, `-> cwd`, `-> harness`, `-> metadata`, `-> message`, `-> nodeId`, `-> from`, `-> edges`, `-> events`, `-> event`, `-> children`, `-> communications`, `-> name` (17) | **property reference, not a call**: the callee symbol ends in `#field.` or `#X:Y.` |
| scip-only | `asTestRows -> projectAgentNetwork`, `asTestRows -> NetworkTestRow` (2) | real call plus one type reference |
| scip-only | `asTestRows -> nodes` (1) | property reference |
| resolve-only | `closure@1759 -> asTestRows`, `closure@2141 -> findRow`, `closure@2492 -> projectAgentNetwork`, `closure@2544 -> projectAgentTree`, `closure@2847 -> projectAgentNetworkTopology`, `closure@3282 -> flattenNetworkRows`, `closure@3656 -> projectAgentTimeline`, `closure@612 -> timeOf` and 10 more (18) | **caller granularity**: the ts arm names an arrow callsite `closure@<offset>`, scip names the enclosing declaration |
| resolve-only | `communicationOf -> eventIdentity`, `communicationOf -> eventStart` (2) | real call scip records under a differently spelled caller symbol |

Symbol-shape histogram over the 209 scip rows confirms the first class:

| callee shape | rows |
|---|---|
| `X#X:X.X` (type-member field) | 90 |
| `X(X)X.X` and variants (call-ish) | 63 |
| `X#X` (class member) | 26 |
| `X.X` (plain member) | 19 |
| `X:X` | 11 |

A `scip_fn_edge` count is not comparable to a `resolved_edge` count. The schema
line (`record=scip_fn_edge caller=<string> callee=<string>`) says nothing about
which references qualify (finding F8).

---

## 8. Perf and RSS

The 20 largest files, serial, `/usr/bin/time -l`.

| MB | s | max RSS MB | file |
|---|---|---|---|
| 12.7 | 3.44 | **1,117** | `monaco-editor/dev/vs/assets/ts.worker-DrA3GP0m.js` |
| 12.1 | 2.58 | **1,030** | `monaco-editor/dev/vs/language/typescript/ts.worker.js` |
| 8.7 | 2.34 | **938** | `monaco-editor/esm/.../typescriptServices.js` |
| 8.5 | 2.33 | **942** | `instant/node_modules/.ignored_typescript/lib/typescript.js` |
| 8.5 | 2.32 | **926** | `typescript@5.6.3/lib/typescript.js` |
| 7.8 | 2.57 | **1,058** | `instant/node_modules/.ignored_mermaid/dist/mermaid.js` |
| 8.1 | 0.36 | 179 | `dist/assets/d2-BzowuzUS.js` (minified) |
| 7.9 | 0.24 | 122 | `dist/assets/0_DiagramLightbox-B1h0L6_1.js` (minified) |

Peak is 1,117 MB resident for a 12.7 MB input: **88x the file**. Where it goes,
same file, one family at a time:

| family | s | max RSS MB |
|---|---|---|
| cst | 3.33 | **1,087** |
| call | 0.17 | 110 |
| type | 0.07 | 82 |
| df | 0.06 | 81 |
| all four | 3.24 | 1,166 |

The CST family is 100% of the time and 93% of the memory. It is the ast-grep /
tree-sitter plane (`src/lang/astgrep.rs`), shared by every language arm, reached
from `src/lang/ts.rs:3018`. The oxc arm this lane owns costs 0.07 s and 82 MB on
the same input. Finding F4; the owner is not this arm.

### The bytes/ms low tail is a measurement artifact

The 5th percentile of bytes/ms is 0.6, and the files under it have a median size
of **48 bytes** (5,834 `.ts`, 266 `.js`, 107 `.mjs`, 80 `.mts`). Re-timed
serially, the 20 worst of them run in **4-6 ms**, not the 271-315 ms the battery
recorded. An empty file costs 2 ms, measured five times. The tail is 12-way
contention plus cold page cache over a 1.5 GB working set, not a slow construct.
Not filed as a finding.

---

## 9. Findings

| # | lang | class | path:line | repro | observed | expected |
|---|---|---|---|---|---|---|
| F1 | ts | missing_fact | `src/lang/ts.rs:119` (was `for stmt in &program.body`) | `extract --family type,call tests/fixtures/ts/corpus_1.ts` | zero facts for every declaration inside `namespace` / `declare module` / `declare global` | one entity per nested declaration. **FIXED, see section 10** |
| F2 | ts | wrong_fact | `src/lang/ts.rs:1776` (`callee_name`, `E::StaticMemberExpression`) | `extract --resolve tests/fixtures/ts/corpus_2.ts tests/fixtures/ts/corpus_2_logger.ts` | `console.log(...)` in `corpus_2.ts` emits a `resolved_edge` to the free `log` in `corpus_2_logger.ts`, which it never imports | no edge; the receiver `console` is not that file |
| F3 | ts | missing_fact | `src/lang/ts.rs:1642` (`CallWalker` has no `visit_decorator`) | `extract --family call tests/fixtures/ts/corpus_3.ts \| grep -c site` -> `0` | `@Injectable` and `@Log` produce no `site` record | two `site` records, callee `Injectable` and `Log` |
| F4 | ts | rss | `src/lang/astgrep.rs`, reached from `src/lang/ts.rs:3018` | `/usr/bin/time -l extract --family cst monaco-editor/dev/vs/assets/ts.worker-DrA3GP0m.js` | 1,087 MB resident for a 12.7 MB input, 3.33 s | a bound that does not scale 88x with input; 8-way parallel dispatch over such files needs ~9 GB |
| F5 | ts | parse_error | `src/lang/ts.rs:92` and the CLI stream | `extract <gzip blob named .js>` -> rc=0, empty stdout, empty stderr | a caller cannot tell "no facts" from "could not parse this input" | a `parse_error` row, or the CST `ERROR` node the text path already emits |
| F6 | ts | unresolved | `--schema:26` vs `--resolve` output | `extract --resolve <11 files> \| grep -c unresolved` -> `0` | `--resolve` emits only `resolved_edge`; the `unresolved` record (`dynamic-import`, `computed-member-call`, `spread-call-args`) is dropped | either the row, or one line of `--help` saying `--resolve` drops it |
| F7 | ts | missing_fact | `scip_index` row | `cd ~/projects/instant && extract --family scip .` | `documents=197` against 18,339 source files; nothing says which files fell outside the root tsconfig | a row naming the uncovered set, the way `scip_skip` names an uncovered root |
| F8 | ts | wrong_fact | `--schema` `record=scip_fn_edge` | boop-adapters: 184 distinct scip pairs, 47 resolve pairs, 0 shared | 90 of 209 `scip_fn_edge` rows have a field/property callee (`X#X:X.X`); the name says function edge | either a narrower relation or a schema line stating that property references are included |
| F9 | ts | crash | `--resolve` / `--deps` argument handling | `extract --resolve ~/projects/hafley-rxjs/packages/rxjs-ext/src` -> rc=1 | `Error: Read("src", Custom { kind: Other, error: "read /abs/path" })`, a Debug dump of the error type | a message naming the cause: a directory is not a `--resolve` path |
| F10 | ts | missing_fact | `src/lang/ts.rs:1399` (`--resolve` with one path) | `extract --resolve one.tsx` -> rc=0, empty stdout | silence, though `--help` states "Needs two or more paths" | a non-zero exit with that sentence, the way a missing `--project-root` already does |

### Known-by-design, recorded not filed

| behaviour | where it is stated |
|---|---|
| `export const port = 8080` mints no type entity; only a string-bearing const does | `src/lang/ts.rs:2869-2871` and `:501-504`, a v5 port |
| `export { a }` without `from` produces no specifier row | `src/lang/ts.rs:1241-1242` |
| A function overload set yields one type entity per signature and one call def (the implementation) | consistent with `fn_call_def` skipping bodiless functions, `src/lang/ts.rs:1436-1448` |
| `callee_path` is null in per-file mode | `--schema:71`, "filled by resolution" |
| A bare name ambiguous across supplied files does not resolve under `diet_scip` | `--help`, FAST MODE |

### Constructs checked and correct

`export * from`, `export * as ns from`, `export { x as y } from`, default
export of a function and of a class, `import type`, `import * as`, side-effect
import, `import()`, `require()`, string enums (member values land as `const`
rows), `const enum`, `declare enum`, `satisfies`, `as`, `!`, JSX components as
callees (`<Widget />` gives `callee: "Widget"` and resolves cross-file),
`.d.ts`/`.d.mts`/`.d.cts` declaration files, barrel re-export chains,
`--deps` reason slugs.

---

## 10. Landed fix: module bodies

Finding F1. Six family loops iterated `program.body` directly, so a
`TSModuleDeclaration` body reached none of them: type entities, type edge
candidates, call defs, lambda defs, module specifiers, dataflow, and const
values all stopped at the `namespace` keyword. `grep TSModuleDeclaration
src/lang/ts.rs` returned nothing before this change.

One helper, `with_module_bodies`, flattens `program.body` plus every nested
`namespace` / `declare module` / `declare global` block in source order; the six
loops iterate it. `namespace A.B {}` nests one declaration per dotted segment, so
`module_block` walks to the innermost block. `declare global` is oxc's separate
`TSGlobalDeclaration` node and gets its own accessor. No visitor double-counts:
`CallWalker` and `ConstWalker` raise depth only inside function and arrow
bodies, so a declaration at module-block level stays depth 0 and is minted once.

Fail-first receipt: `tests/42_ts_module_bodies.rs` written before the fix, red
on all three cases (`extract --family type,call tests/fixtures/ts/corpus_1.ts`
printed nothing at all).

| receipt | value |
|---|---|
| `cargo test --features cli --test 42_ts_module_bodies` | 3 passed, 0 failed |
| `cargo test --features cli`, whole crate, 70 binaries | **356 passed, 0 failed, 2 ignored** |
| corpus files declaring a `namespace` or `declare module` | 1,705 |
| type+call rows over those files, before | 3,872,283 |
| type+call rows over those files, after | 3,912,845 |
| **rows gained** | **+40,562** |
| files gaining rows | 827 |
| files losing rows, 1,500-file random sample of the whole run set | **0** |

`.d.ts` is 38,577 of the 125,844 run-set files, and `namespace` is how a
declaration file groups its exports, so this gap sat under the largest typed
slice of the corpus.

---

## 11. What stays untested and why

| left out | why |
|---|---|
| The 1,978,835 duplicate `node_modules` copies | Identical package-relative paths from pnpm store copies and per-worktree installs. The deduplicated 97,883 cover every distinct package file; running the rest measures the filesystem, not the extractor. |
| `--resolve` across directory boundaries | Every step-3 run took files at depth 1 of one directory. A whole-package run pulls in `dist/` twins of the same sources and doubles every definition, which measures the file selection rather than the resolver. |
| `--family scip` on a root without `tsconfig.json` | All three roots have one. The `not_installed` and `timed_out` `scip_skip` branches never fired, so their text is unverified here. |
| `--scip-override` (`--resolve` with `--project-root` plus an index) | Needs a decision about which index is authoritative per root; step 5 shows `scip_fn_edge` and `resolved_edge` are different relations (F8), so an override comparison would be measuring that mismatch, not the override. |
| Fixes for F2, F3, F5 through F10 | F2 and F3 are inside this arm but change edge semantics corpus-wide: dropping a member call whose receiver is not a local binding would also drop legitimate method edges, and adding decorator call sites changes every framework-heavy file's call graph. Both need the coordinator's call, so this lane files them with repro fixtures (`corpus_2.ts`, `corpus_2_logger.ts`, `corpus_3.ts`) and lands neither. F4 is `astgrep.rs`, F5 through F10 are the CLI and scip planes; none is this arm. |
| Throughput per construct | The bytes/ms low tail is battery contention, shown in section 8. Isolating a slow construct needs a single-process benchmark harness, which is a separate build. |
