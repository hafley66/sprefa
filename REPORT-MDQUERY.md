# REPORT-MDQUERY — markdown + html languages for `extract query`

## One line

`extract query --lang md`, `--lang md_inline`, and `--lang html` now run full
tree-sitter queries over markdown/html documents with the same JSONL capture
contract as the rust/ts/go legs, on the v6 extraction leaf.

## Candidate table (build-vs-buy, decision first)

Only tree-sitter grammars fit the executor (`0_query.rs` drives a
`tree_sitter::Language`), so the field is which crate + version. The two
winners are already the v5 pins or already-unified transitive deps.

**Markdown**

| candidate | version | tree-sitter pairing | verdict |
|---|---|---|---|
| tree-sitter-md (MDeiml) | 0.5.3 | tree-sitter-language only (default-features-off), pairs with the 0.25 runtime | **WIN** |
| tree-sitter-markdown (ikatyang) | 0.7.1 | hard-deps tree-sitter@0.19 | reject: forks a second tree-sitter core (violates the unify rule) |
| tree-sitter-markdown-fork | 0.7.3 | fork of the 0.19 lineage | reject: same dup, off-mainline, divergent from v5 oracle |
| tree-sitter-md-025 | 0.5.6 | subsriram fork, no v5 precedent | reject: not the v5 pin; divergent grammar tree from the oracle |

tree-sitter-md 0.5.3 wins because it is the exact crate v5 pins
(`Cargo.toml:116`; wired in `src/cst.rs`/`src/ingest/mod.rs`), so the block
`LANGUAGE` parse is byte-identical to the v5 oracle's. Its default-features-off
means it ships only `tree-sitter-language` and commits to no tree-sitter core
version, so it slots cleanly onto the existing 0.25 runtime. `tree-sitter-md`
0.5.3 exports the two-parser split: `LANGUAGE` (block) and `INLINE_LANGUAGE`
(inline).

**HTML**

| candidate | version | tree-sitter pairing | verdict |
|---|---|---|---|
| tree-sitter-html | 0.23.2 | tree-sitter-language only (deps `cc` + `tree-sitter-language`) | **WIN** |
| none else | | | already a strict transitive of ast-grep-language 0.38.7 |

tree-sitter-html 0.23.2 wins because it is already in this lock as a strict
transitive of ast-grep-language (previous lock entry), so adding it as a direct
dep unified to one copy, no new tree-sitter version.

**Compat receipt** (`cargo tree -i`):

```
tree-sitter v0.25.10            <- one unified core
├── ast-grep-core v0.38.7
│   ├── ast-grep-language v0.38.7
│   │   └── sprefa-extract v0.1.0
│   └── sprefa-extract v0.1.0
├── ast-grep-language v0.38.7 (*)
└── sprefa-extract v0.1.0
tree-sitter-md v0.5.3           <- direct, LanguageFn only
└── sprefa-extract v0.1.0
tree-sitter-html v0.23.2        <- unified with ast-grep-language's copy
├── ast-grep-language v0.38.7
│   └── sprefa-extract v0.1.0
└── sprefa-extract v0.1.0
```

## Changes file:line

| file | change |
|---|---|
| `v6/sprefa-extract/Cargo.toml:57-67` | add `tree-sitter-md = "0.5"` (57-64), `tree-sitter-html = "0.23"` (65-67); comment runs kept to max 2 consecutive lines for the prose rail |
| `v6/sprefa-extract/Cargo.lock` | cargo-added tree-sitter-md/0.5.3 + tree-sitter-html/0.23.2 entries (12 lines) |
| `v6/sprefa-extract/src/0_query.rs:80-83` | `query_language` gains `md` (block `LANGUAGE`), `md_inline` (`INLINE_LANGUAGE`), `html` |
| `v6/sprefa-extract/tests/9_query_cli.rs:156-229` | `temp_file` helper + three tests (md block, md_inline, html) |
| `v6/prolog/0_program_check.pl:245` | whitelist gains `md`, `html` (RESUME amendment; file newly owned) |

`v6/prolog/0_ast_expand.pl` needed no edit: it holds no language list, only
the refusal-name mapping at `:38`/`:89` (verified by rg, zero membership pins).
The plunit tests at `plunit_tests.pl:2934`/`:2940` pin a literal `plain` and an
anonymous variable, not the whitelist, so no plunit edit; the suite passes
unchanged.

## Gate outputs (verbatim)

Build + test:

```
Finished `release` profile [optimized] target(s) in 32.20s

running 8 tests
test query_html_grammar_emits_tag_names_jsonl ... ok
test query_markdown_block_grammar_emits_headings_jsonl ... ok
test query_markdown_inline_grammar_drops_in_without_structural_change ... ok
test query_rejects_unknown_language_and_invalid_query_with_exit_two ... ok
test query_predicates_filter_matches ... ok
test query_emits_flat_jsonl_for_plain_and_alternating_patterns ... ok
test query_bad_digest_exits_two_with_one_line_stderr ... ok
test query_with_digest_reads_the_staged_blob ... ok

test result: ok. 8 passed; 0 failed

golden_parity: 8 passed; snapshot: 2 passed; doc-tests: 0
```

md probe:

```
$ printf '# Title\n\nA paragraph.\n\n```js\ncode\n```\n' > /tmp/md_probe.md
$ ./target/release/extract query --lang md --query '(atx_heading) @heading' /tmp/md_probe.md
{"end_line":2,"heading":"# Title\n","line":1}
```

html probe:

```
$ printf '<div class="x"><p>hi</p></div>' > /tmp/html_probe.html
$ ./target/release/extract query --lang html --query '(element (start_tag (tag_name) @tag))' /tmp/html_probe.html
{"end_line":1,"line":1,"tag":"div"}
{"end_line":1,"line":1,"tag":"p"}
```

md_inline probe (design-constraint break: exposed only because it drops in
with no structural change):

```
$ printf 'This is *em* and [link](url).' > /tmp/md_inline_probe.md
$ ./target/release/extract query --lang md_inline --query '[(emphasis) @em (shortcut_link) @lnk (inline_link) @lnk]' /tmp/md_inline_probe.md
{"em":"*em*","end_line":1,"line":1}
{"end_line":1,"line":1,"lnk":"[link](url)"}
```

prolog suite:

```
% End unit fact_seeding: passed (0.011 sec CPU)
% All 345 (+44 sub-tests) tests passed in 0.956 seconds (0.921 cpu)
```

## Design-constraint notes

- `md` = the BLOCK grammar (`atx_heading`, `fenced_code_block`, `paragraph`,
  `list`), matching v5's choice.
- `md_inline` exposed: `INLINE_LANGUAGE` is a first-class `LanguageFn` the
  executor parses as its own single tree, so it required no structural change
  to `0_query.rs`. No two-pass merged tree was built.
- Program-layer whitelist (`0_program_check.pl:245`) takes `md`, `html` but
  not `md_inline`: `md_inline` is a query-only language, not a `.dl` program
  language.

## Deviations

The original brief assigned prolog edits to `v6/prolog/0_ast_expand.pl` and
the plunit file; those hold no language list, so no edit there. The real
whitelist is `v6/prolog/0_program_check.pl:245` (file outside the brief's two
sites). STOP.md recorded this; RESUME amended ownership to add that file,
which is what this report covers.
