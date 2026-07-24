//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not buffer
//! the whole corpus; the lib drains). One data-driven path: `dispatch(path,
//! content, mask)` -> `flatten` -> stdout. `--family` selects the mask (default
//! ALL); `--bench` times extract + flatten and reports per-family counts to stderr;
//! `--schema` prints the JSONL output contract and exits. The bin names no
//! ast-grep/oxc type outside the `Source` impls (the uniform-surface law).

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{dispatch, flatten, source_for, FamilyMask};

/// Self-describing enough that `extract --help` + `extract --schema` are a
/// complete contract for a fresh caller (human or AI). No outside docs needed.
const LONG_ABOUT: &str = "\
Extract normalized graph facts from one source file and stream them as JSONL
(one JSON object per line) to stdout. No daemon, no database, no network.

OUTPUT
  Each line is one fact tagged by `record` (run `extract --schema` for every
  shape, its fields, and the per-family `kind` vocabularies). Spans are
  half-open byte offsets [start, end) into the file; records join across
  families by matching spans.

LANGUAGE COVERAGE (first-match, by extension)
  ts/tsx/mts/cts/js/jsx/mjs/cjs    full     families: cst, type, call, df, const
  rs                               full     families: cst, type, call, df, const
  go                               full     families: cst, type, call, df (no const facet)
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
    path: Option<PathBuf>,

    #[arg(long, value_delimiter = ',', long_help = FAMILY_LONG)]
    family: Option<Vec<String>>,

    /// Time extract + flatten and report per-family counts to stderr.
    #[arg(long, long_help = BENCH_LONG)]
    bench: bool,

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

    // clap enforces PATH unless --schema is present, so this is always Some here.
    let path = cli.path.expect("PATH is required unless --schema is given");
    let content = std::fs::read(&path)?;
    let path_str = path.to_string_lossy();
    let mask = cli.family.as_deref().map(parse_mask).unwrap_or(FamilyMask::ALL);
    if cli.bench {
        bench(&path_str, &content, mask)?;
    } else {
        stream(&path_str, &content, mask)?;
    }
    Ok(())
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
  record=sig    family=type                owner={start,end}  slot=<param|ret>  pos=<u32>  ty=<name>
  record=site   family=call                span={start,end}   callee=<name>  callee_path=<string|null>
  record=const  family=type                owner={start,end}  field=<string|null>  text=<string>  kind=<lit|template>

FIELDS
  family       the graph plane: cst (concrete syntax tree), type (declarations),
               call (callables + call sites), df (intra-procedural value flow).
  span         a node location; half-open bytes.
  kind         the node/edge slug from the per-family vocabulary below.
  name         the declared identifier, when the node carries one (else null).
  owner        the span of the owning declaration (sig/const joins to its callable).
  slot         param or ret.
  pos          parameter index (0 for a return slot).
  ty           the referenced type's bare name, UNRESOLVED in phase 1.
  callee       the callee's trailing name as written (the resolution key).
  callee_path  the full qualified path when >1 segment (filled by resolution; else null).
  field        dotted path into an object const, or an enum member (else null).
  text         the resolved string value of a const.

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

PHASE-1 LIMITS (by design)
  No name resolution: type edges, caller->callee links, and cross-file joins are
  NOT emitted. `site` records carry the callee name as written; `sig` records
  carry the referenced type's bare name. Resolution is a later layer.";

fn print_schema() {
    println!("{SCHEMA}");
}
