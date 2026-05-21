use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space")]
struct Cli {
    program: String,
    #[arg(long)]
    db: Option<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    watch: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize()?;
    if cli.watch {
        sprefa_v5::run_watch(&cli.program, cli.db.as_deref(), root)
    } else {
        sprefa_v5::run_file(&cli.program, cli.db.as_deref(), root)
    }
}
