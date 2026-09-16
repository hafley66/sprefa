# refactor-parser-split (pass 1 of 2; the coordinator reads every diff hunk)

You are lane `refactor-parser-split`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `refactor/parser-split`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Goal
Split `v6/prolog/compile/parse_dl_dcg.pl` (1776 lines) into a folder of parts using `:- include`, exactly the shape `v6/prolog/0_generic_expand.pl` already has (lines 50-72: module head stays in the file, then one `:- include('0_generic_expand/<part>.pl')` per part, parts in a folder named after the file). Behavior byte-identical. This is a MOVE, never a rewrite: no clause text changes, no reordering except the one relocation named below, no comment edits.

## The cut (follow it exactly)
`TASKS/parser-split/parse_dl_dcg.cuts.json` lists the 11 parts in order with an anchor predicate each; `TASKS/parser-split/parse_dl_dcg.md` is the plan lane's report with line ranges. Folder: `v6/prolog/compile/parse_dl_dcg/`. Part file names as in the json (`0_cst_shapes.pl` ... `10_expr.pl`). A part begins at the first clause of its anchor predicate's family and runs to the line before the next part's anchor family; a predicate never straddles two parts. If a range in the report disagrees with the anchor rule, the anchor rule wins; say which ranges you adjusted.

## What stays in `parse_dl_dcg.pl`
Lines 1-41 as they are (module decl + exports, `set_prolog_flag(back_quotes, codes)`, the `use_module` lines, any other directives), then the 11 `:- include('parse_dl_dcg/<part>.pl').` lines, one per line, in cut order, nothing else. Check whether `back_quotes` and any `op/3` directives must precede the includes for the parts to read: they must, keep them above.

## Discontiguous rule (Chris's word: none across parts)
- `lex_token/2` has 3 clauses: lines 475-476 and 1164. Move the clause at 1164 to directly after line 476 (same text). Then delete `:- discontiguous lex_token/2.` (line 42) ONLY if `swipl` loads the module with zero discontiguous warnings afterwards; else keep it and say why.
- `type_base/3` clauses sit at 853, 863-864, 869, 880, all inside one part; leave them and their directive (line 43) alone.
- Any other predicate that would straddle parts: STOP and hail its name and lines; do not invent a relocation.

## Receipts (all required, run each in the background with a timeout, never foreground-wait over 10 s)
1. Listing snapshot, before and after, byte-identical except for the relocated `lex_token/2` clause order:
   `cd v6/prolog/compile && swipl -g main -t halt /Users/chrishafley/projects/sprefa/TASKS/parser-split/modsnap.pl -- parse_dl_dcg.pl parse_dl_dcg > /tmp/parser.before.listing` on the base sha (do this FIRST, before any edit), then the same after the split into `/tmp/parser.after.listing`, then `diff`. Paste the diff (expected: only the lex_token clause moved, or empty) into the PR body.
2. `swipl -q -l v6/prolog/compile/parse_dl_dcg.pl -g halt` prints zero warnings. Paste the output.
3. `cd v6/prolog/conformance && swipl -g go -t halt go.pl` -> 445 PASS / 0 FAIL.
4. `cd v6 && just plunit` -> 1115 passed / 0 failed.
5. `bash v6/sprefa-engine-rs/grade.sh` -> graded=445 byte-clean=341.
6. `swipl -g go -t halt v6/prolog/ARCH.pl` -> 7/0.
7. `wc -l v6/prolog/compile/parse_dl_dcg.pl v6/prolog/compile/parse_dl_dcg/*.pl` in the PR body; sum of parts + head == 1776 + 11 include lines (state the arithmetic).

## Deliverables
One commit, message `refactor(parser): split parse_dl_dcg.pl into eleven included parts, byte-identical`, body carrying receipt 7's arithmetic. Push. PR to main titled the same, body = all seven receipts. Then hail: `boop beep hail sprefa-coordinator --from refactor-parser-split --body "PR #<n>: listing diff <empty|lex_token only>, conformance 445/0, plunit 1115/0, grade 445/341, ARCH 7/0"`.

## Yield results over time
Hail after receipt 1's BEFORE snapshot exists, after the split loads warning-free (receipt 2), and at done.

## You own
`v6/prolog/compile/parse_dl_dcg.pl`, `v6/prolog/compile/parse_dl_dcg/**` (new). Forbidden: every other file. If any receipt is red, STOP and hail the exact failing output; do not fix anything outside the two paths you own.

## Style laws
No em dashes. No new comments. No banned words (provenance, substrate, load-bearing, regime, ground truth, refusal, support, honest). Commit message imperative.
