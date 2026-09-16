# feature/golden-nesting-printer: minted-artifacts section + formatting decree in print_dl

## Two user decrees 2026-08-11, both land here
1. FORMATTING LAW for dl6 source: single-line is legal ONLY for rel decls
   and simple facts of at most 2 terms. Everything else breaks across
   lines: rule head + `<-`/`<+` on the first line, then EVERY body goal on
   its own line, 2-space indented (golden-flex's existing multi-line rules
   are the model). Applies to print_dl OUTPUT and to the new fixture text
   you write.
2. Golden-flex gains a teaching section: "MINTED ARTIFACTS: what a nested
   decl becomes".

## Part A: print_dl
v6/prolog/print_dl.pl prints programs; make its output follow the
formatting law. BEFORE changing anything, read how the two gates compare:
- compile/scripts/roundtrip.sh: asserts reparse(print(P)) is a VARIANT of
  P (fail(not_variant)); formatting is free as long as the term survives.
- compile/scripts/text_door_receipt.sh/.pl: check what byte_identical
  compares (compiled artifacts vs source). If ANY gate pins printed source
  bytes, regenerate those pinned artifacts as part of this arc and say so
  in the commit message.
Decision row in v6/prolog/conformance/rulings.pl: dl6_formatting,
single_line_only_decls_and_2term_facts; user 2026-08-11.

## Part B: the golden-flex section
Add "MINTED ARTIFACTS: what a nested decl becomes" after the GENERICS
section. Content, each with arrivals + a retraction so the coverage gate
grades it, comments only stating what the code cannot show:
1. The grade payload enum already at :275: name its three artifacts in the
   section comment (grade_ripe/2, grade_bruised/2 variant rels sharing the
   id space, grade_tag/2 the which-one view) and READ one variant rel and
   the tag view in rules.
2. A rel-payload variant (the #149 shape): a variant whose field is typed
   as another rel; show the ref arriving and being read back.
3. pre/2 one-arm fold BESIDE the existing two-arm pick_count (:264), same
   final rows, so the file shows old and new spellings of the same fold.
4. An option(T) column and what it mints (companion/enum), read through
   coalesce or its natural read path.
5. One list(rel_name) column (the #151/#159 spelling) read back through
   spread/join.
All new text follows the formatting law (part A) so the printer and the
teaching file agree.

## Part C: banner rename
golden-flex line 24 banner says "NAMED REFUSALS"; the word is banned in
prose (user 2026-08-09). Rename the banner to "NAMED UNSUPPORTED
CONSTRUCTS (part of the flex; the coverage gate asserts each one)". Check
whether the coverage gate greps the banner text before renaming; adjust
the gate's pattern if so, never the other way.

## Files you own
- v6/prolog/print_dl.pl
- v6/dl/fixtures/golden-flex.dl6
- v6/prolog/conformance/rulings.pl (one row)
- pinned artifacts your reformat regenerates + coverage-gate pattern if
  part C needs it
Do NOT touch 0_generic_expand.pl (another lane owns it right now),
parse_dl.pl, lower.pl.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate (all green, no exceptions)
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just golden-flex
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```

## Rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> FAILURE-REPORT-GOLDEN-PRINTER.md, exact command + output,
  exit NONZERO. rc=0 with a dirty tree or red gates is a defect.
- NEVER --no-verify. Up to 3 commits, prefix `prolog:`. Comment budget:
  max 2 consecutive comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal
(except the existing literal identifiers part C is renaming AWAY from).
dl variable names descriptive, never single-letter.
