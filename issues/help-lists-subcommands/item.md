---
created: 2026-09-10
updated: 2026-09-10
type: improvement
status: open
priority: normal
labels:
- size:small
---

# extract --help lists none of the four intercepted subcommands

_Source: v6/sprefa-extract/src/bin/extract/help.rs_

## Description

`extract --help` documents the fact-extraction modes and the two aliases `fast`
and `slow`. It does not mention `rename`, `query`, `region`, or `watch`. Only
`move` is reachable from the help text, and only because `extract move --help`
works once you already know the word.

Those four are intercepted by an `argv[1]` string match in `run()`
(`v6/sprefa-extract/src/bin/extract.rs:578-615`) rather than declared as clap
subcommands, so clap never lists them.

## What a caller misses

| subcommand | what it does | how you would find it today |
|---|---|---|
| `rename FILE#OLD NEW` | rename a symbol, respell every bound occurrence | read `src/bin/extract.rs` |
| `query --lang L --query TSQ FILE` | raw tree-sitter query, one row per match, and the only surface that emits **line numbers** rather than byte spans | read the source |
| `watch ROOT` | re-extract on change into a receipt SQLite | read the source |
| `region TARGET ID` | replace a marked region in a DL7 file | read the source |

`query` is the sharpest loss. Every other output is byte spans, so a caller who
wants a line number either writes a span-to-line converter or does not know that
`query` already answers it.

## Suggested behaviour

Add a `SUBCOMMANDS` block to the help prose in
`v6/sprefa-extract/src/bin/extract/help.rs`, one line each, in the same style as
the existing `Aliases:` block at the end:

```
Subcommands:
  extract move OLD NEW         move a file and repair every specifier that named it
  extract rename FILE#OLD NEW  rename a symbol and respell every bound occurrence
  extract query --lang L ...   raw tree-sitter query with line numbers
  extract watch ROOT           re-extract on change into a receipt database
  extract region TARGET ID     replace a marked region in a generated file
```

Declaring them as real clap subcommands would also work and would get the listing
for free, at the cost of touching the dispatch. The help-prose version is the
smaller change and gets the discovery benefit.
