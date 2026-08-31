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
  Under --resolve, --family picks which edges: `call` (default) emits
  resolved_edge, `type` emits resolved_type_edge, `call,type` both.

  The paths ARE the resolution universe: a name that resolves outside them is
  not emitted. One path is a legal universe and resolves that file's edges into
  itself. Pass files, never directories; expand a tree with a glob or find.

  The stream carries phase-2 records only (resolved_edge, resolved_type_edge,
  flow_edge), never the per-file phase-1 records `--schema` also lists (node,
  edge, sig, site, specifier, unresolved and the rest). Those carry no path
  field, so they cannot name their file in a multi-file stream; get them one
  file at a time from a plain `extract FILE`.

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
  md/markdown                      cst only (tree-sitter-md block + inline grammars)
  json/jsonl/ndjson/yaml/yml/toml  data     kinds: data (+ cst where ast-grep has
                                            the grammar: json, yaml)
  html/css                         cst only (ast-grep grammar, no native front-end)
  python/java/c/cpp/cs/rb/php/sh/lua/scala/swift/ex/hs   cst only, same route
  any other extension              no output, exit 0 (not an error)

  NOT COVERED, and it costs a new grammar dependency: xml.

  Asking for a kind a language does not emit makes that kind simply absent.
  An unrecognized language produces zero lines and exits 0.

EXIT CODES
  0  facts streamed (possibly none), or --schema/--help/--version
  1  could not read the input file (I/O or UTF-8)";

pub const PATH_LONG: &str = "\
A source file to extract. Language is inferred from the extension (see coverage
above). Output goes to stdout; with --bench, the timing summary goes to stderr
instead and no facts are printed.";

pub const FAMILY_LONG: &str = "\
Which kinds of facts to extract, comma-separated: cst, type, call, df, data, cfg.
Defaults to every kind. An unknown name is a named error, never a skip;
`type` and `types` are equivalent. `cfg` (intra-procedural control flow) is
derived from the cst parse, so naming it turns cst on; rust, go, ts and kotlin
have the kind_role rows it needs and every other language emits no cfg rows.
`data` is the json/jsonl/yaml/toml plane: one `data_doc` per document carrying it
as a json value, plus one span-carrying `data_value` per value inside it.

Under --resolve this instead picks which resolved edges to emit: `call` (the
default), `type` and/or `flow`, the inter-procedural value-flow join over the
resolved call edges.

Two special names are whole-project MODES, exclusive with the kinds above:
  scip       exact facts from the language's own compiler index over one ROOT
             (builds or reuses the index, then streams scip_* relations)
  diet_scip  fast parse-based facts over the supplied PATHs, no compiler;
             \"diet\" names the technique, never partial SCIP data
Mixing a mode with a kind is an error: one filters per-file output, the other
changes the whole run.";

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

pub const RUST_CHECKER_LONG: &str = "\
Load --project-root's cargo workspace into rust-analyzer (from cargo metadata, no
cargo build) and let it name the destination of every call and type reference the
parse found. The caller, the site spans and the unresolved rows stay this
binary's; only the destination changes hands.

This is the RUST tier; --ts-checker is its ts twin. go stays on the syntax leg by
design: its call and type shapes are name-matchable, rust's are not (a method
call's destination needs the receiver's inferred type and the trait solver).

Cost is index-build class, not per-run: the workspace load is seconds and holds
the whole crate graph in memory for the run. A root with no Cargo.toml, or a
cargo metadata that fails, logs one line and falls back to the syntax leg.

Needs a binary built with --features rust-checker; without it the flag logs the
same fallback line and changes nothing.";

pub const TS_CHECKER_LONG: &str = "\
Load --project-root as one ts.Program (the project's own tsconfig options, emit
cleared) and let the TypeScript compiler name the destination of every call and
type reference the parse found. The caller, the site spans and the unresolved
rows stay this binary's; only the destination changes hands.

The compiler is the project's own: `typescript` resolved from --project-root,
else a TypeScript checkout's built lib/typescript.js. It runs as one `node`
subprocess, once per resolve, in its own process group under the same wall cap
every scip indexer runs under (SPREFA_SCIP_TIMEOUT_SECS, default 600s).

