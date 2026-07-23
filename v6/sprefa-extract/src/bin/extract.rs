//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not
//! buffer the whole corpus; the lib drains). For a TS/JS file it runs BOTH
//! families: CstF via ast-grep (one dep covers rust/ts/go grammars) and TypeF
//! via oxc. `--bench` times each family's parse / walk / serialize separately
//! to stderr; the oxc-vs-ast-grep parse split is the race that sharpens once
//! oxc owns more families (commit 3+). Commit 2: single file.

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{
    dispatch_call, dispatch_cst, dispatch_type, flatten_call, flatten_cst, flatten_type,
    AstGrepParser, CallF, CallProjector, CstF, CstProjector, FamilyBundle, OxcParser, Parser as _,
    Project as _, Strings, TypeF, TypeProjector,
};

#[derive(Parser)]
#[command(
    name = "extract",
    about = "sprefa-extract: corpus -> flat graph facts (JSONL to stdout)"
)]
struct Cli {
    /// A file to extract (commit 2: single file).
    path: PathBuf,

    /// Time each family's parse / walk / serialize to stderr instead of streaming facts.
    #[arg(long)]
    bench: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let content = std::fs::read(&cli.path)?;
    let path = cli.path.to_string_lossy();
    if cli.bench {
        bench(&path, &content)?;
    } else {
        stream(&path, &content)?;
    }
    Ok(())
}

fn stream(path: &str, content: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // CstF via ast-grep (rust/ts/tsx/js/go grammars in one dep).
    let (cst, cst_strings) = dispatch_cst(path, content, &AstGrepParser, &CstProjector)?;
    for fact in flatten_cst(&cst, &cst_strings) {
        println!("{}", serde_json::to_string(&fact)?);
    }
    // TypeF via oxc (TS/JS only); skipped silently if oxc has no grammar for the path.
    if OxcParser.matches(path) {
        let (ty, ty_strings) = dispatch_type(path, content, &OxcParser, &TypeProjector)?;
        for fact in flatten_type(&ty, &ty_strings) {
            println!("{}", serde_json::to_string(&fact)?);
        }
        // CallF: a second projection over the same oxc tree (defs + call sites).
        let (call, call_strings) = dispatch_call(path, content, &OxcParser, &CallProjector)?;
        for fact in flatten_call(&call, &call_strings) {
            println!("{}", serde_json::to_string(&fact)?);
        }
    }
    Ok(())
}

fn bench(path: &str, content: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let cst_parser = AstGrepParser;
    let cst_projector = CstProjector;
    let arena = cst_parser.make_arena();
    let t = Instant::now();
    let cst_parsed = cst_parser.parse(&arena, path, content)?;
    let parse_cst = t.elapsed();
    let mut cst_bundle = FamilyBundle::<CstF>::default();
    let mut cst_strings = Strings::new();
    let t = Instant::now();
    cst_projector.project(&cst_parsed, &mut cst_strings, &mut cst_bundle);
    let walk_cst = t.elapsed();
    let t = Instant::now();
    let cst_facts = flatten_cst(&cst_bundle, &cst_strings);
    let ser_cst = t.elapsed();
    eprintln!(
        "cst:  parse {:?} walk {:?} serial {:?} ({} nodes, {} edges, {} facts)",
        parse_cst, walk_cst, ser_cst, cst_bundle.nodes.len(), cst_bundle.edges.len(), cst_facts.len(),
    );

    if OxcParser.matches(path) {
        let ty_parser = OxcParser;
        let ty_projector = TypeProjector;
        let arena = ty_parser.make_arena();
        let t = Instant::now();
        let ty_parsed = ty_parser.parse(&arena, path, content)?;
        let parse_oxc = t.elapsed();
        let mut ty_bundle = FamilyBundle::<TypeF>::default();
        let mut ty_strings = Strings::new();
        let t = Instant::now();
        ty_projector.project(&ty_parsed, &mut ty_strings, &mut ty_bundle);
        let walk_oxc = t.elapsed();
        let t = Instant::now();
        let ty_facts = flatten_type(&ty_bundle, &ty_strings);
        let ser_oxc = t.elapsed();
        eprintln!(
            "type: parse {:?} walk {:?} serial {:?} ({} entities, {} facts)",
            parse_oxc, walk_oxc, ser_oxc, ty_bundle.nodes.len(), ty_facts.len(),
        );

        // CallF reuses the oxc parse; the bench times its own walk + serialize.
        let call_projector = CallProjector;
        let t = Instant::now();
        let mut call_bundle = FamilyBundle::<CallF>::default();
        let mut call_strings = Strings::new();
        call_projector.project(&ty_parsed, &mut call_strings, &mut call_bundle);
        let walk_call = t.elapsed();
        let t = Instant::now();
        let call_facts = flatten_call(&call_bundle, &call_strings);
        let ser_call = t.elapsed();
        eprintln!(
            "call: walk {:?} serial {:?} ({} defs, {} sites, {} facts)",
            walk_call, ser_call, call_bundle.nodes.len(), call_bundle.aux.sites.len(), call_facts.len(),
        );
    }
    Ok(())
}
