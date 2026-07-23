//! The CLI: clap args, NO tokio. Streams flat JSONL to stdout (RSS does not buffer
//! the whole corpus; the lib drains). One data-driven path: `dispatch(path,
//! content, mask)` -> `flatten` -> stdout. `--family` selects the mask (default
//! ALL); `--bench` times extract + flatten and reports per-family counts. The bin
//! names no ast-grep/oxc type outside the `Source` impls (the uniform-surface law).

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

use sprefa_extract::{dispatch, flatten, source_for, FamilyMask};

#[derive(Parser)]
#[command(
    name = "extract",
    about = "sprefa-extract: corpus -> flat graph facts (JSONL to stdout)"
)]
struct Cli {
    /// A file to extract.
    path: PathBuf,

    /// Families to extract (comma list of cst,type,call,df). Default: all.
    #[arg(long, value_delimiter = ',')]
    family: Option<Vec<String>>,

    /// Time extract + flatten and report per-family counts to stderr.
    #[arg(long)]
    bench: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let content = std::fs::read(&cli.path)?;
    let path = cli.path.to_string_lossy();
    let mask = cli.family.as_deref().map(parse_mask).unwrap_or(FamilyMask::ALL);
    if cli.bench {
        bench(&path, &content, mask)?;
    } else {
        stream(&path, &content, mask)?;
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
        eprintln!("no source for {path}");
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
