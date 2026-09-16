# BRIEF: clean-room DCG -> parser + pretty printer + tree-sitter grammar

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `e7558fc9` (origin/main). Your FIRST action:
  `git log --oneline -1`. Any other base = STOP AND REPORT.
- Everything you write goes in ONE directory:
  `v6/labs/cleanroom-dcg/$(git rev-parse --abbrev-ref HEAD | tr / -)/`
  Compute that path once, `mkdir -p` it, and never write outside it.

## One sentence
Write, from scratch and without reading this repo's existing parser, a
SWI-Prolog DCG for the `.dl6` language, then get THREE artifacts out of that one
DCG: a parser, a pretty printer, and a tree-sitter grammar; report with numbers
which of the three fell out of the DCG and which had to be hand-written.

## The experiment you are half of
Two agents are running this identical brief in isolation. The comparison is
architectural: rule count, how much of the printer and the tree-sitter grammar
were DERIVED from the DCG versus written by hand, and corpus parse rate. Do not
try to guess what the other agent is doing. Do not look for it.

## ISOLATION. This is the whole point of the lane.
You may read EXACTLY these, and nothing else in the repo:

| path | what it is |
|---|---|
| `v6/prolog/compile/SYNTAX.md` | 380-line language spec, your requirements doc |
| `v6/prolog/compile/dl_view/*.dl6` | 397 real programs, 8955 words, your ground truth |

FORBIDDEN, absolutely, no exceptions:
- `v6/prolog/compile/parse_dl_dcg.pl`
- `v6/prolog/compile/parse_dl.pl`
- `v6/prolog/print_dl.pl`
- `v6/labs/tree-sitter-door/**` (contains `grammar.js`, `emitted-grammar.js`,
  `emit_grammar.pl`, and three REPORT files)
- `v6/dl/grammar/dl.langium`
- every other `.pl` file in the repo
- every `.boop-worktrees/**` path

Do not grep for their contents either. Do not shell out to any existing parser
in this repo to check an answer. If you open a forbidden file by accident, say
so in one line at the top of REPORT.md; the run is still useful, silence is not.

## What the language looks like (so you start from the spec, not from guessing)
`SYNTAX.md` is the authority on spelling and semantics. One real corpus file,
so you know the register:

```
diag(Path, LineNo, 'warning', "unwrap-budget", concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
  unwrap_hit(Path, LineNo, Col, EndCol),
  changed_file(Path),
  unwrap_count(Path, Total),
  Total > 10.
```

Two spelling laws from `SYNTAX.md` that the corpus depends on and that are easy
to get wrong. Read the file for the rest, do not stop at these two.
1. A bare identifier is ALWAYS a variable. An atom-literal constant is ALWAYS
   single-quoted (`'warning'`, `'none'`). A string is ALWAYS double-quoted.
2. The printer therefore always quotes atom literals. Never Prolog's `~q`
   "quote only if necessary".

## Work, in this order. Commit after each numbered step.

### 1. Inventory before code
Read `SYNTAX.md`. Read enough of the 397 corpus files to see every construct.
Write `INVENTORY.md`: a table of construct -> `.dl6` spelling -> how many of the
397 files use it (count with grep, put the count in the table). No code yet.

### 2. `dcg.pl` -- the parser
A SWI-Prolog DCG over a code list, text to term. Target is 397/397.
Its term output is yours to design; that design is a finding, so state the term
shape in REPORT.md.

### 3. `harness.pl` -- measure yourself
Emits `results.tsv` to stdout, EXACTLY this format, tab-separated, one header
line then exactly 397 data rows in filename sort order:

```
file	parse_ok	roundtrip_ok
aggregate_count_min_max_track_arrivals_and_retraction.dl6	1	1
```

Run it as `swipl -g run -t halt harness.pl > results.tsv`.
`roundtrip_ok` is defined as: parse the text to `T1`, print `T1` to text2,
parse text2 to `T2`, and `T1 == T2`. TERM equality after re-parse. Not text
equality. A file that fails to parse is `0	0` and STAYS IN THE TABLE.
Never delete a row, never skip a file, never special-case a file by name.

