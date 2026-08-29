# Brief: entrypoint crawl of rust-analyzer with sprefa-extract (lane `crawl-extract-rust-analyzer`)

Read `/Users/chrishafley/projects/sprefa/plans/extract-corpus-2026-08-28/COMMON.md`
(style laws, the 10-second law, forbidden list). This lane is ANALYSIS:
you run the binary over a whole real project, walk its call graph from the
program's entrypoints, and write down every kink. You do NOT edit
`v6/sprefa-extract/src/**` (two fix lanes own it right now); a defect
becomes a finding row plus a minimal repro fixture under
`v6/sprefa-extract/tests/fixtures/rust_findings/` with the expected fact
in its header comment.

## First action
```
git merge --ff-only cec3d5c1d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as crawl-extract-rust-analyzer sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in your worktree.
Corpus: `/Users/chrishafley/projects/rust-analyzer` (shallow clone, read-only;
never modify it, never run its build except where step 4 says).
Scratch: `/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/crawl-rust`.

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
3. **Entrypoint crawl.** Entrypoints: every `fn main` in `crates/*/src/bin/*.rs` and `crates/rust-analyzer/src/main.rs`, plus the LSP request handlers registered in `crates/rust-analyzer/src/handlers/` (list them by name: each `pub(crate) fn handle_*`), plus `#[test]` fns counted separately as a second root set.
   From each entrypoint, BFS over resolved_edge (caller -> callee) using
   the JSONL from step 2 (a small python or jq script in scratch; commit it
   under `plans/extract-crawl-2026-08-29/rust.crawl.py`). Record: reachable
   defs at each depth, total reachable vs total defs (call-plane `node`
   rows with kind function/method), the depth histogram, the 20 largest
   unreachable defs by span size (are they dead, dispatched dynamically, or
   missed edges? open 10 and say which, with file:line), and the 20 nodes
   with the highest out-degree.
4. **scip comparison** where the toolchain exists (rust: see
   `extract --help` EXACT MODE; use `--scip-build` in a COPY of the repo
   root in scratch so the corpus stays clean; cap the build with the
   documented budget flag; record scip_skip rows verbatim if it fails).
   Re-run the crawl over `scip_fn_edge` and put the two reachability
   numbers side by side. Sample 30 edges present in one and absent in the
   other; classify each.
5. **Kinks.** Every classified miss from steps 2-4 with a count in the
   corpus, its cause, the `src/lang/rust*` site you believe owns it
   (cite the fn), and a repro fixture. Table
   `class | count | example file:line | owner fn | fixture`.

## Deliverables (commit, push, PR)
- `plans/extract-crawl-2026-08-29/rust.REPORT.md`: TOC, then one table
  per step, then the kinks table, then "untested and why".
- `plans/extract-crawl-2026-08-29/rust.PLAN.visual.human.unga.md`: the same
  story for a reader with zero context: a mermaid flowchart of the crawl
  (entrypoints -> depth bands -> dead ends), the reachability numbers, the
  top 5 kinks in plain words, no citations.
- `rust.runs.tsv`, `rust.crawl.py`, fixtures as above.
- `gh pr create --base main`, then
  `boop beep --no-wait --as crawl-extract-rust-analyzer sprefa-coordinator "rust crawl: PR #N, files=F, resolved=R, reachable=X/Y, kinks=K"`.

## Forbidden
`v6/sprefa-extract/src/**`, every other language's dirs, `v6/prolog/**`,
`CLAUDE.md`, the corpus repo. No subagents. No `--no-verify`. No em
dashes, none of the banned words in COMMON.md. No foreground wait over 10 s:
batteries run with `nohup ... &` and a log you poll.
