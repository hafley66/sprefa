# Lane: rewrite extract's --help so a stranger understands it

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/helpfix && git merge --ff-only 173d308c`. On failure STOP and report. If reality deviates from this brief, STOP and report; do not improvise.

## Task
You own exactly ONE file: `v6/sprefa-extract/src/bin/extract/help.rs`. Replace the `LONG_ABOUT` and `FAMILY_LONG` string constants with the exact text below. Touch no other constant, no other file, no clap attributes, no flag names.

Constraint that must survive: the test `tests/6_document_formats.rs` (`the_cli_help_names_the_fallback_formats`) greps the rendered help, so the LANGUAGE COVERAGE section below preserves the current table's language/extension lines. Do not reword that table beyond what is written here.

## New LONG_ABOUT (replace the whole string, keep the trailing `";` form)

```
Read source files and print facts about the code as JSONL (one JSON object per
line) on stdout. No daemon, no database, no network: point it at files, get
facts, pipe them anywhere.

QUICK START
  extract src/app.ts                       every fact kind for one file
  extract --family call src/app.ts         only call-graph facts
  extract --resolve a.ts b.ts              cross-file call edges, parse-based
  extract --family scip .                  whole-project facts from the real
                                           compiler index (exact, slower)
  extract --schema                         every record shape this can emit

WHAT --family MEANS
  One flag, two jobs; the second grew out of the first.

  Job 1, fact kinds. A comma-separated subset of what to extract from each
  file, default all four:
    cst    the syntax tree
    type   type declarations and annotations
    call   calls and definitions
    df     dataflow
  `--family call,type src/app.ts` narrows one file's output.

  Job 2, whole-project modes. Two special names that change the run's shape
  instead of filtering it (mixing them with fact kinds is an error):
    scip        exact facts from the language's own compiler/indexer
    diet_scip   fast facts from parsing alone, no compiler involved
  \"diet\" names the technique (parse + name matching). It never means partial
  SCIP data.

EXACT MODE: --family scip ROOT
  Detects the project kind from marker files (Cargo.toml -> rust-analyzer,
  tsconfig.json or package.json -> scip-typescript, go.mod -> scip-go), builds
  or reuses the compiler's index, and streams it as scip_* relations: scip_def,
  scip_name, scip_ref, scip_edge, scip_fn_edge, scip_callee_type, scip_local,
  scip_impl, plus one scip_index header row. Every fact is compiler-resolved.
  An index already on disk is reused untouched; a fresh build runs once under a
  time budget (the indexer's whole process group is killed at the deadline) and
  is cached for next time.

  When a root cannot be indexed you get scip_skip rows saying exactly which
  root and why: not_installed comes with the install command, timed_out with
  the budget, failed with the indexer's own last stderr line. Exit is 0 and the
  stream continues. You never get a silently empty stream.

FAST MODE: --family diet_scip PATH...
  This binary's own parsers (tree-sitter, oxc, syn) plus name matching across
  the files you supply. Nothing to install, no index, no compiler. Emits
  resolved_edge and resolved_type_edge. The trade, stated plainly: it is wrong
  wherever a bare name is ambiguous across your files. Two files defining
  `helper` make every unqualified `helper` call unresolvable here, where a real
  index resolves it through the import. Prefer scip when the toolchain exists;
  use diet_scip when speed matters or the toolchain is missing.

CROSS-FILE RESOLUTION: --resolve PATH...
  Extract every supplied file, build one definition index, emit resolved edges.
  Needs two or more paths. Under --resolve, --family picks which edges: `call`
  (default) emits resolved_edge, `type` emits resolved_type_edge, `call,type`
  both.

  Add `--project-root DIR` plus `--scip-index FILE` (an index you already have)
  or `--scip-build` (build one first) to put a compiler index in the loop; the
  call arm then emits scip_override rows wherever the compiler disagrees with
  the name match. --project-root is what SCIP document paths are relative to,
  so both SCIP flags require it. --scip-build runs one indexer, so all supplied
  paths must be one language; ts, go and rust have indexers.

RAW INDEX FACTS: --scip-facts
  With --project-root and an index (--scip-index or --scip-build), stream the
  index itself as flat facts: scip_occurrence (symbol mentions with byte
  spans), scip_symbol, scip_relationship, and the rest. Deliberately unjoined;
  joining them into definitions/references/impls is the caller's job. Use
  --scip-record to take only the kinds you need; the full stream is large.

MODULE GRAPH: --scip-deps / --deps
  --scip-deps folds a compiler index into file_edge rows (which file depends on
  which, with a crossing-symbol count). Graded against madge over 212 real
  files: recall 0.992, precision 0.988. --deps is the parse-based sibling for
  TypeScript (imports resolved syntactically, best effort, allowed to lose to
  --scip-deps).

FILE FACT
  --file-fact prepends one `file` record per input: path, content digest, byte
  count, line count. Rides the same read; costs no second pass.

PATTERN MODE
  Repeat --ast-pattern ID=PATTERN to run ast-grep patterns over one parsed
  source root; --ast-selector ID=KIND makes a pattern contextual;
  --ast-capture ID=NAME emits a metavariable. Rows carry capture and
  whole-match byte spans. Pattern text is a CLI input, never DL syntax.

OUTPUT
  Each line is one fact tagged by `record` (run `extract --schema` for every
  shape, its fields, and the per-kind vocabularies). Spans are half-open byte
  offsets [start, end) into the file; records join across kinds by matching
  spans.

LANGUAGE COVERAGE (first-match, by extension)
  ts/tsx/mts/cts/js/jsx/mjs/cjs    full     kinds: cst, type, call, df, const
  rs                               full     kinds: cst, type, call, df, const
  go                               full     kinds: cst, type, call, df (no const facet)
  kt/kts                           full     kinds: cst, type, call, df (no const facet)
  pl/pro/prolog/datalog/horn       full     kinds: cst, type, call, df
  html/yaml/json/css               cst only (ast-grep grammar, no native front-end)
  python/java/c/cpp/cs/rb/php/sh/lua/scala/swift/ex/hs   cst only, same route
  any other extension              no output, exit 0 (not an error)

  NOT COVERED, and each costs a new grammar dependency: md, toml, xml.

  Asking for a kind a language does not emit makes that kind simply absent.
  An unrecognized language produces zero lines and exits 0.

EXIT CODES
  0  facts streamed (possibly none), or --schema/--help/--version
  1  could not read the input file (I/O or UTF-8)
```

