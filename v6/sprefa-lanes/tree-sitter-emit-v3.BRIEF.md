# BRIEF: tree-sitter emitter, round 3 (CST/LSP fact tables in the DCG)

## Base
- Branch: `lab/tree-sitter-emit-v3`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `b580d627` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## One sentence
Round 2 left 2767 non-whitespace characters of hand-written Tree-sitter grammar
and a ratio of 1.68; drive that number down by (a) teaching the emitter to read
shapes already present in the DCG and (b) adding declarative CST/LSP fact tables
to the parser, measuring each change separately.

## User decision that opens this round (2026-08-11, verbatim)
"even if we have to add lsp or cst intended data into parser code to data-ify
the boundaries that would be fine with me unless some reason forbids it in
prolog and its dcgs"

So: adding facts to `v6/prolog/compile/parse_dl_dcg.pl` is APPROVED. Nothing in
Prolog forbids it. Facts and DCG clauses coexist in one module, and
`emit_grammar.pl` already loads the file with `read_term/3`, so it sees new
facts with no loader change.

## Files you own
| path | permission |
|---|---|
| `v6/labs/tree-sitter-door/**` | full |
| `v6/prolog/compile/parse_dl_dcg.pl` | ADDITIVE FACTS ONLY, zero parsing-behavior change |
| `plans/2026-08-11-tree-sitter-door.PLAN.md` | append round-3 section |
| `v6/labs/tree-sitter-door/REPORT4.md` | create |

Touch nothing else. Explicitly forbidden: `v6/boop/src/**` (another session's
uncommitted work), `chat_log/**`, every other file under `v6/prolog/`.

## Work, in this order. Measure and commit after EACH.

### Phase 1, emitter-only. No Prolog change. Do this first, it is free.
Both facts are already in the DCG; the emitter is not looking at them.

1. `token.immediate`. The DCG expresses immediacy by NOT calling `ws//0`
   between two goals. Detect adjacency in the parsed clause body and emit
   `token.immediate(...)`. Targets: `member_access`, `capture_key`.
2. `repeat1` by clause-pair shape. A rule written as the pair
   `X --> item, sep, X.` and `X --> item.` is a `repeat1`/`repeat` with a
   separator. Detect the pair and emit the Tree-sitter repetition directly
   instead of leaving recursion for the hand overlay. Targets:
   `enum_variants`, `list`, `object_pattern`, `path`, `match_statement`.

Receipt for phase 1: rerun the emitter, recount, report the new ratio and which
of the 7 named rules changed classification.

### Phase 2, additive fact tables in `parse_dl_dcg.pl`.
Four tables. Add each one separately, measure separately.

| fact | rows | what it records |
|---|---|---|
| `cst_shape/2` | ~20 | rule name -> ordered field-name list, e.g. `cst_shape(rel_decl, [name, columns, modifiers]).` |
| `lex_token/2` | ~4 | raw escape-preserving regex span beside the semantic escape decoders: `string`, `quoted_atom`, `template` |
| `cst_extra/2` | ~1 | comments. `ws//0` at `parse_dl_dcg.pl:243` EATS comments before any emitter can see them; this records that they exist and belong in Tree-sitter `extras` |
| `cst_origin/2` | ~2 | where the parser erases a distinction the editor keeps: `fact` normalized into a rule-with-`true`-body, `spread` built as a semantic term with no CST boundary |

**FIELD NAMES ARE NOT YOURS TO INVENT.** The hand `grammar.js` already contains
26 `field(...)` calls. Every `cst_shape/2` row copies the field name ALREADY
SPELLED in `grammar.js`. If a rule needs a field name that does not appear in
`grammar.js`, do NOT coin one: list it in REPORT4.md under "NEEDS USER WORD"
and leave that rule in the hand overlay. A new user-facing name is a language-
surface decision and belongs to Chris.

### Phase 3, measure the floor.
Round 2's mapping predicts 2 irreducible rules: `source_file` (root repetition
+ error recovery) and `declaration_parameter` (admits half-typed input). The
compiler REJECTS malformed programs; an editor must keep working on them.
Report whether that is really the floor or whether recovery can be declared
too. Do not build a recovery declaration; answer the question with evidence.

## Gates. All of these, every commit. Non-negotiable.
```bash
cd v6 && just parse-parity        # MUST stay 677/677 skips=0 diffs=0
cd v6/labs/tree-sitter-door && ./run-tests.sh   # rc=0
cd v6 && just text-door           # TEXT_DOOR compiled=272 byte_identical=272 failures=0
cd v6 && just green-all           # final gate before you report done
```
Parity is THE gate on the parser edit. A fact table cannot change parsing. If
parity moves by one row, you broke something; revert and report.

## Known fatal, do not repeat
- **The cut trap.** `decl_b_column_type//3` and `host_col_type//3` differ ONLY
  in a cut. Merging them PASSES PARITY while silently widening the accepted
  language. Two prior agents caught this and refused. Do not merge them.
- **Compressing the parser makes it LESS emittable.** Round 2 measured
  108 clauses / 34 translatable at 29534 chars versus 103 / 32 at 26473, ratio
  2.74 -> 2.96. Your job is emittability, NOT character count on the parser.
  Do not shrink `parse_dl_dcg.pl`. It grows by exactly the fact rows you add.
- **The text-door corpus is GENERATED, not committed.** `run-tests.sh` reads
  `v6/prolog/compile/out/text-door/`. Regenerate with
  `cd v6/tsv2 && bash scripts/sweep.sh` if it is empty.
- **Locale.** Every `open/3` needs an explicit encoding; the hub fix lives in
  `compile.pl`. Under `LC_ALL=C` swipl defaults to ASCII `text`. Failure-modes
  class 46.
- A REPORT file at the root proves nothing. Every number in REPORT4.md carries
  the command that produced it.

## Deliverable
`v6/labs/tree-sitter-door/REPORT4.md` containing, in order:
1. The per-phase ratio table: baseline 1.68 -> after phase 1 -> after each of
   the four fact tables. One row per measurement, with the command.
2. The regenerated 43-row classification table (round 2's is REPORT3.md:54-98),
   showing which rules moved from EMITTED-NEEDS-OVERLAY or HAND-ONLY to
   EMITTED-IDENTICAL.
3. A "NEEDS USER WORD" section: every field name you could not copy from
   `grammar.js`.
4. The phase-3 answer on the floor.
5. Gate output, verbatim, all four commands.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- The word "refusal" is banned in prose; an unbuilt construct is "TODO" or
  "not built yet". It stays only in literal code identifiers.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, no restating the next line.
- Tables and lists over prose. Numbers come from tool output only.
- dl variable names are descriptive, never single-letter.
