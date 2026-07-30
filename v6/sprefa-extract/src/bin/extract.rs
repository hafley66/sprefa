//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not buffer
//! the whole corpus; the lib drains). One data-driven path: `dispatch(path,
//! content, mask)` -> `flatten` -> stdout. `--family` selects the mask (default
//! ALL); `--bench` times extract + flatten and reports per-family counts to stderr;
//! `--schema` prints the JSONL output contract and exits. The bin names no
//! ast-grep/oxc type outside the `Source` impls (the uniform-surface law).
//!
//! THE BIN OWNS NO EXTRACTION LOGIC. Argument parsing, one library call, print.
//! Phase 2 used to be assembled here, in a private adapter that reached only the
//! `CallF` arm with no SCIP, and nothing asserted the difference against what the
//! library could already do. The recipe now lives in `sprefa_extract::project`
//! and `tests/4_capability_parity.rs` asserts the binary reaches every library
//! capability, so that drift cannot recur silently.

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{
    dispatch, file_fact, flatten, query_patterns, resolve_project_jsonl, scip_facts_jsonl,
    scip_file_edges_jsonl, source_for, AstPatternQuery, FamilyMask, ResolveArms, ResolveRequest,
    ScipMode, SCHEMA,
};

#[path = "extract/help.rs"]
mod help;

use help::{
    BENCH_LONG, FAMILY_LONG, FILE_FACT_LONG, LONG_ABOUT, PATH_LONG, PROJECT_ROOT_LONG,
    SCIP_BUILD_LONG, SCIP_DEPS_LONG, SCIP_FACTS_LONG, SCIP_INDEX_LONG,
};

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

    /// Resolve cross-file edges across all supplied paths (see --family).
    #[arg(
        long,
        conflicts_with_all = ["bench", "ast_pattern", "ast_selector", "ast_capture"]
    )]
    resolve: bool,

    /// Root that SCIP document paths are relative to; also the --scip-build root.
    #[arg(long, value_name = "DIR", long_help = PROJECT_ROOT_LONG)]
    project_root: Option<PathBuf>,

    /// Load a prebuilt index.scip into the resolve context.
    #[arg(
        long,
        value_name = "FILE",
        requires = "project_root",
        conflicts_with = "scip_build",
        long_help = SCIP_INDEX_LONG,
    )]
    scip_index: Option<PathBuf>,

    /// Build the index with the language's own indexer, then load it.
    #[arg(
        long,
        requires_all = ["project_root"],
        long_help = SCIP_BUILD_LONG,
    )]
    scip_build: bool,

    /// Stream file-to-file dependency edges folded from a SCIP index.
    #[arg(
        long,
        requires = "project_root",
        conflicts_with_all = ["bench", "ast_pattern", "resolve", "scip_facts", "file_fact"],
        long_help = SCIP_DEPS_LONG,
    )]
    scip_deps: bool,

    /// Stream the raw SCIP index as facts: occurrences, symbols, relationships.
    #[arg(
        long,
        requires = "project_root",
        conflicts_with_all = ["bench", "ast_pattern", "resolve"],
        long_help = SCIP_FACTS_LONG,
    )]
    scip_facts: bool,

    /// Prepend one `file` record: path, content digest, byte count, line count.
    #[arg(long, conflicts_with_all = ["resolve", "scip_facts", "ast_pattern"], long_help = FILE_FACT_LONG)]
    file_fact: bool,

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
        stream_resolve(&cli)?;
        return Ok(());
    }

    if cli.scip_deps {
        for line in scip_file_edges_jsonl(&scip_request(&cli))? {
            println!("{line}");
        }
        return Ok(());
    }

    if cli.scip_facts {
        for line in scip_facts_jsonl(&scip_request(&cli))? {
            println!("{line}");
        }
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
    // The file row rides the SAME read as extraction: counting lines must never
    // cost a second pass over the file, let alone a second subprocess.
    if cli.file_fact {
        println!(
            "{}",
            serde_json::to_string(&file_fact(&path_str, &content))?
        );
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

/// The SCIP-mode half of the CLI's flags, shared by `--resolve` and
/// `--scip-facts`.
fn scip_request(cli: &Cli) -> ResolveRequest<'_> {
    ResolveRequest {
        paths: &cli.paths,
        arms: ResolveArms::default(),
        scip: match (&cli.scip_index, cli.scip_build) {
            (Some(path), _) => ScipMode::Load(path),
            (None, true) => ScipMode::Build,
            (None, false) => ScipMode::Off,
        },
        project_root: cli.project_root.as_deref(),
    }
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

/// Project mode: translate flags to a `ResolveRequest`, call the library, print.
/// Every decision below is argument shaping; the recipe itself is
/// `sprefa_extract::project`.
fn stream_resolve(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Under --resolve, --family names the phase-2 arms. Absent, the default is
    // `call` alone, which keeps pre-existing --resolve output byte-identical.
    let arms = match cli.family.as_deref() {
        None => ResolveArms {
            call: true,
            types: false,
        },
        Some(families) => parse_arms(families)?,
    };
    let request = ResolveRequest {
        arms,
        ..scip_request(cli)
    };
    for line in resolve_project_jsonl(&request)? {
        println!("{line}");
    }
    Ok(())
}

/// `--family` under `--resolve`. Unlike the phase-1 mask, an unknown name here
/// is an ERROR rather than a silent drop: the phase-1 mask can afford to ignore
/// noise because every family it does know still runs, while a typo'd resolve
/// arm would silently produce an empty stream that reads as "nothing resolved".
fn parse_arms(families: &[String]) -> Result<ResolveArms, String> {
    let mut arms = ResolveArms::default();
    for family in families {
        match family.trim() {
            "call" => arms.call = true,
            "type" | "types" => arms.types = true,
            other => {
                return Err(format!(
                    "--family '{other}' is not a resolve arm; under --resolve only \
                     'call' and 'type' are meaningful"
                ))
            }
        }
    }
    if !arms.call && !arms.types {
        return Err("--family selected no resolve arm".to_string());
    }
    Ok(arms)
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

/// `--schema` prints the library's own wire contract. The text lives in
/// `sprefa_extract::wire::SCHEMA`, not here, so a library consumer can read the
/// same contract without shelling out to this binary.
fn print_schema() {
    println!("{SCHEMA}");
}
