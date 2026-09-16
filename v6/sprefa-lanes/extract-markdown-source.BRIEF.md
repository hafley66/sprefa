# Lane: add a markdown Source to sprefa-extract

## Base
`git merge --ff-only e70417d9` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/feature/extract-markdown-source`.

## Why this exists

`CLAUDE.md` cites code as `` `path:line` ``. Those citations rot: a file is
renamed, a block moves, the doc keeps asserting the old address. A rail must
fail when a citation stops resolving.

That rail cannot be written in `.dl6` today, and the reason is one function.
`v6/sprefa-extract/src/lang/mod.rs:36-45`:

```rust
pub fn sources() -> &'static [&'static dyn Source] {
    &[&RustSource, &GoSource, &KotlinSource, &PrologSource, &TsSource, &AstgrepSource]
}
```

No markdown. `source_for` returns `None` for `.md`, so nothing extracts facts
from a markdown file, so no `.dl` or `.dl6` program can query one.

A Python fallback exists at `v6/tools/check-md-citations.py`. It is a stopgap.
Your job makes the real rail possible.

## The contract you implement

`v6/sprefa-extract/src/types.rs:1376`:

```rust
pub trait Source: Sync + Send {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput;
}
```

Families available, `types.rs:95`:

```rust
pub enum FamilyTag { Df, Call, Type, Module, Cst }
```

A markdown source emits `Cst` and nothing else. Markdown has no dataflow, no
calls, and no types. Do NOT invent a family. If you believe markdown needs one,
that is a design question and it goes to the user as a cited fork.

## Roster order matters, and there is a trap already documented

`lang/mod.rs:28-34` records why: `"x.kts".ends_with(".ts")` is true, so
`KotlinSource` must precede `TsSource`. Apply the same care. Check whether any
existing `matches()` already claims `.md` or `.markdown` before you insert, and
say where you inserted and why.

`AstgrepSource` is the cst-only fallback and is LAST. Determine whether ast-grep
already has a markdown grammar. If it does, adding `.md` to the fallback may be
most of the work, and that is a legitimate finding, so measure it before writing
a new parser.

## Build-vs-buy, mandatory, no one-line dismissals

Do NOT write a markdown parser. Research and present a written
candidate-by-candidate table before any code:

| candidate | what to record |
|---|---|
| `pulldown-cmark` | maintenance date, downloads, CommonMark conformance, span/byte-offset support |
| `comrak` | same, plus GFM tables and footnotes |
| `markdown-rs` | same |
| `tree-sitter-md` | same, and whether it fits the ast-grep path already in the crate |
| ast-grep's existing grammar set | whether markdown is already there |

Spans are the hard requirement. `SpanOut` at `types.rs:1388-1393` is
inclusive-exclusive BYTE offsets. A parser that gives you line numbers but not
byte offsets fails the contract. Check this FIRST; it eliminates candidates fast.

Look at what the other sources already pull in before adding a dependency. The
crate is 17,126 lines across five extractors; match its habits.

## What the Cst nodes should carry

Enough that a rail can find a citation and check it. At minimum:

| node | why |
|---|---|
| inline code span | `` `path:line` `` lives in one |
| fenced code block, with its info string | commands live in these |
| heading, with level | doc structure, TOC rails |
| link, with destination | dead-link rails |

Name them with the vocabulary the other sources use. Read `rust.rs` or `go.rs`
for the naming habit before you choose.

## Scope boundary

Markdown ONLY. The user's stated sequence is markdown now, then JSON (which
covers YAML and TOML the way v3/v4/v5 did), then XML and HTML. Do NOT start
those. Note in your plan doc what a JSON source would reuse from yours.

## Gates
```
cargo build --workspace
cargo test --no-fail-fast
```
Three runs each. Add tests: a fixture `.md` in, expected Cst nodes out, spans
asserted as byte offsets. A test that only checks "it did not crash" is not a
test.

`just green-all` is RED by design; `.github/CI-KNOWN-RED.md` is the allowlist
and it goes stale. Do not chase anything listed there, and do not add rows to it.

## Files you own
`v6/sprefa-extract/**`, plan doc `plans/2026-08-12-extract-markdown-source.md`.

## Files you must NOT touch
`v6/prolog/**`, `v6/sprefa-engine-rs/**`, `v6/boop/**`, `v6/labs/exec_shootout/**`,
`v6/justfile`, `CLAUDE.md`, `v6/tools/check-md-citations.py`. Other lanes own
those.

## COMMIT YOUR WORK, THIS IS THE MOST COMMON FAILURE

Eight lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING, and five of those eight were flash4. The work was on disk and
invisible.

Before you exit:
```bash
git add -A
git commit -m "<subject>"
git log --oneline -1        # confirm YOUR commit is HEAD
```

If `git log --oneline -1` does not show your commit, you have not delivered.

## Laws
- Build-vs-buy: library research and a written candidate table BEFORE any code.
- Doubt yourself before asserting. Cite what you read.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No `eprintln!` in `src/**`, `tracing` only.
- The 10-second law: any single operation over 10s is a defect.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Report
The candidate table with the span-support column, which crate you chose and the
number that decided it, where in the roster you inserted and why, the node kinds
you emit, and the test counts.
