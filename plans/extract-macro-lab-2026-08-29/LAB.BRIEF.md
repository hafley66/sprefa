# Brief: lab the rust macro-expansion options for sprefa-extract (lane `lab-extract-rust-macros`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law,
forbidden list). This is a LAB, not a fix: you MEASURE each option on the
same corpus and the same fixture, and write the comparison. You do NOT edit
`v6/sprefa-extract/src/**`. Lab code lives under
`v6/sprefa-extract/labs/macro_expand/` (a standalone cargo crate you create;
add it to nothing else). Labs die on landing: the plan doc records what was
learned and the lab crate is deleted in your last commit, with the last-copy
commit hash written into the plan doc.

## First action
```
git merge --ff-only 62878cf0d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as lab-extract-rust-macros sprefa-coordinator "<one line>"`.

## The gap (rust.REPORT.md kink 2, plans/extract-crawl-2026-08-29/)
17,184 macro invocations in `~/projects/rust-analyzer/crates/**/*.rs` (941
src files); calls inside a macro body mint no site. Corpus counts you must
re-measure first (grep, cite the commands): `macro_rules!` definitions,
invocations of local `macro_rules` vs std builtins (`format!`, `vec!`,
`assert!`...) vs proc-macro attributes vs `#[derive(...)]`.

## Options to lab, each in its own module of the lab crate
1. `ra_ap_mbe` + `ra_ap_syntax` + `ra_ap_tt` (0.0.349 on crates.io):
   parse each `macro_rules!` in the corpus, expand each invocation's token
   tree in-process, map expanded tokens back to invocation spans through the
   span map, then run the EXISTING extract call walker over the expanded
   syntax (feed it as a synthetic file, record how spans map).
2. `ra_ap_hir_expand` (full expander incl. builtins and proc-macro server):
   measure link cost (`cargo build --release` wall and binary size delta),
   startup cost per crate, and what it takes to point it at a workspace.
3. `cargo expand` / `rustc -Zunpretty=expanded` (nightly): one run per RA
   crate; wall per crate, whether every crate expands, size of expanded text
   vs source, and how you would map expanded spans back (diff-based; measure
   how often the mapping is ambiguous).
4. `--family scip` (rust-analyzer index): re-run on the RA root with
   `--scip-timeout 1500`; the earlier build panicked with
   `No generics for EnumVariantId(...)`; find the RA version on PATH, try
   the newest release binary in scratch, record whether the panic persists,
   and count `scip_fn_edge` rows whose occurrence sits inside a macro
   invocation span.
5. `syn`: state in one row that it parses and does not expand (no work).

## The fixture, same for every option
`labs/macro_expand/fixtures/`: 8 files covering local `macro_rules!` with a
call in the body, a `macro_rules` defined in another file, nested
invocations, `format!`/`vec!`/`assert!`, `#[derive(Debug, Clone)]`, an
attribute proc macro (use `serde::Serialize` via a tiny Cargo dep), a
`macro_rules` that mints a `fn` def, and `include!`. For each option
record the table: fixture | sites found inside expansions | defs minted by
expansion | spans mapped to source (yes/no/partial) | wall ms.

## Corpus measurement
For the options that run (1, 3, 4): over RA's 941 src files, count
invocations expanded, sites gained, wall, RSS, failures. Under `timeout 10`
per invocation; batteries in background with a log.

## Deliverables (commit, push, PR)
- `plans/extract-macro-lab-2026-08-29/PLAN.md`: TOC; corpus counts; per
  option: what it is, what it needs (deps, toolchain, subprocess), the
  fixture table, the corpus table, span-mapping story, the exact seam in
  `src/lang/rust.rs` it would plug into (cite fn and line); a final
  candidate-by-candidate comparison table and a recommendation that names
  a tier 1 and tier 2 with the numbers that justify them.
- `plans/extract-macro-lab-2026-08-29/PLAN.visual.human.unga.md`: plain
  words, a mermaid flowchart of the expansion pipeline per option, the
  comparison table, zero citations.
- Lab crate deleted in the last commit; hash recorded in PLAN.md.
- `gh pr create --base main`; hail
  `boop beep --no-wait --as lab-extract-rust-macros sprefa-coordinator "macro lab: PR #N, tier1=<opt> tier2=<opt>, sites gained <n>"`.

## Forbidden
`v6/sprefa-extract/src/**`, every other crate, `CLAUDE.md`, the corpus.
No subagents. No em dashes. Never write "the language does not support";
cite the throw site. No design decision in prose without its measured row.
