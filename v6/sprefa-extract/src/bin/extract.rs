//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not buffer
//! the whole corpus; the lib drains). One data-driven path: `dispatch(path,
//! content, mask)` -> `flatten_each` -> stdout. `--family` selects the mask (default
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

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::Parser;

use sprefa_extract::schema::schema_text;
use sprefa_extract::tsi::{ingest, Mode, RunOut};
use sprefa_extract::{
    cfg_bundle, content_id_of, deps::diet_file_edges_jsonl, diet_scip_jsonl, dispatch, file_fact,
    flatten_cfg_each, flatten_each, package_edges_jsonl, query_patterns, resolve_project_jsonl,
    scip_facts_jsonl, scip_family_jsonl, scip_file_edges_jsonl, scip_index_location,
    size_skip_fact, source_for, AstPatternQuery, FamilyMask, FlatFact, IndexBudget, ResolveArms,
    ResolveRequest, ScipFamilyRequest, ScipMode, ScipRecords, DEFAULT_MAX_BYTES,
};

#[path = "extract/help.rs"]
mod help;

use help::{
    BENCH_LONG, DEPS_LONG, FAMILY_LONG, FILE_FACT_LONG, LONG_ABOUT, MAX_BYTES_LONG,
    OCCURRENCE_TEXT_LONG, PACKAGE_DEPS_LONG, PATH_LONG, PROJECT_ROOT_LONG, RUST_CHECKER_LONG,
    TS_CHECKER_LONG,
    SCIP_BUILD_LONG,
    SCIP_CACHE_LONG, SCIP_DEPS_LONG, SCIP_FACTS_LONG, SCIP_INDEX_LONG, SCIP_RECORD_LONG,
    SCIP_TIMEOUT_LONG,
};

#[path = "../0_query.rs"]
mod query;

#[path = "../0_move.rs"]
mod source_move;

#[path = "../2_move_text.rs"]
mod move_text;

#[path = "../0_rename.rs"]
mod source_rename;

#[derive(Parser)]
#[command(
    name = "extract",
    version,
    about = "sprefa-extract: one source file -> flat graph facts (JSONL to stdout)",
    long_about = LONG_ABOUT,
)]
struct Cli {
    #[arg(required_unless_present_any = ["schema", "ingest"], value_name = "PATH", long_help = PATH_LONG)]
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

    /// Answer rust call and type destinations with rust-analyzer's own
    /// resolution instead of the syntax leg's name match.
    #[arg(
        long = "rust-checker",
        requires = "project_root",
        long_help = RUST_CHECKER_LONG,
    )]
    rust_checker: bool,

    /// Answer ts call and type destinations with the TypeScript compiler's own
    /// resolution instead of the syntax leg's name match.
    #[arg(
        long = "ts-checker",
        requires = "project_root",
        long_help = TS_CHECKER_LONG,
    )]
    ts_checker: bool,

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

    /// Stream the whole SCIP index as facts, every field the protobuf carries.
    #[arg(
        long,
        requires = "project_root",
        conflicts_with_all = ["bench", "ast_pattern", "resolve"],
        long_help = SCIP_FACTS_LONG,
    )]
    scip_facts: bool,

    /// Narrow --scip-facts to a comma-separated list of record kinds.
    #[arg(
        long = "scip-record",
        value_name = "KINDS",
        requires = "scip_facts",
        long_help = SCIP_RECORD_LONG,
    )]
    scip_record: Option<String>,

    /// Also carry the source slice at each scip_occurrence span, as `text`.
    #[arg(
        long = "occurrence-text",
        requires = "scip_facts",
        long_help = OCCURRENCE_TEXT_LONG,
    )]
    occurrence_text: bool,

    /// Stream file_edge rows resolved syntactically, with no SCIP index.
    #[arg(
        long,
        requires = "project_root",
        conflicts_with_all = ["bench", "ast_pattern", "resolve", "scip_facts", "scip_deps", "file_fact"],
        long_help = DEPS_LONG,
    )]
    deps: bool,

    /// Stream package_edge rows: workspace-internal manifest-to-manifest edges.
    #[arg(
        long = "package-deps",
        requires = "project_root",
        conflicts_with_all = ["bench", "ast_pattern", "resolve", "scip_facts", "scip_deps", "deps", "file_fact"],
        long_help = PACKAGE_DEPS_LONG,
    )]
    package_deps: bool,

    /// Prepend one `file` record: path, content digest, byte count, line count.
    #[arg(long, conflicts_with_all = ["resolve", "scip_facts", "ast_pattern"], long_help = FILE_FACT_LONG)]
    file_fact: bool,

    /// Wrap the stream in the TSI envelope: protocol, one run per tier, per-row
    /// `fact` ordinals, one witness per resolver leg, coverage per family.
    #[arg(
        long,
        conflicts_with_all = [
            "bench", "ast_pattern", "deps", "package_deps",
            "scip_facts", "scip_deps", "file_fact",
        ],
    )]
    witness: bool,

    /// Read foreign TSI JSONL, validate it against the relation registry, and
    /// re-emit it canonically. Several files are read as one stream.
    #[arg(
        long,
        value_name = "PATH",
        num_args = 1..,
        conflicts_with_all = [
            "paths", "family", "bench", "resolve", "ast_pattern", "deps",
            "package_deps", "scip_facts", "scip_deps", "file_fact", "witness",
        ],
    )]
    ingest: Vec<PathBuf>,

    /// Byte ceiling for one input; over it emits `size_skip` and exits 0. 0 = none.
    #[arg(long = "max-bytes", value_name = "BYTES", long_help = MAX_BYTES_LONG)]
    max_bytes: Option<u64>,

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

    /// Where `--family scip` places and finds its index cache.
    #[arg(long, value_name = "DIR", long_help = SCIP_CACHE_LONG)]
    scip_cache: Option<PathBuf>,

    /// Wall budget in seconds for ONE indexer run under `--family scip`.
    #[arg(long, value_name = "SECS", long_help = SCIP_TIMEOUT_LONG)]
    scip_timeout: Option<u64>,

    /// Print the JSONL output contract to stdout and exit (no extraction).
    #[arg(long)]
    schema: bool,
}

