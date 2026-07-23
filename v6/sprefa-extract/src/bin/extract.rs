//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not
//! buffer the whole corpus; the lib drains). `--bench` times parse / walk /
//! serialize separately to stderr; this is the harness that will race oxc once
//! oxc is the Parser (commit 2). Commit 1: one file (dir walk lands with the
//! corpus harness).

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{
    dispatch_cst, flatten_cst, AstGrepParser, CstF, CstProjector, FamilyBundle, Parser as _,
    Project as _, Strings,
};

#[derive(Parser)]
#[command(
    name = "extract",
    about = "sprefa-extract: corpus -> flat graph facts (JSONL to stdout)"
)]
struct Cli {
    /// A file to extract (commit 1: single file; dir walk lands with the corpus harness).
    path: PathBuf,

    /// Time parse / walk / serialize to stderr instead of streaming facts to stdout.
    #[arg(long)]
    bench: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let content = std::fs::read(&cli.path)?;
    let path = cli.path.to_string_lossy();
    let parser = AstGrepParser;
    let projector = CstProjector;

    if cli.bench {
        let parse_start = Instant::now();
        let root = parser.parse(&path, &content)?;
        let parse_time = parse_start.elapsed();

        let mut bundle = FamilyBundle::<CstF>::default();
        let mut strings = Strings::new();
        let walk_start = Instant::now();
        projector.project(&root, &mut strings, &mut bundle);
        let walk_time = walk_start.elapsed();

        let ser_start = Instant::now();
        let facts = flatten_cst(&bundle, &strings);
        let ser_time = ser_start.elapsed();

        eprintln!("parse   {:?}", parse_time);
        eprintln!("walk    {:?}", walk_time);
        eprintln!("serial  {:?}", ser_time);
        eprintln!("nodes   {}", bundle.nodes.len());
        eprintln!("edges   {}", bundle.edges.len());
        eprintln!("facts   {}", facts.len());
    } else {
        let (bundle, strings) = dispatch_cst(&path, &content, &parser, &projector)?;
        for fact in flatten_cst(&bundle, &strings) {
            println!("{}", serde_json::to_string(&fact)?);
        }
    }
    Ok(())
}
