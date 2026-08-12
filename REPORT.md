# REPORT: `import` statement + hover showing the expanded sprefa types

Lane: `feature/import-openapi-hover`. All three pieces built and proven on the
`.dl6`/engine side. The editor-side delivery (real VS Code hover) is gated on a
v5 bridge that does not exist as the lane claimed; that finding is item 3 and
section "v5 bridge finding" below.

## 1. Import spelling and rationale

Surface: `import "spec.json".`

```
import "pokeapi.json".
```

Spelling model is `use_item//1` in `parse_dl_dcg.pl` (a `kw` + `string_lit` +
`.`, the same shape `use`/`rel`/`sh` use). It reuses the existing `string_lit`
token and the existing position machinery (`here/1` + `remaining_line_column/3`,
zero new position scheme). No new type spelling or language semantics were
invented; the statement only records its own span 0-based/inclusive as
`import_decl(File, Line, Col, EndLine, EndCol)`.

Recorded span for `import "pokeapi.json".` is `import_decl(pokeapi.json, 0, 0, 0, 21)`
(0-based lines/cols, end inclusive at the final `.`).

## 2. Fail-first receipts (red then green)

### Receipt 1: the import statement

RED, against the pre-piece-1 parser (`git show HEAD:.../parse_dl_dcg.pl`):

```
error(dl_parse_error(statement,position(1,8)))
```

The `import` token was not a statement; the parse died at col 8.

GREEN, with the piece-1 DCG change (`import-mini.dl6`):

```
import_decl(pokeapi.json,3,0,3,21)
```

(Line 3 is 0-based because the fixture file carries a 2-line header comment;
the identifier and inclusive end span are correct.)

### Receipt 2: hover_note rows before and after the derive rule

A hover at the import span yields no note when the program derives no
`hover_note` row, and yields notes when the rule heads the sink.

RED, `hover-rail` without the `hover_note` rule: the emitted program has no
`INSERT OR IGNORE INTO "hover_note"`, so the table never derives and no note
exists.

GREEN, with the `hover_note` rule in `import-hover-rail.dl6`: the derive emit
appears

```
INSERT OR IGNORE INTO "hover_note" ("path", "line", "col", "end_line", "end_col", "md")
  SELECT b0."path", b0."line", b0."col", b0."end_line", b0."end_col", b1."decl"
  FROM "import_stmt" b0, "schema_expansion" b1 WHERE b1."spec" = b0."spec"
```

