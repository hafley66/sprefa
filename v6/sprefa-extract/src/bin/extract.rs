//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not buffer
//! the whole corpus; the lib drains). One data-driven path: `dispatch(path,
//! content, mask)` -> `flatten` -> stdout. `--family` selects the mask (default
//! ALL); `--bench` times extract + flatten and reports per-family counts to stderr;
//! `--schema` prints the JSONL output contract and exits. The bin names no
//! ast-grep/oxc type outside the `Source` impls (the uniform-surface law).

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{
    build_def_index, dispatch, flatten, query_patterns, source_for, AstPatternQuery, BlobHash,
    CallF, ExtractOutput, FamilyMask, FileSet, FlatFact, GoSource, IndexBag, KotlinSource,
    ManifestMap, ProjectCx, ProjectDigest, ProjectEdge, PrologSource, Resolve, RustSource, Source,
    TsSource,
};

/// Self-describing enough that `extract --help` + `extract --schema` are a
/// complete contract for a fresh caller (human or AI). No outside docs needed.
const LONG_ABOUT: &str = "\
Extract normalized graph facts from one source file and stream them as JSONL
(one JSON object per line) to stdout. No daemon, no database, no network.

PROJECT MODE
  `--resolve PATH...` extracts every supplied file, builds one definition index,
  then emits resolved caller-to-callee edges as JSONL. It requires two or more
  source paths when resolving across files.

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
  python/c/... (any ast-grep grammar)        cst only
  any other extension              no output, exit 0 (not an error)

  Selecting a family a language does not emit makes that family simply absent.
  An unrecognized language produces zero lines and exits 0.

EXIT CODES
  0  facts streamed (possibly none), or --schema/--help/--version
  1  could not read the input file (I/O or UTF-8)";

const PATH_LONG: &str = "\
A source file to extract. Language is inferred from the extension (see coverage
above). Output goes to stdout; with --bench, the timing summary goes to stderr
instead and no facts are printed.";

const FAMILY_LONG: &str = "\
Comma-separated subset of: cst,type,call,df. Defaults to all four. Unknown names
are silently ignored; `type` and `types` are equivalent.";

const BENCH_LONG: &str = "\
Extract + flatten, then print one summary line to stderr (per-family node counts
and total fact count) and emit nothing to stdout. Use it to check which families
a language produces without parsing JSONL.";

#[derive(Parser)]
#[command(
    name = "extract",
    version,
    about = "sprefa-extract: one source file -> flat graph facts (JSONL to stdout)",
    long_about = LONG_ABOUT,
)]
struct Cli {
    #[arg(required_unless_present = "schema", value_name = "PATH", long_help = PATH_LONG)]
    paths: Vec<PathBuf>,

    #[arg(long, value_delimiter = ',', long_help = FAMILY_LONG)]
    family: Option<Vec<String>>,

    /// Time extract + flatten and report per-family counts to stderr.
    #[arg(long, long_help = BENCH_LONG)]
    bench: bool,

    /// Resolve caller-to-callee edges across all supplied paths.
    #[arg(
        long,
        conflicts_with_all = ["bench", "ast_pattern", "ast_selector", "ast_capture"]
    )]
    resolve: bool,

    /// Ast-grep pattern in ID=PATTERN form. Repeat to batch patterns over one parse.
    #[arg(
        long = "ast-pattern",
        value_name = "ID=PATTERN",
        action = clap::ArgAction::Append,
        conflicts_with_all = ["family", "bench", "resolve"]
    )]
    ast_pattern: Vec<String>,

    /// Contextual pattern selector in ID=KIND form. Repeat at most once per query.
    #[arg(
        long = "ast-selector",
        value_name = "ID=KIND",
        action = clap::ArgAction::Append,
        requires = "ast_pattern",
        conflicts_with_all = ["family", "bench", "resolve"]
    )]
    ast_selector: Vec<String>,

    /// Single-node metavariable to emit in ID=NAME form. Repeat per query.
    #[arg(
        long = "ast-capture",
        value_name = "ID=NAME",
        action = clap::ArgAction::Append,
        requires = "ast_pattern",
        conflicts_with_all = ["family", "bench", "resolve"]
    )]
    ast_capture: Vec<String>,

    /// Print the JSONL output contract to stdout and exit (no extraction).
    #[arg(long)]
    schema: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.schema {
        print_schema();
        return Ok(());
    }

    if cli.resolve {
        resolve_project(&cli.paths)?;
        return Ok(());
    }

    if cli.paths.len() != 1 {
        return Err("exactly one PATH is required unless --resolve is given".into());
    }

    let path = &cli.paths[0];
    let content = std::fs::read(path)?;
    let path_str = path.to_string_lossy();
    if !cli.ast_pattern.is_empty() {
        let queries = parse_ast_queries(&cli.ast_pattern, &cli.ast_selector, &cli.ast_capture)?;
        stream_ast_queries(&path_str, &content, &queries)?;
        return Ok(());
    }
    let mask = cli
        .family
        .as_deref()
        .map(parse_mask)
        .unwrap_or(FamilyMask::ALL);
    if cli.bench {
        bench(&path_str, &content, mask)?;
    } else {
        stream(&path_str, &content, mask)?;
    }
    Ok(())
}

