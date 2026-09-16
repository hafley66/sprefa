# LANE: `import pokeapi.json` + hover showing the expanded sprefa types

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only e7558fc9
```
Failure or missing trees = STOP AND REPORT. No archive/tar/copy workaround.

## WORKTREE SETUP BEFORE FIRST COMMIT (never `--no-verify`)
1. copy `v6/sprefa-extract/target/release/extract` from the main tree in
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## THE USER'S ASK, VERBATIM
"i would love to wake up to a line that says import poekapi.json etc. and then
its hover of that import statement is a diag in vscode with html enabled that
says what the types are expanded to in sprefa from the openapi spec bc dl6 can
program its own lsp"

## THE USER IS RIGHT THAT THE DOOR ALREADY EXISTS. USE IT, BUILD NO LSP CODE.
`src/engine/decls.rs:294-303` declares a sink relation:

```
hover_note(path: File, line: Int, col: Int, end_line: Int, end_col: Int, md: Text)
```
its own doc: "markdown hover note attached to a source span; head it from a
rule, shown by the LSP on hover. Positions are 0-based, same convention as
diag (end_line/end_col inclusive); several notes on one span all show,
appended after the synthesized entity hover".

`src/engine/lens.rs:294` `hover_notes_at/3` selects `md` for the covering span
and TOLERATES a db where the table never derived (returns empty, never errors).
`src/lsp.rs:73` already advertises `hover_provider`. `src/lsp.rs:857-863`
merges note markdown after the synthesized entity hover.

THE PRECEDENT TO COPY EXACTLY is the diagnostics bridge, and read its script
header first: `v6/tsv2/scripts/lsp-diags.sh`. Quoting it: "live editor
diagnostics with ZERO new LSP code ... THE BRIDGE IS ENTIRELY IN-LANGUAGE, not
a serve-side projection: v6's rel table naming is the bare rel name
(`v6/prolog/compile/lower.pl:156` `table_name(Name/_Arity, Name)`) so a `.dl6`
rel literally named `diag_v5` with v5's 9 columns in order compiles to a real
SQLite table named `diag_v5` -- the exact identifier `poll_diag_v5_once`
(`src/lsp.rs:537`) selects from. No CREATE VIEW, no COALESCE, no serve/*.ts
touched."

So: a `.dl6` rel named `hover_note` with those 6 columns in that order
compiles to the table `hover_notes_at/3` already reads. That is the whole
mechanism. IF YOU FIND YOURSELF EDITING RUST LSP CODE, YOU HAVE TAKEN THE
WRONG PATH; stop and report.

## THE THREE PIECES, IN DEPENDENCY ORDER

### Piece 1: the `import` statement in the dl6 surface
There is NO import statement today. `grep -n import
v6/prolog/compile/parse_dl_dcg.pl` returns only a module comment at :16.

Add it to the DCG. Follow the shape of an existing statement; `rel_stmt//1` at
`parse_dl_dcg.pl:454` is a good model. THE STATEMENT MUST RECORD ITS OWN SPAN
(line, col, end_line, end_col), because piece 3 attaches a hover note to
exactly that span. The DCG already carries position machinery: `mark/1` and
`peek/1` appear 19 times. USE THE EXISTING MECHANISM; do not invent a second
position scheme.

Surface spelling: `import "pokeapi.json".` unless the existing statement
grammar makes another spelling obviously more consistent. If you pick
differently, say why in the report. DO NOT invent new type spellings or
language semantics beyond the statement itself; the lang-design law reserves
that for the user.

### Piece 2: the OpenAPI expansion, reachable as data
`v6/tsv2/gen/pokeapi_gen.dl6` is what `openapi_to_dl6.ts` produces today: a
build-time TypeScript converter writing a .dl6 file. The hover text is
"what the types expanded to", so what you need is the mapping from the
imported spec to the emitted `rel` declarations.

CHEAPEST HONEST PATH FIRST: the converter already knows this mapping. Have it
also emit the per-schema expansion as data the hover rule can read, rather
than re-deriving it. Do not rewrite the converter in prolog.

### Piece 3: the rule that heads `hover_note`
A `.dl6` rule that, for each import statement span, heads a `hover_note` row
whose `md` is the expanded type list.

