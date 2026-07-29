# comment-node dogfood header (user directive 2026-07-29): the golden comment techniques return to full parity in v6

User word: "start dogfooding with comment nodes our own cli and builtins etc.
either in the prolog or the generated code or both ... we had significant
comment_node techniques written down in golden use cases, i would like those
to return to full parity ability."

## The golden record (v5, all live and it-tested)

`comment_node(path, line, col, end_line, end_col, text, kind)` with
kind ∈ line|block|doc, grammar-backed (oxc for TS/TSX, tree-sitter for
rust/kotlin/python/go/...), string-literal-safe (a `//` inside a string is
never a row), text = token-stripped comment body. Golden contract:
tests/it/comment_node.rs. The technique inventory is
research/2026-07-15-comments-as-architectural-space.md; the seven techniques,
each with a live rail + it-test:

| # | Technique | v5 home | it-test |
|---|---|---|---|
| 1 | ARCH JSON markers -> named arch nodes/hierarchy | std/arch.dl | tests/it/arch.rs |
| 2 | dl-disable suppression + pairing + unused rail | std/suppress.dl | tests/it/suppress.rs |
| 3 | README(anchor) doc prose | examples/gen-readme.dl | tests/it/readme_gen.rs |
| 4 | LANG-JUNCTION(slug) distributed registry | examples/gen-lang-skill.dl | (drift rails in the .dl) |
| 5 | todo(category)/TODO/FIXME plans index | examples/gen-plans-index.dl | tests/it/plans_index.rs |
| 6 | BEGIN: gen zone ownership | examples/gen-zone-info.dl | tests/it/template_parts.rs |
| 7 | lint finding governed by suppression | examples/lint-unwrap.dl -> suppress | tests/it/suppress.rs |

The repeated shape: comment fact -> opt-in marker convention (rich `match`
regex capture over text, joined to comment_node at same path+line for the
grammar witness) -> typed rels -> diags/indexes/zones/graphs.

## v6 today (scouted 2026-07-29, receipts in chat)

- sprefa-extract cst family DOES emit comment nodes: `{"record":"node",
  "family":"cst","span":{start,end},"kind":"line_comment","name":null}` —
  byte spans, string-safe (tree-sitter), but NO text column and no
  line|block|doc normalization. Prolog .pl files are full-coverage, so
  dogfooding our own compiler sources is reachable today.
- v6 has NO regex/text-match construct (ts_query/sg_pattern exist; neither
  captures text fields from comment prose). The v5 marker technique
  (match-capture + join) has no v6 spelling.
- struct-as-rows landed: JSON payloads (technique 1's `ARCH {...}`) can live
  as declared structs; host output columns accept type names; spans are a
  declared struct.
- Spans are bytes; v5 rows are line/col. Parity grading vs v5 output needs a
  mapping somewhere.
- The extractor-is-fixed directive STANDS; the one prior waiver
  (--resolve) was scoped. SLOT-EXTRACTOR-WAIVER below.

## Lab contract (opus, worktree, lab dies on landing)

Grade, with runnable receipts, the SMALLEST CORRECT route to parity:

1. **Text acquisition** (the load-bearing question — where does comment TEXT
   come from?): price (a) extractor grows a `comment` family (span + line/col
   + kind line|block|doc + token-stripped text) — needs waiver; (b) NO
   extractor change: cst comment spans + a generic sh host slicing text by
   byte range (`tail -c`/`dd`), token stripping in-language or in the host
   template; (c) sh host does the whole job (grep -n over the file, no cst
   join — loses string-literal safety; measure a real false positive to
   prove the loss). Each priced on: statement counts at repo scale (v6/prolog
   has ~50 .pl files), content-addressed re-extraction behavior, and whether
   the string-safety witness survives.
2. **Marker capture**: v5's `match` regex capture has no v6 spelling. Price
   (a) per-marker sh host (grep/sed capture groups -> output columns);
   (b) extractor-side marker splitting (couples policy into rust — the
   suppress.dl header law says policy lives in the language, cite it);
   (c) a match/regex construct (NEW construct — needs the proven-gap
   discipline; state what program proves it, do not build it).
3. **Payload destructure**: ARCH's JSON payload through struct-as-rows
   (declared struct column on the host output + decode joins). Grade one
   real ARCH marker end to end.
4. **Two dogfood programs carried to receipts** (fixture/5 + a real-tree
   run, the extraction-live.sh precedent): technique 2 (dl-disable-line
   suppressing a diag from the live diag-rail.dl6 feed — composes with the
   shipped LSP bridge) and technique 1 (ARCH markers over v6/prolog/*.pl
   building the node/parent hierarchy). These two cover marker capture,
   grammar witness, JSON payload, and antijoin (unused-suppression) — the
   four capabilities all seven techniques decompose into.
5. **Parity grading**: the flagship rig precedent — run the v5 rail
   (std/suppress.dl or std/arch.dl) on a pinned corpus slice, run the v6
   port on the same files, classify every diff row. State which of the 7
   techniques the graded pair does NOT yet cover and what each needs.

## Named slots
- SLOT-EXTRACTOR-WAIVER: does the comment family enter the extractor?
  (User's dogfood directive implies but does not state the waiver; the lab
  prices the no-touch route so the user chooses with costs visible.)
- SLOT-MARKER-CAPTURE: sh-host regex vs extractor split vs new construct.
- SLOT-COMMENT-KIND-VOCAB: line|block|doc from tree-sitter kind names —
  where does the mapping live (extractor vs language)?
- SLOT-SPAN-UNITS: line/col columns vs byte spans + derived mapping; what
  does parity grading against v5's line/col output use?
- SLOT-TOKEN-STRIP: who strips `//`/`/*`/`%`/`#` tokens (v5: extractor).

## Laws riding this arc
- Zero new constructs unless a program PROVES the gap (extraction-lab
  discipline); every .dl6 snippet carries its rx lowering.
- Worktree, ff-only base check, no main-tree writes; lab dies on landing
  (durables -> fixtures/plans/rulings, deletion commit recorded).
- Vocabulary law: rxjs/prolog/SQL words only.
- The seven techniques are the definition of done for the ARC; the LAB's
  done = the graded route + the two receipt programs + slots filled or
  priced for user ruling.