fn split_assignment<'a>(flag: &str, value: &'a str) -> Result<(&'a str, &'a str), String> {
    let Some((id, body)) = value.split_once('=') else {
        return Err(format!("{flag} expects ID=VALUE, got '{value}'"));
    };
    if id.is_empty() || body.is_empty() {
        return Err(format!(
            "{flag} expects non-empty ID and VALUE, got '{value}'"
        ));
    }
    Ok((id, body))
}

fn parse_ast_queries(
    patterns: &[String],
    selectors: &[String],
    captures: &[String],
) -> Result<Vec<AstPatternQuery>, String> {
    let mut queries = Vec::with_capacity(patterns.len());
    for spec in patterns {
        let (id, pattern) = split_assignment("--ast-pattern", spec)?;
        if queries.iter().any(|query: &AstPatternQuery| query.id == id) {
            return Err(format!("duplicate --ast-pattern id '{id}'"));
        }
        queries.push(AstPatternQuery {
            id: id.to_string(),
            pattern: pattern.to_string(),
            selector: None,
            captures: Vec::new(),
        });
    }
    for spec in selectors {
        let (id, selector) = split_assignment("--ast-selector", spec)?;
        let Some(query) = queries.iter_mut().find(|query| query.id == id) else {
            return Err(format!(
                "--ast-selector id '{id}' has no matching --ast-pattern"
            ));
        };
        if query.selector.is_some() {
            return Err(format!("duplicate --ast-selector id '{id}'"));
        }
        query.selector = Some(selector.to_string());
    }
    for spec in captures {
        let (id, capture) = split_assignment("--ast-capture", spec)?;
        let Some(query) = queries.iter_mut().find(|query| query.id == id) else {
            return Err(format!(
                "--ast-capture id '{id}' has no matching --ast-pattern"
            ));
        };
        if !query.captures.iter().any(|existing| existing == capture) {
            query.captures.push(capture.to_string());
        }
    }
    for query in &queries {
        if query.captures.is_empty() {
            return Err(format!(
                "--ast-pattern id '{}' has no --ast-capture",
                query.id
            ));
        }
    }
    Ok(queries)
}

fn stream_ast_queries(
    path: &str,
    content: &[u8],
    queries: &[AstPatternQuery],
) -> Result<(), Box<dyn std::error::Error>> {
    for fact in query_patterns(path, content, queries)? {
        println!("{}", serde_json::to_string(&fact)?);
    }
    Ok(())
}

struct ProjectInput {
    path: String,
    blob: BlobHash,
    output: ExtractOutput,
}

fn resolve_project(paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let content = std::fs::read(path)?;
        let path = path.to_string_lossy().to_string();
        if let Some(output) = dispatch(&path, &content, FamilyMask::ALL) {
            inputs.push(ProjectInput {
                blob: BlobHash::of(&content),
                path,
                output,
            });
        }
    }

    let pairs: Vec<(BlobHash, &ExtractOutput)> = inputs
        .iter()
        .map(|input| (input.blob, &input.output))
        .collect();
    let files = FileSet;
    let manifests = ManifestMap;
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh project definition index");

    let mut lines = Vec::new();
    for input in &inputs {
        for edge in resolve_call_edges(&input.path, &input.output, &cx) {
            let Some(callee) = inputs
                .iter()
                .find(|candidate| candidate.blob == edge.dst_blob)
            else {
                continue;
            };
            let caller_name = input.output.call.as_ref().and_then(|call| {
                call.node(edge.src)
                    .name
                    .map(|name| input.output.strings.lookup(name).to_string())
            });
            let callee_name = callee.output.call.as_ref().and_then(|call| {
                call.nodes
                    .iter()
                    .find(|node| node.span == edge.dst_span)
                    .and_then(|node| node.name)
                    .map(|name| callee.output.strings.lookup(name).to_string())
            });
            lines.push(serde_json::to_string(&FlatFact::ResolvedEdge {
                caller_path: input.path.clone(),
                caller_name,
                callee_path: callee.path.clone(),
                callee_name,
                caller_site_start: edge.call_site.map_or(0, |span| span.start),
                caller_site_end: edge.call_site.map_or(0, |span| span.end()),
                kind: edge.kind.as_str().to_string(),
            })?);
        }
    }
    lines.sort();
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

fn resolve_call_edges(
    path: &str,
    output: &ExtractOutput,
    cx: &ProjectCx,
) -> Vec<ProjectEdge<CallF>> {
    match source_for(path).map(Source::name) {
        Some("ts") => Resolve::<CallF>::resolve(&TsSource, output, cx),
        Some("rust") => Resolve::<CallF>::resolve(&RustSource, output, cx),
        Some("go") => Resolve::<CallF>::resolve(&GoSource, output, cx),
        Some("kotlin") => Resolve::<CallF>::resolve(&KotlinSource, output, cx),
        Some("prolog") => Resolve::<CallF>::resolve(&PrologSource, output, cx),
        _ => Vec::new(),
    }
}

