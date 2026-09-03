use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::{query_source, SourceQuery, SourceQueryOutput, TreeSitterQuery};

#[derive(Parser)]
#[command(name = "extract query")]
struct QueryCli {
    #[arg(long)]
    lang: String,
    #[arg(long)]
    query: String,
    #[arg(long)]
    digest: Option<String>,
    path: PathBuf,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = QueryCli::try_parse_from(args).map_err(one_line)?;
    let bytes = source_bytes(&cli.path, cli.digest.as_deref())?;
    let request = SourceQuery::TreeSitter(TreeSitterQuery {
        language: cli.lang,
        query: cli.query,
    });
    let output = query_source(cli.path.to_string_lossy().as_ref(), &bytes, &request)
        .map_err(|error| error.to_string())?;
    let SourceQueryOutput::TreeSitter(rows) = output else {
        unreachable!("tree-sitter request returns tree-sitter rows")
    };
    for row in rows {
        println!(
            "{}",
            serde_json::to_string(&row).map_err(|error| format!("query output: {error}"))?
        );
    }
    Ok(())
}

fn source_bytes(path: &PathBuf, digest: Option<&str>) -> Result<Vec<u8>, String> {
    match digest {
        Some(oid) => cat_blob(path, oid),
        None => std::fs::read(path)
            .map_err(|error| format!("query input '{}': {error}", path.display())),
    }
}

fn cat_blob(path: &Path, oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(path.parent().unwrap_or(path))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let mut batch = soopy::GitBatch::open(&repository.root)
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let bytes = batch
        .read(&soopy::ObjectId(oid.into()))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    Ok(bytes.to_vec())
}

fn one_line(error: clap::Error) -> String {
    one_line_text(error.to_string())
}

fn one_line_text(text: String) -> String {
    text.lines()
        .next()
        .unwrap_or("invalid query command")
        .to_string()
}