/// The two `--family` names that select a whole-project MODE rather than a
/// member of the per-file extraction mask. Split out here so the mask parser
/// below stays exactly what it was for `cst,type,call,df`.
enum FamilyMode {
    /// Real SCIP index data over one root.
    Scip,
    /// The tree-sitter + heuristic resolve pass over the supplied paths.
    DietScip,
}

/// Which mode `--family` names, if any. Mixing a mode with a mask name is an
/// ERROR rather than a silent pick: `--family cst,scip` has no honest reading
/// (one is a per-file mask over one file, the other a whole-project index run),
/// and guessing one would produce a stream the caller did not ask for.
fn family_mode(families: Option<&[String]>) -> Result<Option<FamilyMode>, String> {
    let Some(families) = families else {
        return Ok(None);
    };
    let named: Vec<&str> = families.iter().map(|name| name.trim()).collect();
    let mode = named.iter().find_map(|name| match *name {
        "scip" => Some(FamilyMode::Scip),
        "diet_scip" => Some(FamilyMode::DietScip),
        _ => None,
    });
    let Some(mode) = mode else {
        return Ok(None);
    };
    let mode_names: Vec<&str> = named
        .iter()
        .copied()
        .filter(|name| matches!(*name, "scip" | "diet_scip"))
        .collect();
    if mode_names.len() > 1 {
        return Err(format!(
            "--family named both {} and {}; scip and diet_scip are different \
             answers to the same question and one invocation gives one of them",
            mode_names[0], mode_names[1]
        ));
    }
    let extras: Vec<&str> = named
        .iter()
        .copied()
        .filter(|name| !matches!(*name, "scip" | "diet_scip"))
        .collect();
    if !extras.is_empty() {
        return Err(format!(
            "--family {} is a whole-project mode and cannot combine with {:?}, \
             which select the per-file extraction mask",
            mode_names[0], extras
        ));
    }
    Ok(Some(mode))
}

/// `--family scip ROOT`: ensure the root's SCIP index (existing wins, else the
/// detected indexer runs under the budget) and stream v5's `scip_*` relation
/// shapes. Named skips ride the stream as `scip_skip` rows; the index location
/// is a stderr line because it is machine-dependent and would pin a checkout
/// path into any golden that captured stdout.
fn stream_scip_family(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.paths.len() != 1 {
        return Err("--family scip takes exactly one ROOT directory".into());
    }
    let request = ScipFamilyRequest {
        root: &cli.paths[0],
        cache_dir: cli.scip_cache.as_deref(),
        budget: match cli.scip_timeout {
            Some(secs) if secs > 0 => IndexBudget { secs },
            Some(_) => return Err("--scip-timeout must be a positive number of seconds".into()),
            None => IndexBudget::from_env(),
        },
        slug: None,
    };
    for line in scip_family_jsonl(&request)? {
        emit(&line)?;
    }
    if let Some(path) = scip_index_location(&request) {
        // @eprintln-ok: CLI-UX location line, deliberately off the fact stream.
        eprintln!("extract: scip index {}", path.display());
    }
    Ok(())
}