and the table is created with v5's exact 6-column shape `hover_note("path",
"line", "col", "end_line", "end_col", "md")`.

## 3. What VS Code actually rendered

I could not produce a live VS Code render in this environment, and the hover
bridge (section below) is gated, so no note reached an editor. What I measured
and state precisely:

Server payload (measured from `src/lsp.rs:904-909` and the note composition at
`857-897`): v5 returns `HoverContents::Markup(MarkupContent{ kind: Markdown,
value: <notes joined with "\n\n---\n\n"> })`. The server performs NO sanitization:
raw HTML inside `md` passes through the payload byte for byte. The three note
forms `import-hover-rail.dl6` carries are:

- fenced block: ` ```dl6\nrel ability_detail(id: int, name: text, ...).\n``` `
- markdown table: `| native | dl6 |\n| --- | --- |\n| int | integer |\n...`
- raw HTML: `<b>raw html</b><script>evil()</script>`

VS Code renders untrusted hover markdown (`MarkdownString`, `isTrusted=false`)
through its markdown-it sanitizer: fenced `dl6` code blocks and markdown tables
render as a code block and a table; inline `dl6` code and bold render; raw HTML
tags are stripped (the `<b>` is not applied as bold, the `<script>` is removed).
This is VS Code's documented sanitizer behavior for hover, not verified by a GUI
run here. Anyone who wants the byte-level editor truth must run a GUI VS Code
against a v5 LSP; the payload above is the exact input it would render.

## 4. Tree-sitter ratio before / after

| metric | before | after |
| --- | --- | --- |
| identical hand-rule spans | 3426 | 3453 |
| generated specialized rules | 934 | 934 |
| emitted total | 4360 | 4387 |
| remaining hand-rule overlay | 445 | 445 |
| ratio | 0.1021 | 0.1014 |
| TS_CORPUS | 288/288 | 288/288 |
| TEXT_DOOR | 288/288/0 | 288/288/0 |

The `import` statement landed in the GENERATED rules (`seq("import", $.string,
".")` inside `statement`), not the hand overlay: overlay stayed 445 while `ratio`
fell. That is the favorable outcome; no hand-overlay finding to report.

Derived-grammar files were regenerated: `emitted-grammar.js > grammar.js` plus
`src/{grammar.json,node-types.json,parser.c}`.

## 5. Gate commands

`cd v6/prolog && swipl -g go -t halt ARCH.pl` -> all PASS (ends
`covers_endpoints_ground`).

`cd v6/tsv2 && bash scripts/sweep.sh` -> `RUN total=286 identical=283 wrong=0
emitted_crash=0 rejection=3`, `FINAL identical=283 wrong=0`. Matches the stated
baseline (identical=283 wrong=0).

`cd v6/labs/tree-sitter-door && ./run-tests.sh && python3 measure.py` -> exit 0,
`TS_CORPUS total=288 clean=288 errors=0`, ratio 0.1014, `PASS parse:
golden-flex.dl6 lines=630 errors=0`, `PASS format`.

`just green-all` -> GREEN ALL FAILED, but every red leg is a pre-existing
baseline failure, reproduced on a clean `HEAD` checkout (I stashed all my
changes and re-ran): plunit 5 failures (json_merge_patch stand-in x3,
catalog_plane corpus counts, expression inventory), tsv2 3 failures
(bopCheck ghcacher exit code, the matching load-over-http check, one sh-host
grid order), leak-soak (mktemp temp-file collision, an infra race), and
rtkq-golden (row ordering nondeterminism). None touch import/hover/openapi. My
own additions: tsv2 openapi suite 10/10 pass; `import-hover-receipt.sh` HOLDS.

## 6. Full pokeapi spec

`openapi_roundtrip_check.ts` PASS: `componentName:212 propName:786
kind:786/0/0 refTarget:257/0/0 nullable:786/0/0`. Piece 2 now also writes
`gen/pokeapi_expansion.dl6` (373 `schema_expansion` facts, one per emitted rel,
each carrying its PascalCase component source), which compiles clean
(`COMPILE-TRACE ... total=1641`). Pieces 1 and 3 were proven on a tiny fixture
first, per the lane order. The remaining 4 G1 drops are unchanged and blocked on
the concurrent lane `fix-zero-column-ref-target`; they fall outside pieces 1-3
and do not gate them.

## 7. What I did NOT do

- Did not edit any v5 Rust LSP code.
- Did not build a CREATE VIEW or COALESCE projection bridge.
- Did not use `--no-verify` (the pre-commit hooks, comment-budget rail and
  `comment-prod`, both pass).
- Did not invent type spellings or language semantics beyond the statement.
- Did not widen a fixture to force a pass.
- Did not spawn subagents.
- Did not lower the `import` statement into a runnable rel (that needs the
  compiler lowering, `0_generic_expand` / `1_expansion` / `lower.pl`, which are
  outside my owned files). The hover derive reads the span from an
  `import_stmt` data rel and the expansion from the Piece 2 data.
- Did not deliver the hover to a real VS Code (headless plus the bridge wall).

## v5 bridge finding (the gated delivery)

The lane asserted a `.dl6` rel named `hover_note` (bare table `hover_note`)
compiles to the table v5's `hover_notes_at/3` reads. That holds for the DIAG
bridge (v5's `poll_diag_v5_once` selects the bare hardcoded identifier
`diag_v5`, `src/lsp.rs:633`) but NOT for hover:

- `hover_notes_at/3` (`src/engine/lens.rs:294`) reads `txt_tbl("hover_note")`
  = `rel_hover_note_txt` (v5 naming, `src/lower.rs:10`), not the bare `hover_note`
  a `.dl6` compile emits.
- v5 has no `--hover-db` foreign-read mode. The `--diag-db` mode
  (`run_diag_db_mode`, `src/lsp.rs:495`) polls only `diag_v5` and never answers
  `textDocument/hover`. `hover_notes_at` runs only in the full `--lsp` engine
  mode, against v5's own compiled db.
- Therefore a v6-produced `hover_note` table never reaches v5's hover handler.

Making hover reach an editor requires v5 Rust edits (a hover foreign-read seam,
or aligning `hover_notes_at` to the bare/delta table). That is the exact "you
took the wrong path" signal the lane defines, so I stopped there and report it
rather than editing `src/**`. The `.dl6` side is complete and proven: the
statement records its span, the converter emits the expansion as data, and a
program heads the v5-shaped `hover_note` sink from that data.
