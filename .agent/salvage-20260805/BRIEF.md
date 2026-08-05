# BRIEF: mdquery — markdown + html languages for `extract query`

You are the mdquery lane, worktree `~/projects/sprefa-lanes/mdquery`, branch
`lab/md-html-query`, base `57430529`. FIRST ACTION: `git merge --ff-only
57430529`; any failure = STOP, write STOP.md, do nothing else. If reality
deviates from this brief at any point, STOP and report; do not improvise.

## Task

`extract query --lang md` and `--lang html` must work: full tree-sitter
queries over markdown and html documents, same JSONL capture contract as the
existing rust/ts/go legs. This fills the known markdown hole (ARCH row
doc_format_extraction names it) and is the base for querying prose the way we
query code.

## Precedent (verified receipts, read them first)

- v5 already shipped markdown CST: root `Cargo.toml:112-116` pins
  `tree-sitter-md = "0.5"` (MDeiml two-parser split, block + inline; v5 used
  the BLOCK language). Wiring lives in root `src/cst.rs`, `src/ingest/mod.rs`,
  `src/engine/decls.rs`. Read `src/cst.rs`'s md leg before writing yours.
- v6 target: `v6/sprefa-extract/` pins `tree-sitter = "0.25"`
  (Cargo.toml:45), which is the same major v5 pairs with tree-sitter-md 0.5,
  so version unification is expected; verify with `cargo tree -i tree-sitter`
  after adding deps.
- The query executor you are extending: `v6/sprefa-extract/src/0_query.rs`
  (language registry + capture emission). Existing tests:
  `v6/sprefa-extract/tests/9_query_cli.rs`.

## Build-vs-buy (do this FIRST, in the report)

Candidate table before any code, comparing grammar crates for each language:
markdown (tree-sitter-md 0.5, the newer tree-sitter-markdown forks) and html
(tree-sitter-html crate versions compatible with tree-sitter 0.25). Only
tree-sitter grammars fit the existing executor, so the analysis is which
crate + version, with the compat receipt (`cargo tree` output). One-line
dismissals are banned; state why the chosen pin wins.

## Design constraints

- `md` = the BLOCK grammar (headings, fenced blocks, paragraphs, lists),
  matching v5's choice. The inline grammar (emphasis, inline code, links
  inside paragraphs) is a SECOND parser in the same crate: expose it as lang
  `md_inline` ONLY if it drops into the executor with no structural change;
  otherwise name the gap in the report and skip it. Do not build a two-pass
  merged tree.
- html: one grammar, lang name `html`.
- The prolog side keeps parity: the known-language list that backs the
  `ast_lang_unknown` refusal lives in `v6/prolog/0_ast_expand.pl`; add the
  new names there and update the matching plunit expectation in
  `v6/prolog/compile/test/plunit_tests.pl` if one pins the list. You own
  ONLY those two prolog edit sites; nothing else outside v6/sprefa-extract/.

## Files you own (disjoint ownership, touch nothing else)

- v6/sprefa-extract/Cargo.toml (+ lockfile via cargo)
- v6/sprefa-extract/src/0_query.rs
- v6/sprefa-extract/tests/9_query_cli.rs (append new tests)
- v6/prolog/0_ast_expand.pl (language list only)
- v6/prolog/compile/test/plunit_tests.pl (only if a test pins the list)

## Validation (run all, paste outputs verbatim in the report)

```bash
cd v6/sprefa-extract && cargo build --release --features cli --bin extract
cargo test --release --features cli
printf '# Title\n\nA paragraph.\n\n```js\ncode\n```\n' > /tmp/md_probe.md
./target/release/extract query --lang md --query '(atx_heading) @heading' /tmp/md_probe.md
printf '<div class="x"><p>hi</p></div>' > /tmp/html_probe.html
./target/release/extract query --lang html --query '(element (start_tag (tag_name) @tag))' /tmp/html_probe.html
cd ../.. && cd v6/prolog && swipl -g run_tests -t halt compile/test/plunit_tests.pl 2>&1 | tail -3
```

Cargo may need the network for the two new crates; that is sanctioned. Never
run npm or pnpm anywhere.

## Deliverable

Commit on this branch (pre-commit rail may need
`SPREFA_COMMENT_RAIL_DL6=0` in a fresh worktree; that fallback is
sanctioned). REPORT-MDQUERY.md at worktree root: candidate table, changes
file:line, verbatim gate outputs, deviations section (write "None." only if
literally true). NOT named REPORT.md.

## Style laws

Comments state only constraints the code cannot show; max 2 consecutive
comment lines. No em dashes. Banned words in prose and identifiers:
provenance, substrate, load-bearing, regime. Match 0_query.rs's existing
error style: named one-line stderr, exit 2.