## New FAMILY_LONG (replace the whole string)

```
Which kinds of facts to extract, comma-separated: cst, type, call, df.
Defaults to all four. Unknown names are silently ignored; `type` and `types`
are equivalent.

Under --resolve this instead picks which resolved edges to emit: `call` (the
default) and/or `type`.

Two special names are whole-project MODES, exclusive with the kinds above:
  scip       exact facts from the language's own compiler index over one ROOT
             (builds or reuses the index, then streams scip_* relations)
  diet_scip  fast parse-based facts over the supplied PATHs, no compiler;
             \"diet\" names the technique, never partial SCIP data
Mixing a mode with a kind is an error: one filters per-file output, the other
changes the whole run.
```

## Escaping note
The text above contains double quotes around the word diet; in the rust string constants they must be escaped exactly as shown (`\"diet\"`). Keep the `\` line-continuation opening (`pub const LONG_ABOUT: &str = "\`) and trailing `";` exactly as the current file has them. Preserve trailing-whitespace-free lines.

## Gates, run all, paste output
- `cargo build --release --features cli --bin extract` in v6/sprefa-extract: clean.
- `./target/release/extract --help | head -40` renders the new text.
- `cargo test --features cli` in v6/sprefa-extract: all pass (the help test must stay green).
- Diff shows exactly one file changed.

## Commit and report
Commit on lane/help-family-unfuck, message: `extract: --help speaks plain words, family defined at first use`. The pre-commit rail needs the extract binary you just built, so it should pass. Never push, no subagents. REPORT.md at worktree root: gate outputs, deviations (expected: none).