fn parse_mask(families: &[String]) -> FamilyMask {
    let mut mask = FamilyMask::NONE;
    for family in families {
        match family.trim() {
            "cst" => mask.cst = true,
            "type" | "types" => mask.types = true,
            "call" => mask.call = true,
            "df" => mask.df = true,
            _ => {}
        }
    }
    mask
}

fn stream(path: &str, content: &[u8], mask: FamilyMask) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(out) = dispatch(path, content, mask) {
        for fact in flatten(&out) {
            println!("{}", serde_json::to_string(&fact)?);
        }
    }
    Ok(())
}

fn bench(path: &str, content: &[u8], mask: FamilyMask) -> Result<(), Box<dyn std::error::Error>> {
    let Some(src) = source_for(path) else {
        eprintln!("no source for {path}"); // @eprintln-ok: CLI-UX summary, not a diagnostic.
        return Ok(());
    };
    let t = Instant::now();
    let out = src.extract(path, content, mask);
    let extract = t.elapsed();
    let t = Instant::now();
    let facts = flatten(&out);
    let serial = t.elapsed();
    eprintln!(
        "{}: extract {:?} serial {:?} (cst={} type={} call={} df={} facts={})",
        src.name(),
        extract,
        serial,
        out.cst.as_ref().map_or(0, |b| b.nodes.len()),
        out.types.as_ref().map_or(0, |b| b.nodes.len()),
        out.call.as_ref().map_or(0, |b| b.nodes.len()),
        out.df.as_ref().map_or(0, |b| b.nodes.len()),
        facts.len(),
    );
    Ok(())
}

/// The JSONL contract, as one block. Keep it in sync with `wire::FlatFact` (the
/// source of truth); this mirrors it for human/AI readers without a doc-build step.
const SCHEMA: &str = "\
sprefa-extract JSONL contract: one fact per line, each a JSON object tagged by \
`record`. All spans are half-open byte offsets [start, end) into the file. \
Records join across families by matching spans.

RECORD SHAPES
  record=node   family=<cst|type|call|df>  span={start,end}   kind=<slug>   name=<string|null>
  record=edge   family=<cst|df>            kind=<slug>        from={start,end}  to={start,end}
  record=sig    family=type                owner={start,end}  owner_start=<u32>  owner_end=<u32>  slot=<param|ret>  pos=<u32>  ty=<name>
  record=param  family=df                  span={start,end}   pos=<u32>
  record=arg    family=df                  call={start,end}   pos=<i64>  arg={start,end}
  record=site   family=call                span={start,end}   callee=<name>  callee_path=<string|null>
  record=const  family=type                owner={start,end}  field=<string|null>  text=<string>  kind=<lit|template>
  record=capture  query=<id>  capture=<name>  text=<string>  start=<u32>  end=<u32>  match_start=<u32>  match_end=<u32>
  record=resolved_edge  caller_path=<string>  caller_name=<string|null>  callee_path=<string>  callee_name=<string|null>  caller_site_start=<u32>  caller_site_end=<u32>  kind=<slug>

FIELDS
  family       the graph plane: cst (concrete syntax tree), type (declarations),
               call (callables + call sites), df (intra-procedural value flow).
  span         a node location; half-open bytes.
  kind         the node/edge slug from the per-family vocabulary below.
  name         the declared identifier, when the node carries one (else null).
  owner        the span of the owning declaration (sig/const joins to its callable).
  owner_start  flat start byte of the sig owner span; retained alongside owner for text-host joins.
  owner_end    flat end byte of the sig owner span; retained alongside owner for text-host joins.
  slot         param or ret.
  pos          parameter index (0 for a return slot).
  ty           the referenced type's bare name, UNRESOLVED in phase 1.
  callee       the callee's trailing name as written (the resolution key).
  callee_path  the full qualified path when >1 segment (filled by resolution; else null).
  field        dotted path into an object const, or an enum member (else null).
  text         the resolved string value of a const.
  query        caller-supplied identity for one batched ast-grep pattern.
  capture      one requested single-node ast-grep metavariable.
  start/end    capture's half-open byte span in pattern mode.
  match_start/match_end  whole pattern match's half-open byte span.
  caller_site_start  start byte of the call site that produced a resolved edge.
  caller_site_end    end byte of the call site that produced a resolved edge.

KIND VOCABULARIES (the `kind` field)
  type node   struct enum trait class interface alias function method const
  call node   function method lambda
  df node     param let_bind var_read var_write lit call_res new member ret
              borrow binop unop loop if match block closure try break expr
              cond logic concat template
  cst node    the grammar node type as named by ast-grep / tree-sitter (open set)
  cst edge    child
  df edge     direct
  const kind  lit (cooked literal) | template (raw source slice, holes intact)
  sig slot    param | ret

PHASE-1 LIMITS (default mode)
  No name resolution: type edges, caller->callee links, and cross-file joins are
  NOT emitted. `site` records carry the callee name as written; `sig` records
  carry the referenced type's bare name. `--resolve PATH...` emits resolved
  caller->callee `resolved_edge` records.";

fn print_schema() {
    println!("{SCHEMA}");
}