An answer the checker gives OUTSIDE the corpus suppresses the name-match leg for
that site: the compiler knowing the destination is a dependency is knowledge, and
a name match may not invent a corpus edge over it.

Needs a binary built with --features ts-checker, and node on PATH; without
either the flag logs one fallback line and changes nothing.";

pub const SCIP_FACTS_LONG: &str = "\
Load a SCIP index (--scip-index or --scip-build) and stream EVERYTHING it
carries as flat facts: scip_metadata, scip_document, scip_occurrence (byte
spans, all seven role bits, syntax kind, enclosing range), scip_occurrence_doc,
scip_diagnostic, scip_symbol, scip_relationship, scip_documentation,
scip_signature and scip_signature_occurrence. No resolve arm runs and no source
file is parsed beyond reading its bytes for the line/col to byte conversion.

FULL PASSTHROUGH IS NOT FREE. Over v6/tsv2 (204 indexed TypeScript documents)
the stream is 177,967 rows and 59.4MB, of which scip_occurrence alone is 123,655
rows and 48.5MB. Use --scip-record to take only the kinds a program demands.

The rows are deliberately unjoined. Filtering and joining them is what produces
the distinctions a caller wants (a definition is an occurrence with definition
true, a reference is one without, a local is a `local `-prefixed symbol), and
those machines belong above this binary.

PATH... under this flag only selects the indexer for --scip-build; the facts
cover every document in the index either way.

--project-root must be inside a Git worktree: the index's bytes are read
through a source tree, so a scratch copy of a corpus dies on
`Read(\".\", \". is not inside a Git worktree\")`.";

pub const SCIP_DEPS_LONG: &str = "\
Fold a SCIP index into file_edge rows: src_path holds a reference to a symbol
defined in dst_path, and symbols counts how many distinct symbols cross. This is
v6's module graph without a module resolver: the indexer already resolved every
reference, so the graph falls out of the index.

Graded against madge over v6/tsv2: 755 of madge's 761 edges agree, recall 0.992,
precision 0.988. The 6 madge-only edges are files the corpus tsconfig excludes,
which the indexer therefore never saw; the 9 scip-only edges are inferred type
references with no import statement, which a syntactic import scanner cannot
see.

Unlike --scip-facts this needs no file content: both ends of the join are in the
index.";

pub const SCIP_RECORD_LONG: &str = "\
Comma-separated list of SCIP record kinds --scip-facts should produce. The
default is every kind (full passthrough); this is the demand-side lever for its
cost, and it filters before the rows are built rather than at the wire, so a
narrowed stream also skips reading the corpus when no occurrence-side kind is
asked for.

An unknown kind is an error, never a silent drop: a typo would otherwise print
an empty stream that reads as 'this index has no such facts'.";

pub const OCCURRENCE_TEXT_LONG: &str = "\
Ask each scip_occurrence row to also carry `text`: the source slice at its byte
span, lossy-utf8, sliced from the same corpus bytes the row is already read
against. This is the v6 answer to v5's scip_binding `local_name` (an alias or
default import binds its local spelling, which the canonical-only symbol drops).

Off by default so a plain --scip-facts run stays byte-identical. A span past the
file's end drops the field rather than failing, and the field is JSON-absent
(not null) when it cannot be produced.";

pub const DEPS_LONG: &str = "\
DIET module resolution: parse the supplied TypeScript files, read their import
and export-from specifiers, and resolve each one to a file path syntactically.
Emits the same file_edge rows as --scip-deps, so the module graph is one
relation regardless of which resolver filled it, with symbols counting the
distinct bound names crossing the edge.

No indexer subprocess and no type checker. The supplied paths are BOTH the
corpus and the resolution universe: a specifier resolving outside them yields no
edge. Rules applied, in order: exact relative path; the NodeNext emitted-name
rewrite (./x.js -> ./x.ts); extension inference; directory index files; then
tsconfig paths and baseUrl. A bare specifier no tsconfig rule claims stops at
the node_modules boundary, which is a stated stop and not a failure.