/// Every mode but `--family scip` takes source FILES. A directory or a missing
/// path reaches the library as an `io::Error` Debug dump that names no cause.
fn check_file_paths(paths: &[PathBuf]) {
    for path in paths {
        let stop = if path.is_dir() {
            format!(
                "{} is a directory; --resolve takes files, so expand the tree \
                 with a shell glob or find",
                path.display()
            )
        } else if !path.exists() {
            format!("{} does not exist", path.display())
        } else {
            continue;
        };
        // @eprintln-ok: CLI-UX argument error, off the fact stream, exit 2.
        eprintln!("extract: {stop}");
        std::process::exit(2);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let summary = sprefa_extract::trace::install();
    let outcome = match run() {
        Ok(()) => Ok(()),
        // A consumer closing the pipe early (`extract FILE | head -1`) is a
        // clean exit 0 with nothing on stderr, whatever path the write failed
        // on: the BufWriter stream, a flush, or one of the row loops.
        Err(error) if is_broken_pipe(error.as_ref()) => Ok(()),
        Err(error) => Err(error),
    };
    if let Some(state) = summary {
        state.print();
    }
    outcome
}

/// True when the error chain carries an io::BrokenPipe.
fn is_broken_pipe(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(step) = current {
        if let Some(io) = step.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::BrokenPipe {
                return true;
            }
        }
        current = step.source();
    }
    false
}