### 4. The printer: try reverse mode FIRST
Before writing a line of printer code, try running `dcg.pl` backwards:
`phrase(program(Term), Codes)` with `Term` bound and `Codes` free.

- If it works: your printer is a few lines and that is a major finding.
- If it does not: report EXACTLY what blocked it, per construct. Name the goal.
  Cuts, `code_type/2` guards, arithmetic, and position reads via `mark`/`peek`
  are the usual blockers. Count them: `N cuts, M code_type guards, K position
  reads`. Then write `print.pl` by hand and say so.

Either way REPORT.md answers: **was the printer derived or hand-written**.

### 5. `grammar.js`: try EMITTING it from the DCG first
Load `dcg.pl` with `read_term/3` and walk the clauses as terms; a DCG clause is
an ordinary Prolog term, so an emitter can read your own grammar and print
tree-sitter rules. Write `emit.pl` that does as much of `grammar.js` as it can.
Whatever the emitter cannot reach, hand-write into an overlay file, and count
BOTH: emitted rules versus hand-written rules, and non-whitespace characters of
each. Gate: `tree-sitter generate` must succeed, then parse the corpus:

```
tree-sitter generate
tree-sitter parse ../../../prolog/compile/dl_view/*.dl6 --quiet --stat
```

Record the totals line verbatim.

### 6. `REPORT.md`
In this order:

1. The metric table. Every row carries the command that produced it.

| metric | value | command |
|---|---|---|
| DCG lines | | `wc -l dcg.pl` |
| DCG nonterminals | | count of `-->` clause heads |
| printer lines | | `wc -l print.pl` |
| grammar.js lines | | `wc -l grammar.js` |
| tree-sitter named rules | | count of keys in the rules object |
| corpus parse | X/397 | `results.tsv` column 2 sum |
| round-trip | X/397 | `results.tsv` column 3 sum |
| tree-sitter parse | X/397 | `--stat` totals line |
| grammar chars emitted vs hand | E / H | `emit.pl` output vs overlay |

2. **Printer origin**: derived from the DCG, or hand-written, and the blocker
   list with counts if hand-written.
3. **Grammar origin**: which rules the emitter reached, which needed hand code,
   and WHY each hand rule resisted.
4. Your term shape, in one code block.
5. The construct rows you could not parse at all, with the corpus filenames.
6. One mermaid diagram of your architecture: the DCG in the middle, the three
   consumers around it, each edge labelled derived or hand-written.

## Anti-cheat
| rule | why |
|---|---|
| Every one of the 397 files appears in `results.tsv` | a parse rate computed over a filtered corpus is a fabricated number |
| Round-trip is term equality after RE-PARSE | text equality lets a printer that emits garbage pass by echoing input |
| No corpus file is special-cased by name in any code path | that is memorization, not a grammar |
| No shelling out to an existing repo parser | isolation is the experiment |
| `tree-sitter generate` must exit 0 before you report any grammar number | an ungenerated grammar has no rules |
| Numbers in REPORT.md come from tool output you actually ran | no estimates, no "approximately" |

## Worktree setup, before your first commit
The pre-commit hook runs a comment-budget rail that needs a built binary. Copy
it once, do not build it:

```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
```

`git commit -n` and `--no-verify` are FORBIDDEN. A blocked commit is a real
finding; fix the comment budget instead, or STOP AND REPORT.

## Rails
- The 10-second law: any single command over 10 seconds is a defect to
  investigate, not a budget to spend. 397 tiny files parse in well under that.
- Commit after each numbered step, with the step's numbers in the message.
- You own ONLY your lab directory. Zero files outside it may appear in
  `git status`. Never spawn a subagent.

## Style laws, inline so you need no judgment
- No em dashes.
- Banned words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base layer, critical, mode.
- The word "refusal" is banned in prose; an unbuilt construct is "TODO" or
  "not built yet".
- No `here is`, `here's`, `below is`, `the following`. The content just starts.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no restating the next line.
- Tables and lists over prose. Prose is a one-line caption under a table.
- Variable names are descriptive, never single-letter, in Prolog and JS alike.