## MARKDOWN, NOT ARBITRARY HTML — SAY THIS BACK TO THE USER IN YOUR REPORT
The user wrote "a diag in vscode with html enabled". The LSP hover payload is
`MarkupContent` with `MarkupKind::Markdown`. VS Code renders that markdown
through a SANITIZER: fenced code blocks, tables, lists, bold, and links all
work; arbitrary HTML and any script or style are stripped. So the rich hover
they want is reachable, through markdown tables and fenced `dl6` code blocks,
NOT through raw HTML. MEASURE THIS, do not take my word for it: render a note
containing a markdown table and a fenced block, and one containing an HTML
tag, and report what VS Code actually displayed for each.

Aim the hover content at a markdown table plus a fenced `dl6` block showing
the emitted `rel` lines. That reads better than HTML would anyway.

## SCOPE HONESTY: WHAT IS GATED AND WHAT IS NOT
pokeapi G1 currently sits at 4 drops, blocked on a CONCURRENT LANE
(`fix-zero-column-ref-target`). Pieces 1 and 3 do NOT depend on that: the
import statement and the hover_note bridge can be built and proven on ANY
imported spec, including a 3-line hand-written one. BUILD AND PROVE THOSE
FIRST on a tiny fixture. Only then try the full pokeapi spec, and if the
remaining drops block it, report that plainly rather than waiting.

## FILES YOU OWN
```
v6/prolog/compile/parse_dl_dcg.pl        (the import statement ONLY)
v6/tsv2/src/openapi_to_dl6.ts            (expansion-as-data emit)
v6/dl/fixtures/                          (new fixtures only)
v6/tsv2/scripts/                         (a receipt script, modeled on lsp-diags.sh)
```
DO NOT EDIT: `src/**` (all v5 rust LSP code), `v6/prolog/lower.pl`,
`v6/prolog/compile/0_generic_expand.pl` (concurrent lane),
`v6/dd-runner/**`, `v6/bench-cli/**`, `v6/justfile` (concurrent lane).
If you need one, STOP AND REPORT.

## TREE-SITTER COUPLING, DO NOT MISS THIS
The tree-sitter grammar is EMITTED from the DCG by
`v6/labs/tree-sitter-door/emit_grammar.pl`, which reads `parse_dl_dcg.pl` as
terms. Adding a statement to the DCG changes that emitted grammar. Run
`v6/labs/tree-sitter-door/run-tests.sh` and `measure.py` and report the ratio
before and after. Baseline: overlay 445, ratio 0.1021, TS_CORPUS 286/286.
A new statement that lands in the hand overlay instead of the generated rules
is a finding worth reporting, not a failure.

## FAIL-FIRST RECEIPT, REQUIRED
1. A `.dl6` with an import statement FAILS TO PARSE before piece 1. Paste it.
2. A hover at the import span returns NO note before piece 3. Paste it.
Then both green. A report without red-then-green is rejected.

## ANTI-CHEAT TABLE
| banned | why |
|---|---|
| editing v5 rust LSP code | the door exists; needing to change it means you took the wrong path |
| a CREATE VIEW or COALESCE bridge | `lsp-diags.sh` proved the bare-rel-name path; a projection layer is the thing that precedent exists to avoid |
| claiming VS Code renders HTML without testing it | measure both cases and paste what you saw |
| inventing type spellings or language semantics | lang design is the user's, per the standing law |
| widening a fixture to match output | that is deleting the test |
| `--no-verify` | the rail is the gate |

## GATE (run, paste output)
```
cd v6/prolog && swipl -g go -t halt ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh
cd v6/labs/tree-sitter-door && ./run-tests.sh && python3 measure.py
just green-all
```
Baseline: conformance 281/0, plunit 276, TEXT_DOOR 196/196/0, tsv2 128/1skip,
store 74/74, dl 96/96, sweep identical=283 wrong=0.

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose, say TODO or "not built
yet". Comments state ONLY constraints the code cannot show; no change-log
narrative, no dates, no arc references. Descriptive names, never
single-letter. Construct names use ONLY rxjs, prolog, or SQL vocabulary.
Colocated consistency inside a file.

## COMMIT OFTEN. A prior lane lost a whole run to a machine sleep.

## REPORT
`REPORT.md` at the worktree root: (1) the import spelling you chose and why,
(2) red-then-green for both fail-first receipts, (3) WHAT VS CODE ACTUALLY
RENDERED for markdown table, fenced block, and raw HTML, (4) tree-sitter ratio
before/after, (5) every gate command with pasted output, (6) whether the full
pokeapi spec worked or was blocked by the remaining G1 drops, (7) what you did
NOT do. Do not open a PR. Do not spawn subagents; lanes never fan out.