/// One stdout row, `println!` minus the panic on a closed pipe: `println!`
/// panics with "failed printing to stdout" (rc 101) when the consumer closed
/// early, and the error here propagates to `main`'s BrokenPipe intercept.
fn emit(line: &str) -> Result<(), std::io::Error> {
    let mut out = std::io::stdout().lock();
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("query") {
        if let Err(error) = query::run(std::env::args().skip(1)) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("move") {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        if let Err(error) = source_move::run(argv) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("rename") {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        if let Err(error) = source_rename::run(argv) {
            eprintln!("{error}");
            std::process::exit(error.exit);
        }
        return Ok(());
    }
    let cli = Cli::parse();

    // `--scip-timeout` must reach the library's `ScipMode::Build` path, whose
    // budget comes from `IndexBudget::from_env` (project.rs). Setting the same
    // variable the library documents keeps one budget rule for both build
    // paths; `--family scip` threads its own `IndexBudget` and ignores this.
    if let Some(secs) = cli.scip_timeout {
        if secs > 0 {
            std::env::set_var("SPREFA_SCIP_TIMEOUT_SECS", secs.to_string());
        }
    }

    if cli.schema {
        print_schema();
        return Ok(());
    }

    if !cli.ingest.is_empty() {
        return stream_ingest(&cli.ingest);
    }

    // The two named families are whole-project modes, so they are dispatched
    // before every per-file path below.
    let mode = family_mode(cli.family.as_deref())?;
    // `--family scip`, `--scip-facts` and `--scip-deps` take a ROOT directory;
    // every other mode takes files.
    if !matches!(mode, Some(FamilyMode::Scip)) && !cli.scip_facts && !cli.scip_deps {
        check_file_paths(&cli.paths);
    }
    match mode {
        Some(FamilyMode::Scip) => {
            stream_scip_family(&cli)?;
            return Ok(());
        }
        Some(FamilyMode::DietScip) => {
            for line in diet_scip_jsonl(&cli.paths)? {
                emit(&line)?;
            }
            return Ok(());
        }
        None => {}
    }

    if cli.resolve {
        stream_resolve(&cli)?;
        return Ok(());
    }

    if cli.deps {
        for line in diet_file_edges_jsonl(&scip_request(&cli)?)? {
            emit(&line)?;
        }
        return Ok(());
    }

    if cli.package_deps {
        for line in package_edges_jsonl(&scip_request(&cli)?)? {
            emit(&line)?;
        }
        return Ok(());
    }

    if cli.scip_deps {
        for line in scip_file_edges_jsonl(&scip_request(&cli)?)? {
            emit(&line)?;
        }
        return Ok(());
    }

    if cli.scip_facts {
        for line in scip_facts_jsonl(&scip_request(&cli)?)? {
            emit(&line)?;
        }
        return Ok(());
    }

    if cli.paths.len() != 1 {
        return Err("exactly one PATH is required unless --resolve is given".into());
    }

    let path = &cli.paths[0];
    let content = std::fs::read(path)?;
    let path_str = path.to_string_lossy();
    // The file row rides the SAME read as extraction: counting lines must never
    // cost a second pass over the file, let alone a second subprocess.
    if cli.file_fact {
        emit(&serde_json::to_string(&file_fact(&path_str, &content))?)?;
    }
    // Before any parse, so the ceiling bounds the cost it exists to bound. The
    // file row above is a digest over bytes already read, not that cost.
    let limit = cli.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
    let bytes = content.len() as u64;
    if limit > 0 && bytes > limit {
        tracing::warn!(path = %path_str, bytes, limit, "input over the byte ceiling");
        emit(&serde_json::to_string(&size_skip_fact(
            &path_str, bytes, limit,
        ))?)?;
        return Ok(());
    }
    if !cli.ast_pattern.is_empty() {
        let queries = parse_ast_queries(&cli.ast_pattern, &cli.ast_selector, &cli.ast_capture)?;
        stream_ast_queries(&path_str, &content, &queries)?;
        return Ok(());
    }
    let mask = match cli.family.as_deref() {
        Some(families) => parse_mask(families)?,
        None => FamilyMask::ALL,
    };
    let cfg = cli
        .family
        .as_deref()
        .is_some_and(|families| families.iter().any(|family| family.trim() == "cfg"));
    // The cfg plane is derived AFTER the flatten, so its rows would land past
    // the coverage rows and outside the numbering.
    if cli.witness && cfg {
        return Err("--witness does not cover --family cfg: the cfg plane is derived \
                    after the flatten, so its rows carry no fact ordinal"
            .into());
    }
    if cli.bench {
        bench(&path_str, &content, mask, cfg)?;
    } else {
        stream(&path_str, &content, mask, cfg, cli.witness)?;
    }
    Ok(())
}

/// The SCIP-mode half of the CLI's flags, shared by `--resolve` and
/// `--scip-facts`.
fn scip_request(cli: &Cli) -> Result<ResolveRequest<'_>, String> {
    Ok(ResolveRequest {
        paths: &cli.paths,
        arms: ResolveArms::default(),
        scip: ScipMode::from_flags(cli.scip_index.as_deref(), cli.scip_build),
        project_root: cli.project_root.as_deref(),
        scip_records: match &cli.scip_record {
            Some(spec) => ScipRecords::parse(spec)?,
            None => ScipRecords::all(),
        },
        occurrence_text: cli.occurrence_text,
        rust_checker: cli.rust_checker.then(|| cli.project_root.as_deref()).flatten(),
        ts_checker: cli.ts_checker.then(|| cli.project_root.as_deref()).flatten(),
        witness: cli.witness,
    })
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
        emit(&serde_json::to_string(&fact)?)?;
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
            flow: false,
        },
        Some(families) => parse_arms(families)?,
    };
    let request = ResolveRequest {
        arms,
        ..scip_request(cli)?
    };
    for line in resolve_project_jsonl(&request)? {
        emit(&line)?;
    }
    Ok(())
}

/// `--family` under `--resolve`. An unknown name is a named stop; `parse_mask`
/// refuses unknown names the same way.
fn parse_arms(families: &[String]) -> Result<ResolveArms, String> {
    let mut arms = ResolveArms::default();
    for family in families {
        match family.trim() {
            "call" => arms.call = true,
            "type" | "types" => arms.types = true,
            "flow" => arms.flow = true,
            other => {
                tracing::warn!(family = other, "not a resolve arm");
                return Err(format!(
                    "--family '{other}' is not a resolve arm; under --resolve only \
                     'call', 'type' and 'flow' are meaningful"
                ));
            }
        }
    }
    if !arms.call && !arms.types && !arms.flow {
        return Err("--family selected no resolve arm; name call, type or flow".to_string());
    }
    Ok(arms)
}

fn parse_mask(families: &[String]) -> Result<FamilyMask, String> {
    let mut mask = FamilyMask::NONE;
    for family in families {
        match family.trim() {
            "cst" => mask.cst = true,
            "type" | "types" => mask.types = true,
            "call" => mask.call = true,
            "df" => mask.df = true,
            "data" => mask.data = true,
            // The cfg plane is derived from the cst parse, so it turns cst on.
            "cfg" => mask.cst = true,
            other => {
                tracing::warn!(family = other, "not a mask family");
                return Err(format!(
                    "--family '{other}' is not a mask family; per-file families are \
                     cst, type, call, df, data, cfg"
                ));
            }
        }
    }
    Ok(mask)
}

