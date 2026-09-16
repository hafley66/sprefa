# Citations both ways

[Example](#example) · [The rail](#the-rail) · [Where the rows would come from](#where-the-rows-would-come-from) · [Receipts](#receipts)

Not built. Two checks: every `path:line` a book page cites names a line that exists, and every `/// # Book` section in the Rust source names a page that exists.

## Example

Four seeded relations stand in for rows an extractor would write: one source line, two citations into that file, and two doc tags.

```dl7
{{#include ../probes/16_citation_rail.dl7}}
```

```console
$ bash book/show.sh eval book/src/probes/16_citation_rail.dl7 | grep '^(DanglingCite \|^(UnpagedTag \|^exit'
(DanglingCite "hosting/0_the_seam.md" "src/_9_runtime/_2_reconcile.rs" 9999)
(UnpagedTag "src/_9_runtime/_3_executors/mod.rs" 44 "hosting/9_missing.md")
exit 0
```

```
step 0  book_cite (0_the_seam.md, _2_reconcile.rs, 21)    source_line holds       -> no row
step 1  book_cite (0_the_seam.md, _2_reconcile.rs, 9999)  source_line absent      -> DanglingCite
step 2  doc_tag (_2_reconcile.rs, 20, book, 0_the_seam.md)  book_page holds       -> no row
step 3  doc_tag (mod.rs, 44, book, 9_missing.md)            book_page absent      -> UnpagedTag
```

Steady state after step 3: both negations read relations with no rules, so one stratum settles them.

## The rail

The same two rules as streams. Each input is a stream of row sets, the closure of that relation at each tick.

```ts
import { combineLatest, type Observable } from "rxjs";
import { map } from "rxjs/operators";

type SourceLine = { path: string; line: number };
type BookCite = { page: string; path: string; line: number };
type DocTag = { path: string; line: number; tag: string; argument: string };

const key = (path: string, line: number) => `${path}:${line}`;

// (<- (DanglingCite ?Page ?Path ?Line) (book_cite ?Page ?Path ?Line) (not (source_line ?Path ?Line)))
export const danglingCite = (
  cites: Observable<readonly BookCite[]>,
  lines: Observable<readonly SourceLine[]>,
): Observable<readonly BookCite[]> =>
  combineLatest([cites, lines]).pipe(
    map(([cited, present]) => {
      const known = new Set(present.map((row) => key(row.path, row.line)));
      return cited.filter((row) => !known.has(key(row.path, row.line)));
    }),
  );

// (<- (UnpagedTag ?Path ?Line ?Page) (doc_tag ?Path ?Line "book" ?Page) (not (book_page ?Page)))
export const unpagedTag = (
  tags: Observable<readonly DocTag[]>,
  pages: Observable<readonly string[]>,
): Observable<readonly DocTag[]> =>
  combineLatest([tags, pages]).pipe(
    map(([tagged, existing]) => {
      const known = new Set(existing);
      return tagged.filter((row) => row.tag === "book" && !known.has(row.argument));
    }),
  );
```

## Where the rows would come from

| relation | would come from | exists | gap |
|---|---|---|---|
| `source_line` | a file's line count per path | `extract` rows carry spans (`hafley-rs/crates/sprefa-extract/src/types.rs:357-364` `DocFact.owner: Span`) | no relation of line counts; `extract` payload is one text term (`README.md:128`) |
| `book_cite` | inline code spans shaped `path:line` in `book/src/**/*.md` | markdown rows: `Heading`, `CodeBlock`, `Link`, `Image` (`types.rs:388-393`) | no inline code kind |
| `doc_tag` | `/// # Book` sections | `push_doc` writes a `DocFact` per documented item (`hafley-rs/crates/sprefa-extract/src/lang/rust_docs.rs:75-95`); a `# Heading` line becomes `DocTag { tag: section, arg: Heading }` (`rust_docs.rs:121-137`) | the heading is `section`/`Book`, the page name is the section body text; no `book` tag word |
| `book_page` | `SUMMARY.md` links | markdown `Link` rows (`types.rs:391`) | none beyond the payload projection |

Lineage: v3 (`~/projects/sprefa-archive-20260701/v3`) has no doc-comment source; v5 ran `examples/doc-coverage.dl` over `doc_comment` rows (`src/graph/typegraph/rust/mod.rs:603-618` at the repository root); `sprefa-extract` ports it (`rust_docs.rs:1-2`).

## Receipts

| claim | path | command |
|---|---|---|
| the rail sketch compiles and derives both rows | `book/src/probes/16_citation_rail.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
| negation settles over relations with no rules | [Negation and strata](../4_negation.md) | `bash book/show.sh eval fixtures/term_lt/1_top.dl7` |
| the extract CST survey lists `DocFact` | `plans/v8/2026-09-15-extract-cst-astgrep-survey.md:81`, uncommitted at the main checkout | none on this base |
| v3 has no doc-comment rows | `~/projects/sprefa-archive-20260701/v3` | `grep -rln doc_comment ~/projects/sprefa-archive-20260701/v3 --include=*.rs` |
