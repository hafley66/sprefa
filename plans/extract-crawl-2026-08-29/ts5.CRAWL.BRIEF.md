# Brief: entrypoint crawl of TypeScript with sprefa-extract (lane `crawl-extract-typescript-5`)

Read `/Users/chrishafley/projects/sprefa/plans/extract-corpus-2026-08-28/COMMON.md`
(style laws, the 10-second law, forbidden list). This lane is ANALYSIS:
you run the binary over a whole real project, walk its call graph from the
program's entrypoints, and write down every kink. You do NOT edit
`v6/sprefa-extract/src/**` (two fix lanes own it right now); a defect
becomes a finding row plus a minimal repro fixture under
`v6/sprefa-extract/tests/fixtures/ts5_findings/` with the expected fact
in its header comment.

## First action
```
git merge --ff-only 483c055a3
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as crawl-extract-typescript-5 sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in your worktree.
Corpus: `/Users/chrishafley/projects/TypeScript-5.9` (branch release-5.9, the LAST TypeScript-in-TypeScript compiler; `src/` holds 701 `.ts`) (shallow clone, read-only;
never modify it, never run its build except where step 4 says).
Scratch: `/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/crawl-ts5`.

## Steps, in order, every extract call under `timeout 10`
1. **Per-file battery** over every source file of the language (exclude
   `node_modules`, `target`, vendored dirs; say which). TSV
   `path rc ms bytes lines`; report rc!=0, rc=124, size_skip rows,
   slowest 10, largest 10.
2. **Whole-project --resolve** over all files (split into packages/crates
   if one call exceeds 10 s; record which). Counts: resolved_edge,
   resolved_type_edge, distinct callers, distinct callees. Per-file
   unresolved sites: run `extract --family call FILE` per file, count
   `site` rows, subtract the file's resolved_edge count = unresolved
   sites; rank files by unresolved ratio, open the top 10, classify the
   cause (external crate/package, method on unknown receiver, macro,
   generic, re-export, closure, trait dispatch, interface dispatch).
3. **Entrypoint crawl.** Entrypoints: the compiler and tsserver mains: `src/tsc/tsc.ts`, `src/tsserver/server.ts`, `src/typescript/typescript.ts` exports, and every `export function` in `src/compiler/program.ts` `createProgram`; `src/testRunner/**` counted as a second root set. Exclude `tests/baselines/**` and `tests/cases/**` from every step (they are not the program); include them ONLY in step 1 as a separate row (they are a huge parser stress corpus: report rc!=0/timeouts there separately). `scip-typescript` is at `~/.nvm/versions/node/v24.15.0/bin/scip-typescript`.
   From each entrypoint, BFS over resolved_edge (caller -> callee) using
   the JSONL from step 2 (a small python or jq script in scratch; commit it
   under `plans/extract-crawl-2026-08-29/ts5.crawl.py`). Record: reachable
   defs at each depth, total reachable vs total defs (call-plane `node`
   rows with kind function/method), the depth histogram, the 20 largest
   unreachable defs by span size (are they dead, dispatched dynamically, or
   missed edges? open 10 and say which, with file:line), and the 20 nodes
   with the highest out-degree.
4. **scip comparison** where the toolchain exists (ts: see
   `extract --help` EXACT MODE; use `--scip-build` in a COPY of the repo
   root in scratch so the corpus stays clean; cap the build with the
   documented budget flag; record scip_skip rows verbatim if it fails).
   Re-run the crawl over `scip_fn_edge` and put the two reachability
   numbers side by side. Sample 30 edges present in one and absent in the
   other; classify each.
5. **Kinks.** Every classified miss from steps 2-4 with a count in the
   corpus, its cause, the `src/lang/ts*` site you believe owns it
   (cite the fn), and a repro fixture. Table
   `class | count | example file:line | owner fn | fixture`.

## Deliverables (commit, push, PR)
- `plans/extract-crawl-2026-08-29/ts5.REPORT.md`: TOC, then one table
  per step, then the kinks table, then "untested and why".
- `plans/extract-crawl-2026-08-29/ts5.PLAN.visual.human.unga.md`: the same
  story for a reader with zero context: a mermaid flowchart of the crawl
  (entrypoints -> depth bands -> dead ends), the reachability numbers, the
  top 5 kinks in plain words, no citations.
- `ts5.runs.tsv`, `ts5.crawl.py`, fixtures as above.
- `gh pr create --base main`, then
  `boop beep --no-wait --as crawl-extract-typescript-5 sprefa-coordinator "ts crawl: PR #N, files=F, resolved=R, reachable=X/Y, kinks=K"`.

## Forbidden
`v6/sprefa-extract/src/**`, every other language's dirs, `v6/prolog/**`,
`CLAUDE.md`, the corpus repo. No subagents. No `--no-verify`. No em
dashes, none of the banned words in COMMON.md. No foreground wait over 10 s:
batteries run with `nohup ... &` and a log you poll.

## Context from the first ts crawl (PR #538, `plans/extract-crawl-2026-08-29/ts.REPORT.md`)
That lane ran on `~/projects/TypeScript` main, which is now the Go port and
holds only the npm shim (2,517 defs). Read its kinks table first and do NOT
re-derive those 8 kinks; count them again on this corpus and add what is new.
The brief's entrypoints exist on this branch: `src/tsc/tsc.ts`,
`src/tsserver/server.ts`, `src/typescript/typescript.ts`,
`src/compiler/program.ts`. Exclude `tests/**` from steps 2-5; run
`tests/cases/**` in step 1 as its own stress row (rc!=0, timeouts, size_skip).
The compiler is `namespace ts { }`-free since 5.0 (ES modules), so the
namespace-body fix from PR #528 is not the shape here; `export * from`
barrels (`src/compiler/_namespaces/ts.ts`) are, and `--resolve` through a
barrel is the first thing to measure.