fn stream(
    path: &str,
    content: &[u8],
    mask: FamilyMask,
    cfg: bool,
    witness: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // ONE BufWriter held for the whole run: a per-row println! goes through
    // LineWriter, which flushes on every newline and turns 2M-row streams
    // into 2M write syscalls.
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::with_capacity(256 * 1024, stdout.lock());
    // Each row is written and dropped: collecting them first held a second copy
    // of the whole stream, which on a 13 MB bundle is 800 MB of the 1,094 MB peak.
    let mut write = |fact: FlatFact| -> Result<(), std::io::Error> {
        serde_json::to_writer(&mut out, &fact)?;
        out.write_all(b"\n")
    };
    // One run per invocation, scoped to the digest of the bytes just read: two
    // runs over the same file are comparable without re-reading it.
    let run = witness.then(|| RunOut {
        run: 0,
        mode: Mode::Syntax,
        tool: "extract".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        scope: vec![content_id_of(content).to_string()],
    });
    if let Some(bundle) = dispatch(path, content, mask) {
        flatten_each(&bundle, run.as_ref(), &mut write)?;
        // The cfg plane rides the SAME parse: it is derived from `bundle.cst`.
        if cfg {
            if let Some(cfg_bundle) = cfg_bundle(path, &bundle, content) {
                flatten_cfg_each(&cfg_bundle, &mut write)?;
            }
        }
    }
    out.flush()?;
    Ok(())
}

fn bench(
    path: &str,
    content: &[u8],
    mask: FamilyMask,
    cfg: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(src) = source_for(path) else {
        tracing::warn!(path, "no Source matches this path; nothing to bench");
        eprintln!("no source for {path}"); // @eprintln-ok: CLI-UX summary, not a diagnostic.
        return Ok(());
    };
    let t = Instant::now();
    let out = src.extract(path, content, mask);
    let extract = t.elapsed();
    let t = Instant::now();
    let mut facts = 0usize;
    let counted: Result<(), std::convert::Infallible> = flatten_each(&out, None, &mut |_| {
        facts += 1;
        Ok(())
    });
    counted.expect("counting cannot fail");
    let serial = t.elapsed();
    // The cfg plane rides the SAME parse, so its timing is charged separately
    // from extract rather than folded into it.
    let (cfg_nodes, cfg_elapsed) = if cfg {
        let t = Instant::now();
        let nodes = cfg_bundle(path, &out, content).map_or(0, |bundle| bundle.nodes.len());
        (nodes, Some(t.elapsed()))
    } else {
        (0, None)
    };
    eprintln!(
        "{}: extract {:?} serial {:?}{} (cst={} type={} call={} df={} data={} cfg={} facts={})",
        src.name(),
        extract,
        serial,
        cfg_elapsed.map_or(String::new(), |elapsed| format!(" cfg {elapsed:?}")),
        out.cst.as_ref().map_or(0, |b| b.nodes.len()),
        out.types.as_ref().map_or(0, |b| b.nodes.len()),
        out.call.as_ref().map_or(0, |b| b.nodes.len()),
        out.df.as_ref().map_or(0, |b| b.nodes.len()),
        out.data
            .as_ref()
            .map_or(0, |b| b.aux.docs.len() + b.aux.values.len()),
        cfg_nodes,
        facts,
    );
    Ok(())
}

/// `--schema` prints the library's own wire contract. The text lives in
/// `sprefa_extract::wire::SCHEMA`, not here, so a library consumer can read the
/// same contract without shelling out to this binary.
fn print_schema() {
    let _ = emit(&schema_text());
}

/// The reverse door. Every file is one stream, so line numbers in a stop run
/// across the whole argument list rather than restarting per file.
fn stream_ingest(paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    check_file_paths(paths);
    let mut lines: Vec<String> = Vec::new();
    for path in paths {
        lines.extend(std::fs::read_to_string(path)?.lines().map(str::to_string));
    }
    match ingest(lines.into_iter()) {
        Ok(rows) => {
            for row in rows {
                emit(&row)?;
            }
            Ok(())
        }
        Err(error) => {
            // @eprintln-ok: CLI-UX stop, off the fact stream, exit 1.
            eprintln!("extract: {error}");
            std::process::exit(1);
        }
    }
}
