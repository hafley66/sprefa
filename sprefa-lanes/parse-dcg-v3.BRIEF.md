# feature/parse-dcg-v3: DCG parser beside classic, attempt 2 (sol)

## What happened to attempt 1 (read this; it names the failure mode)
A prior lane on this task shipped NOTHING: its one commit is a failure
report saying its DCG entry secretly delegated to classic
`parse_dl:parse_dl_source/5` and was deleted before commit. Delegating to
the classic parser from inside the DCG path is the one banned move. A
partial REAL migration with accurate counts beats a complete fake.

## The toggle (user-ruled 2026-08-11; this exact shape, no threading)
One impure read at load, closed over via a prolog flag. The grammar never
sees the toggle; no predicate gains an extra argument for it:

```prolog
:- ( getenv('DL_PARSER', 'dcg') -> set_prolog_flag(dl_parser, dcg)
   ; set_prolog_flag(dl_parser, classic) ).

% the ONE dispatch seam, at the existing compile-entry call site:
parse_source(Text, Prog) :-
    ( current_prolog_flag(dl_parser, dcg)
    -> parse_dl_dcg_entry(Text, Prog)
    ;  classic_entry_as_today(Text, Prog) ).
```

Find where compile.pl (or its entry module) calls the parse entry today;
that call site becomes the dispatch. Smallest possible diff there.

## The work
1. New file v6/prolog/compile/parse_dl_dcg.pl: the grammar of
   parse_dl.pl rewritten as real DCGs (`-->`). parse_dl.pl already has 15
   real `-->` clauses; they move nearly verbatim. Hand-threaded clauses
   (`p(Args, S0, S) :- q(S0,S1), r(S1,S)`) become `p(Args) --> q, r` with
   term construction in `{}` escapes. Preserve clause order, cuts, and
   every thrown error term EXACTLY: identical program terms AND identical
   throws on the same input.
2. A section not yet migrated FAILS LOUDLY under DL_PARSER=dcg (missing
   nonterminal existence error is fine). NEVER falls through to classic.
3. Migrate section by section, COMMIT PER SLICE (up to 6 commits, prefix
   `prolog:`), each commit message naming the sections migrated and the
   parity numbers (next section).

## Your referee is already on main
`cd <worktree>/v6 && just parse-parity` runs
compile/scripts/parse_parity.pl over 411 corpus files. It auto-detects
parse_dl_dcg.pl and switches to classic-vs-dcg mode. Named migration
skips are its vocabulary for "not migrated yet": use them, keep them
loud, drive `diffs` to 0 and `skips` down slice by slice. Report the
`PARSE_PARITY mode=... total=411 parity=N skips=S diffs=D` line verbatim
in every commit message.

## Files you own
- v6/prolog/compile/parse_dl_dcg.pl (new)
- the ONE dispatch seam at the compile entry
- skip-list registration inside the parity harness's declared mechanism
  if it has one (read parse_parity.pl first; do not restructure it)
Do NOT touch parse_dl.pl, print_dl.pl, 0_generic_expand.pl,
golden-flex.dl6.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
Toggle OFF (default) battery stays green, proving the default path
untouched:
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just parse-parity
```
Then under the toggle: `DL_PARSER=dcg just conformance` — report its
count. Full green under the toggle is the finish line; partial migration
with accurate parity/skip counts is an acceptable final state.

## Rails
- Exiting rc=0 with a dirty tree, no commits, or red gates is a DEFECT.
  Blocked -> FAILURE-REPORT-DCG-V3.md, exact command + output, exit
  NONZERO. Work is independently re-verified after exit.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
Descriptive variable names, never single-letter.
