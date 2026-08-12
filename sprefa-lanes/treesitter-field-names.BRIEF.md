# BRIEF: tree-sitter round 4, land the three named fields + the emitter gaps

## Base
- Branch: `lab/tree-sitter-emit-v4`.
- Base sha: `26f3f25f` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- FIRST action after the worktree exists: `git merge --ff-only 26f3f25f`.
  Failure = STOP AND REPORT.

## Where round 3 left it
Ratio 1.6790 -> 0.3180. 1121 characters of hand overlay remain, in three kinds:

| kind | chars | route |
|---|---:|---|
| emitter gaps, no new parser fact needed | 541 | build it, part B |
| no DCG nonterminal names the node | 479 | more `cst_origin/2` rows, part C |
| editor deliberately wider than the parser | 101 | user decision, NOT yours |

Round 3's receipts: `plans/2026-08-11-tree-sitter-door.PLAN.md` (round-3
section), `v6/labs/tree-sitter-door/classification.tsv`, `measure.py`,
`REPORT3.md` for round 2's baseline.

## PART A: the three field names, DECIDED, just land them

The user answered "yes" on 2026-08-11 to these three. They are no longer open.

| slot | field names | rule it unblocks | chars |
|---|---|---|---:|
| the `sh` input and output column lists | `inputs`, `outputs` | `shell_declaration` | 171 |
| the `rel` column list and modifier list | `columns`, `modifiers` | `relation_declaration` | 166 |
| a type's trailing `?` | `optional` | `type` | 102 |

Add the `cst_shape/2` rows using exactly those names. Do NOT rename anything
already spelled in `grammar.js`; those 26 `field(...)` calls stay as they are.
Measure the ratio after this part alone.

## PART B: the emitter gaps, 541 chars, no new parser fact
Round 3 named three, each with its cause. Confirm each against the code before
building it.

1. **`source_file`**, 35 chars. The repetition IS present in `statements//3`
   (`v6/prolog/compile/parse_dl_dcg.pl:264-274`): `{S1 == []}` is the
   end-of-input guard and the recursive call is the tail. The emitter's
   detector misses it because it expects the recursive call in the `then`
   branch and reads a bare `{Goal}` alternative as the empty alternative.
   Widen the detector.
2. **`relation_declaration`, `shell_declaration`**, longest-common-prefix
   factoring across their alternatives.
3. **`column`, `type`**, resolving `call//3` through the specialization
   inventory the emitter already builds.

`column` is BLOCKED and stays blocked: `typed_col//2` defers through
`call(TypeP, Col, Type)` with `TypeP` unbound, and its two concrete bindings
`decl_b_column_type//3` and `host_col_type//3` differ ONLY in a cut. Merging
them passes parity while silently widening the accepted language. Four agents
have now caught this. Do NOT merge them. If part B cannot reach `column`
without merging, leave `column` hand-written and say so.

## PART C: `cst_origin/2` rows, 479 chars
Five rules have no DCG nonterminal naming the node: `expression`, `literal`,
`unary_expression`, `parenthesized_expression`, `member_expression`.

Round 3 measured that all but two fall to more `cst_origin/2` rows.
`expression` needs editor precedence tiering and `unary_expression` has no
clause at all. Take the three that fall. Report the two that do not with the
line that blocks each.

## OUT OF SCOPE, report only
The 101 chars where the editor is deliberately wider than the parser:
`enum_variant` and `query`. Round 3 raised three language questions about them
(enum variant field type, whether `? name(...)` keeps an `$.atom` node, whether
the editor shows `set` as a relation modifier). Those are the user's calls.
Restate them in your report; do not answer them.

## Files you own
| path | permission |
|---|---|
| `v6/labs/tree-sitter-door/**` | full |
| `v6/prolog/compile/parse_dl_dcg.pl` | ADDITIVE FACTS ONLY, zero parsing-behavior change |
| `plans/2026-08-11-tree-sitter-door.PLAN.md` | append a round-4 section |

Forbidden: everything else under `v6/prolog/`, plus `v6/dd-runner/**`,
`v6/boop/**`, `v6/tsv2/**`, `.github/**`, `chat_log/**`. Three other lanes are
live; one of them owns the rest of `v6/prolog/`.

## Gates, every commit
```bash
cd v6 && just parse-parity     # parity == total, skips=0, diffs=0. THE gate on the parser edit.
cd v6/labs/tree-sitter-door && ./run-tests.sh    # rc=0
cd v6 && just text-door        # compiled=272 byte_identical=272 failures=0
cd v6 && just green-all        # report the delta against your own stashed diff
```

A fact table cannot change parsing. If parity moves by one row, you broke
something: revert and report.

**KNOWN RED ON BASE, do not chase:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, 1 of 598),
`rtkq-golden`, `flagship`, `extraction-live`, `lsp-diags` (all four: missing
release extractor binary), `compile-speed` (baseline 2026-08-07), `tsv2-test`
(`hostDecode.test.ts:144`), `golden-flex` (stale `json_object/2` excuse). The
full list with exact failure text is `.github/CI-KNOWN-RED.md`. Zero legs may
turn red versus your own base measurement.

## Worktree setup you need first
`node_modules` is absent in a fresh worktree: `pnpm install` in `v6/tsv2` and
`v6/sprefa-store/js`. The text-door corpus is GENERATED: `cd v6/tsv2 && bash
scripts/sweep.sh`. The pre-commit rail needs
`v6/sprefa-extract/target/release/extract`; copy the prebuilt one from
/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract to
the same relative path. Four lanes have been stranded by this today. Never
`--no-verify`.

## Deliverable
A round-4 section in `plans/2026-08-11-tree-sitter-door.PLAN.md` with:
1. The per-part ratio table, baseline 0.3180 -> after A -> after B -> after C,
   one row per measurement with the command that produced it.
2. The regenerated classification table and the count of rules that moved to
   EMITTED-IDENTICAL.
3. The new floor: what is left, in chars, and why each piece cannot fall.
4. The three restated language questions, unanswered.
5. Gate output verbatim and the green-all delta.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Tables and lists over prose. Numbers come from tool output only.
- Never announce location in text ("here is", "below is", "the following").
