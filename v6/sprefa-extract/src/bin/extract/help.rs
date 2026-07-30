//! The `--help` text: every long-help string clap renders.
//!
//! Split out of `extract.rs` on size. These are documentation, the parent module
//! is argument dispatch, and they change for different reasons; keeping them
//! together pushed one file past the size limit on prose alone.
//!
//! Included with `#[path]` rather than living beside a `main.rs`, so cargo's
//! bin auto-discovery does not see this directory as a second binary.

/// Self-describing enough that `extract --help` + `extract --schema` are a
/// complete contract for a fresh caller (human or AI). No outside docs needed.
pub const LONG_ABOUT: &str = "\
Extract normalized graph facts from one source file and stream them as JSONL
(one JSON object per line) to stdout. No daemon, no database, no network.

PROJECT MODE
  `--resolve PATH...` extracts every supplied file, builds one definition index,
  then emits resolved edges as JSONL. It requires two or more source paths when
  resolving across files. Under --resolve, --family selects which resolve arms
  run: `call` (the default) emits resolved_edge, `type` emits
  resolved_type_edge, `call,type` emits both.

  SCIP-BACKED RESOLUTION
  `--project-root DIR` plus either `--scip-index FILE` (an index you already
  built) or `--scip-build` (run the language's own indexer over DIR first) puts
  a SCIP index in the resolve context. The call arm then takes its SCIP leg and
  emits scip_override rows where the indexer disagrees with the name match.
  --project-root is what SCIP document paths are relative to, so it is required
  by both SCIP modes. --scip-build runs one indexer, so every supplied path must
  be the same language; ts, go and rust have indexers.

SCIP FACTS MODE
  `--scip-facts --project-root DIR` with `--scip-index FILE` or `--scip-build`
  streams the index itself as flat facts: scip_occurrence (symbol mentions with
  byte spans), scip_symbol (symbol information), scip_relationship (implements /
  type-definition / references). The rows are unjoined on purpose; filtering and
  joining them is what yields definitions, references, locals and impl edges,
  and those machines belong above this binary.

DEPENDENCY EDGES
  `--scip-deps --project-root DIR` with `--scip-index FILE` or `--scip-build`
  folds the index into file_edge rows: the module graph, with no module resolver
  in v6 at all. Graded against madge over 212 real files at recall 0.992 and
  precision 0.988; both divergence classes are corpus-definition or semantic
  reach, not resolution errors.

FILE FACT
  `--file-fact` prepends one `file` record to the normal stream: path, content
  digest, byte count, line count. It rides the same read, so line counting never
  costs a second pass or a second process.

PATTERN MODE
  Repeat `--ast-pattern ID=PATTERN` to run several ast-grep patterns over one
  parsed source root. `--ast-selector ID=KIND` makes one pattern contextual and
  selects that syntax-node kind from its context. Repeat `--ast-capture ID=NAME`
  for each single-node metavariable to emit. Output rows are flat and carry
  capture + whole-match half-open byte spans. Pattern text is a CLI contract
  input, never DL syntax.

OUTPUT
  Each line is one fact tagged by `record` (run `extract --schema` for every
  shape, its fields, and the per-family `kind` vocabularies). Spans are
  half-open byte offsets [start, end) into the file; records join across
  families by matching spans.

LANGUAGE COVERAGE (first-match, by extension)
  ts/tsx/mts/cts/js/jsx/mjs/cjs    full     families: cst, type, call, df, const
  rs                               full     families: cst, type, call, df, const
  go                               full     families: cst, type, call, df (no const facet)
  kt/kts                           full     families: cst, type, call, df (no const facet)
  pl/pro/prolog/datalog/horn       full     families: cst, type, call, df
  html/yaml/json/css               cst only (ast-grep grammar, no native front-end)
  python/java/c/cpp/cs/rb/php/sh/lua/scala/swift/ex/hs   cst only, same route
  any other extension              no output, exit 0 (not an error)

  NOT COVERED, and each costs a new grammar dependency: md, toml, xml.

  Selecting a family a language does not emit makes that family simply absent.
  An unrecognized language produces zero lines and exits 0.

EXIT CODES
  0  facts streamed (possibly none), or --schema/--help/--version
  1  could not read the input file (I/O or UTF-8)";

pub const PATH_LONG: &str = "\
A source file to extract. Language is inferred from the extension (see coverage
above). Output goes to stdout; with --bench, the timing summary goes to stderr
instead and no facts are printed.";

pub const FAMILY_LONG: &str = "\
Comma-separated subset of: cst,type,call,df. Defaults to all four. Unknown names
are silently ignored; `type` and `types` are equivalent.

Under --resolve this selects the phase-2 arms instead of the phase-1 mask: only
`call` and `type` are meaningful there, and the default is `call`.";

pub const PROJECT_ROOT_LONG: &str = "\
The directory SCIP document paths are relative to, and the root --scip-build
runs the indexer over. Required by --scip-index and --scip-build: without it
there is no reader to join SCIP documents to their content.";

pub const SCIP_INDEX_LONG: &str = "\
Path to an index.scip built earlier. The decode is indexer-agnostic, so an index
from scip-typescript, scip-go or rust-analyzer all load the same way.";

pub const SCIP_BUILD_LONG: &str = "\
Run the language's own indexer over --project-root, then load the result. One
index means one indexer, so every supplied path must be the same language;
ts uses scip-typescript, go uses scip-go, rust uses rust-analyzer. This spawns a
foreign process and is slow: prefer --scip-index when you already have one.";

pub const SCIP_FACTS_LONG: &str = "\
Load a SCIP index (--scip-index or --scip-build) and stream it as flat facts:
one scip_occurrence row per symbol mention with byte spans, one scip_symbol row
per symbol information entry, one scip_relationship row per symbol relationship.
No resolve arm runs and no source file is parsed.

The rows are deliberately unjoined. Filtering and joining them is what produces
the distinctions a caller wants (a definition is an occurrence with definition
true, a reference is one without, a local is a `local `-prefixed symbol), and
those machines belong above this binary.

PATH... under this flag only selects the indexer for --scip-build; the facts
cover every document in the index either way.";

pub const SCIP_DEPS_LONG: &str = "\
Fold a SCIP index into file_edge rows: src_path holds a reference to a symbol
defined in dst_path, and symbols counts how many distinct symbols cross. This is
v6's module graph without a module resolver: the indexer already resolved every
reference, so the graph falls out of the index.

Graded against madge over v6/tsv2 (212 TypeScript files): 746 of madge's 752
edges agree, recall 0.992, precision 0.988. The 6 madge-only edges are files the
corpus tsconfig excludes, which the indexer therefore never saw; the 9 scip-only
edges are inferred type references with no import statement, which a syntactic
import scanner cannot see.

Unlike --scip-facts this needs no file content: both ends of the join are in the
index.";

pub const FILE_FACT_LONG: &str = "\
Prepend one `file` record carrying the path, the content digest every resolved
edge is keyed on, the byte count and the line count. Off by default so existing
output is unchanged; on, it rides the same invocation, so counting lines never
costs a second read of the file.";

pub const BENCH_LONG: &str = "\
Extract + flatten, then print one summary line to stderr (per-family node counts
and total fact count) and emit nothing to stdout. Use it to check which families
a language produces without parsing JSONL.";