Every specifier that resolves to nothing also emits a file_unresolved row naming
the policy that stopped it, so a package import, an absolute path and a broken
relative path stay three different answers instead of one silence.

BEST EFFORT, and allowed to lose to --scip-deps. tools/1_madge_oracle.sh diet
is where the loss is measured.";

pub const PACKAGE_DEPS_LONG: &str =
    "Workspace-internal PACKAGE edges: read the supplied manifests and emit one
package_edge row per manifest-to-manifest dependency, keyed on project-relative
manifest paths rather than package names, so the rows join file_edge directly.

One arm per manifest kind. Cargo.toml: dependencies / dev-dependencies /
build-dependencies as kind normal / dev / build, honouring the `package = \"..\"`
rename. package.json: dependencies / devDependencies / peerDependencies as
normal / dev / peer. go.mod: require as kind require, and replace as kind
replace, whether it names a module path or a directory (a directory is joined
lexically against the manifest's own and must hold a supplied go.mod).

A dependency is an edge ONLY when another supplied manifest declares that name:
everything else is a registry package with no manifest in the corpus. The
supplied path list is the whole universe, and a path that is not a manifest is
skipped rather than an error.";

pub const FILE_FACT_LONG: &str = "\
Prepend one `file` record carrying the path, the content digest every resolved
edge is keyed on, the byte count and the line count. Off by default so existing
output is unchanged; on, it rides the same invocation, so counting lines never
costs a second read of the file.";

pub const MAX_BYTES_LONG: &str = "\
Byte ceiling for ONE input file. Over it, the file is not parsed: `extract`
emits one `size_skip` record naming the path, the byte count and this ceiling,
and exits 0. Default 16777216; 0 removes the ceiling entirely.

A silent timeout is a defect and a named skip is a fact. Killing a run that has
outgrown its budget leaves rc=124 and an empty stream, which a caller cannot
tell from a file that legitimately has no facts, and which names neither the
file nor its size.

The number is measured, not chosen. 16777216 skips exactly one file in a
77,472-file rust registry corpus, a 29,328,358 B machine-generated parser table
that costs 12.55 s and 3.0 GB peak RSS, and all of that is parse time: --bench
charges 4.5 s to 7.0 s per family to the parse against 4 ms to 225 ms of row
flattening. No ts/js corpus file and no fixture in this crate reaches it.

The decision is made on file size before any parse, so it covers the normal
family stream, --bench and --ast-pattern alike. --file-fact still prepends its
identity row: a digest and a line count over bytes already read is not the cost
being bounded. A whole-project mode (--resolve, --deps, --scip-*) takes
directories and path sets and is not covered.";

pub const SCIP_CACHE_LONG: &str = "\
Where `--family scip` places a freshly built index and finds it again. The
default is v5's location, <ROOT>/.dl/.state/index.scip, and a `.dl/.gitignore`
gains a `.state/` line so a turnkey build never leaves a committable index blob
in a worktree.

Point it elsewhere when the root must not be written to at all: a committed
fixture, a read-only checkout, or a run whose cache should not outlive it. The
reuse probe still checks <ROOT>/index.scip and <ROOT>/.dl/index.scip either way,
and $SPREFA_SCIP_INDEX still overrides everything.";

pub const SCIP_TIMEOUT_LONG: &str = "\
Wall budget in seconds for ONE indexer run under `--family scip`. Overrides
$SPREFA_SCIP_TIMEOUT_SECS; the default is 600.

On the deadline the indexer's WHOLE PROCESS GROUP is killed, not just the
process that was spawned. That is the part that matters: these indexers fork
(rust-analyzer runs cargo metadata, scip-typescript runs the TypeScript
compiler), so a bound reaching only the direct child would leave the real work
running and reparented. A run that exceeds the budget emits a scip_skip row with
reason timed_out and the stream continues.";

pub const BENCH_LONG: &str = "\
Extract + flatten, then print one summary line to stderr (per-family node counts
and total fact count) and emit nothing to stdout. Use it to check which families
a language produces without parsing JSONL.";
