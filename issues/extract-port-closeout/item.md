---
created: 2026-08-16
updated: 2026-09-08
type: epic
status: open
priority: high
labels:
- pkg:extract
---

## Description

Goal, in the user's words: finish the v5-to-v6 extract port done-done so nobody
opens dl5 again.

## Census: every v5-vs-v6 extract gap, with a receipt

| # | gap | v5 receipt | v6 receipt | issue | verdict |
|---|---|---|---|---|---|
| 1 | `.dl6` phase-2 arms exist and are never dispatched | n/a (v6-native) | `lang/dl6/_0_source.rs:425,449` vs `project.rs:449-478` | @extract-dl6-resolve-unwired | dispatch |
| 2 | SCIP indexer roster short 3 langs | `src/scip_setup.rs:66-72,80-99` | `scip_ensure.rs:65-88`, gap named at `:35-40` | @extract-scip-indexer-roster | dispatch |
| 3 | docs facet unported, all langs | `typegraph/mod.rs:161-177`, `family/mod.rs:507` | zero `DocFact` in `types.rs` | @extract-docs-facet-shape, @extract-docs-facet-lang-arms | dispatch |
| 4 | df aux fields/lits | `family/mod.rs:455-491` | `types.rs:540-543` | @extract-df-aux-fields-lits | dispatch |
| 5 | df aux loops/nests/allocates | `family/mod.rs:455-491` | `types.rs:540-543` | @extract-df-aux-loops-nests | dispatch |
| 6 | kotlin type plane (candidates + `Resolve<TypeF>`) | `typegraph/kotlin.rs` | `lang/kotlin.rs:27-31`, `project.rs:463-478` | @extract-kotlin-type-plane | dispatch |
| 7 | no python arm at all | `typegraph/python.rs`, `modgraph/python.rs` | `lang/mod.rs:40-51` | @extract-python-arm | dispatch |
| 8 | module plane is TS-only | `family/mod.rs:397-408`, `modgraph/*` | `deps.rs:1-2`, specifiers only in `lang/ts.rs` | @extract-module-plane-non-ts | dispatch (shape depends on #14) |
| 9 | runtime-computed edge markers | `family/mod.rs:552-570` | none | @extract-unresolved-markers | dispatch |
| 10 | markdown doc_node / doc_ref | `family/mod.rs:493-501` | `lang/markdown/_0_source.rs:1-7` (cst only) | @extract-doc-node-markdown | dispatch |
| 11 | content-keyed cache + parallel dispatch | n/a | `types.rs:50-53,1838-1839`, `dispatch.rs:1-16` | @extract-blob-cache-parallel | dispatch |
| 12 | `Resolve<F>` default body is `todo!()` | n/a | `types.rs:1105-1109` | @extract-resolve-todo-default | needs-chris |
| 13 | `DfEdgeKind::Flow` union commented out | n/a | `types.rs:605-612` | @extract-df-flow-union | needs-chris |
| 14 | `ModuleF` collapsed, flagged for human review | `family/mod.rs:397-408` | `types.rs:629-645` | @extract-modulef-collapse | needs-chris |
| 15 | `scip_occurrence` / `scip_binding` outside the v5-vocab set | `src/rels/scip.rs:41-50,77-88` | `schema.rs:160-173` | @extract-scip-vocab-occurrence-binding | needs-chris (occurrence = doc close) |
| 16 | rust type graph as a drawn board | n/a | `types.rs:226-253` `TypeEdgeKind` | @rust-typegraph-d2 | dispatch |

## Closed with no code owed

| v5 thing | why v6 already answers it | receipt |
|---|---|---|
| `comment_node` (`family/mod.rs:532`) | the CstF plane emits `comment` nodes for every grammar; probed on a `.ts` file, 2 comment nodes for 2 comments | `src/lang/astgrep.rs:167`, probe under `extract probe.ts` |
| `template_parts` (`family/mod.rs:549`) | CstF emits `template_string` / `template_substitution` / `string_fragment` as nodes with child edges; v5's `idx`/`kind` row is a join over those children | same probe |
| `call_kind` (`family/mod.rs:448`) | engine-side, not extract: computed over `call_site` in the engine | `src/engine/family/call_kind.rs:2-6` |
| `string` / `ref` / `node` / `child` (`family/mod.rs:585,599`) | engine meta-table views and the CstF plane respectively | `family/mod.rs:578-599` |
| "sprefa-extract has no markdown extractor" (CLAUDE.md open row) | STALE. `MarkdownSource` is in the roster and `source_for(".md")` returns it | `src/lang/mod.rs:46` |

## Baseline, measured 2026-08-16 at 988e2b5

```
cd v6/sprefa-extract && cargo build --all-targets --features cli   # rc=0
cd v6/sprefa-extract && cargo test --features cli                  # rc=0, all legs pass
```

## Comments

### 2026-08-16T17:29:18Z · @extract-closeout-driver

df_args (44 oracle rows) and df_param_pos (42) are in the captured oracle AND emitted by v6, and neither is in golden_parity's PORTED list, so 86 oracle rows sit unasserted. Adding both to PORTED is a one-line ratchet worth trying before any new work; if it goes red, that is a real port gap nobody has measured. Note PR #304 made ported_facets_match_v5 filter BOTH sides by PORTED, so the flip is now safe to try.

### 2026-09-08T17:37:06Z · @fable

Analysis doc: research/2026-09-08-fact-sources-beyond-scip.md (glossary, candidate-by-candidate, matrix, ordered recommendation with 8 issue slugs to file, measurements incl. codeql 3.61 s vs scip 1.25 s cold on the same package).


## Agent Runs

### 2026-09-08T17:24:46Z · @fable

extract as a code-review instrument: field test on `@hafley66/vitest-playwright` (2026-09-08)

Setup: three reviews of the same 13-file / 922-line TypeScript package (vitest 4 plugin folding playwright/test into node-realm vitest), same afternoon, same snapshot digest. One opus lane read source by eye with node_modules cross-checks; one opus lane was forbidden from viewing any source and allowed only `extract` (+ jq/grep/awk); the coordinator ran adversarial and perf tests. Both lane reports live next to the package as `REVIEW-2026-09-08-opus-eyes.md` and `REVIEW-2026-09-08-opus-extract.md` (the latter attached here).

#### Timing on this corpus (17 ts files, 1053 lines, typescript 7.0.2 = Go-native tsc, scip-typescript 0.4.0, node 24)

| run | wall | rows | note |
|---|---|---|---|
| `extract --family scip .` cold (index build + stream) | 1.25 s | 2965 | `scip_index reused:false, documents:19`; index 661 KB |
| same, warm (index reused) | 0.00 s | 2965 | `reused:true`; reuse probe hits `.dl/.state/index.scip` |
| `extract --family diet_scip src/*.ts tests/*.ts` | 0.12 s | 694 | no compiler |
| `extract FILE` x17 serial, all four families | 0.09 s | 40645 | phase-1 facts |
| `--scip-facts` raw stream (warm) | ~0.1 s | 7885 | `scip_documentation` 1215 rows = the inferred-type oracle |

The Go compiler makes the cold scip path cheap enough to run per review; the agent's whole session was 128 tool calls / 23 min, dominated by jq joins, not by extract.

#### Findings side by side

| | eyes | extract-only | anti tests |
|---|---|---|---|
| rows | 34 (P0 3, P1 10, P2 21) | 18 (P0 3, P1 5, P2 10) | 6 breaks |
| unique to this reviewer | 24 | 7 | 4 |
| shared with another reviewer | 10 | 11 | 2 |
| fixed the same day | 30 | 15 | 6 |

What only extract found (all real, all fixed):
- duplicated cleanup: `pageH?.release()` / `ctxH?.release()` / `connection.unsubscribe()` present in two nested `finally` clauses 300 bytes apart, both executing on the normal path. `--ast-pattern a='$H?.release()' --ast-capture a=H` gave 4 rows with receivers; `--family cst` gave the `try_statement`/`finally_clause` span containment. Introduced by the coordinator's own refactor 20 minutes before the snapshot; eyes review had the same snapshot and did not see it.
- acquire-before-try in two fixtures (`4_test.ts`): `site` rows for `acquire@offset` outside the `try_statement` span whose `finally` holds the `release`. Same shape in `globalSetup` (no try at all around `provide`).
- `addEventListener` with zero `removeEventListener` across all 17 files (one loop of `--ast-pattern` per file).
- inferred `any` at the package's headline symbols (`proxy(): any`, `rootOf(task: any)`), invisible in source, only in `scip_documentation` hover text.
- 41 rxjs v8 `DEPRECATED` compiler diagnostics for free from `scip_diagnostic`.
- 9 of 13 src files referenced by no test (`--scip-deps` file_edge set-subtract): the barrel and `exports` map were never exercised.

What only eyes found (sample): fixtures resolve lazily so `releaseLazyBrowser` never ran for plain-`it` files; `task.result.repeatCount` written inside `runTest` so the artifact dir is one phase stale; one `AbortController` per test across retries; `provide` serialization drops function-valued `LaunchOptions`; `waitForURL` missing the abort signal vs playwright's own matcher; message-shape divergences from `playwright/lib/matchers/expect.js`. Every one needed node_modules line citations, which extract has no family for (it indexes the corpus you hand it, and nobody handed it vitest or playwright).

What only the anti tests found: ALS store dropped through playwright event dispatch and route handlers; leaked timer from a finished test reading `$page`; `vi.useFakeTimers` freezing the rxjs poll and then the playwright client and teardown; 84 of 98 empty attempt dirs. Runtime semantics, out of reach of both static reviewers.

#### Command scorecard (from the extract-only lane)

| command | useful | why |
|---|---|---|
| `--file-fact FILE` (site, node:cst/call/df, df_field/lit/loop, specifier, sig) | yes | the workhorse; every span-containment join |
| `--family scip .` | yes | zero-reference set in 1.2 s cold |
| `--scip-facts --project-root . --scip-index` | yes | `scip_documentation` type oracle, `scip_diagnostic` |
| `--ast-pattern ID=P --ast-capture ID=N` | yes | highest signal per row; the only mode that returns source text |
| `--deps` / `--scip-deps` | yes | cycle check (none), test-to-src coverage |
| `--resolve --family call,type,flow` | partial | 920 `flow_edge` unused; `unresolved reason=inferred` on host methods is noise |
| `--family diet_scip` | no | subsumed by real scip on a corpus this small |
| `--family cfg` | no | no exception edges, no post-dominance; "does this finally run twice" had to be rebuilt from cst spans |
| `--package-deps`, `--witness`, `--bench` | no | not defect facts |

#### Gaps to file (each is one issue)

1. cfg family has no exception edges and no post-dominance. The best finding of the session (double release in nested finally) was reconstructed by hand from `try_statement`/`finally_clause` spans. A `cfg_edge kind=finally` or a `post_dominates` relation would make it a one-line query.
2. No "acquire/release pairing" query primitive. Three findings were the same shape: call X at offset a, matching call Y inside a `finally` whose `try` span does not contain a. A `df_pair` or dl6 rule over `site` + `cst` spans would name it directly.
3. `--scip-facts` refuses to run outside a git worktree; a scratch snapshot needed `git init`. `--deps`/`--scip-deps` require `--project-root` even with explicit paths; `--scip-facts` requires `PATH...` as well. Three flag-shape papercuts the agent hit before producing its first fact.
4. Under `--resolve` the phase-1 records vanish, so per-file `site`/`specifier` facts had to be collected file by file in a shell loop.
5. Dependency corpora. Eyes won 24 rows by reading `node_modules/@vitest/runner` and `playwright-core`. extract could index them (they are TS/JS) but nothing points it there; a `--corpus node_modules/<pkg>` or a resolved-import follow into the dependency's dist would close most of that gap.
6. The default `--scip-cache` writes `.dl/.state/index.scip` and `.dl/.gitignore` into the reviewed package; the lane worked around it, and the coordinator had to `rm -rf .dl` afterwards. A review run wants `--scip-cache` under the scratch dir by default when the root is a git worktree with a clean status.

Verdict: extract found real defects in a codebase two other reviewers had already covered, at 1.25 s cold, and its unique wins were exactly the structural ones (span containment, inferred types, zero-reference sets) that eyes skim past. It cannot replace the node_modules reads or the runtime probes. The right shape for the next session is all three at once, with extract's output feeding the eyes lane its file:line list.

### 2026-09-08T17:48:07Z · @fable

CodeQL swing on the same package: stock suite 87 rules / 0 findings / 11 s; five custom QL queries (research/2026-09-08-codeql-review-queries/, fixture included) found all four pre-fix shapes on the fixture (3 duplicated-cleanup rows, 1 acquire-before-try, 1 never-released, 1 require-in-esm) and one leftover duplicate in the refactor, since fixed; ExceptionalEdgeIntoFinally proves the JS CFG has exception edges from the AST alone. Cost: db 3.6 s, first query compile 51 s, warm 2.6 s. Doc §8. The .ql files + fixture are the acceptance test for @extract-cfg-exception-edges and @dl6-acquire-release-pairing.


## Decisions

### 2026-09-08T17:42:42Z · @fable

Field test 2026-09-08 follow-ups filed as children of this epic (label review-field-test), in priority order: @extract-cfg-exception-edges (high), @dl6-acquire-release-pairing (high), @extract-dependency-corpus, @extract-go-ssa-door, @extract-rust-mir-door, @extract-lsp-door, @extract-scip-cache-default (low), @extract-resolve-keep-phase1 (low). Rationale and measurements: research/2026-09-08-fact-sources-beyond-scip.md §5.

### 2026-09-08T18:05:39Z · @fable

Q&A on the CodeQL comparison and the decision to put review capabilities into extract proper (scope columns + smell family in Rust, no QL/dl6 dependency; fast tier never spawns a toolchain): @extract-review-capabilities-in-extract. License row in the analysis corrected to MIT OR Apache-2.0.

