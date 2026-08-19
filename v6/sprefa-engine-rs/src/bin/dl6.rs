// @comment-ok: the binary's usage contract, the one doc site for its flags.
// dl6 build <prog>.dl6 [--out <path>] [--adapters <file>]
//
// Compiles the program for the Rust target, writes a cargo bin crate from
// src/build_template/ under <engine>/target/dl6-build/<prog>/, builds it
// release, and copies the binary to --out. Wall time per step on stdout.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

const TEMPLATE_MAIN: &str = include_str!("../build_template/main.rs");
const TEMPLATE_CARGO: &str = include_str!("../build_template/Cargo.toml.in");

#[derive(Parser)]
#[command(name = "dl6", version, about = "the dl6 program toolchain")]
struct Cli {
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Build one `.dl6` program into one binary.
    Build(BuildArgs),
}

#[derive(Args)]
struct BuildArgs {
    /// The program source.
    source: PathBuf,
    /// Where the built binary lands. Defaults beside the generated crate.
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,
    /// The `.adapters.json` sidecar to embed. Defaults to `<prog>.adapters.json`
    /// beside the source, and to an empty row set when there is none.
    #[arg(long, value_name = "FILE")]
    adapters: Option<PathBuf>,
}

// The one seam onto the .dl6 compiler. `dl6c` (v6/prolog/dl6c.pl) wraps the
// same compile_dl6/3 call and replaces this once it emits `.types.rs` too.
struct Dl6Compiler {
    repo_root: PathBuf,
}

impl Dl6Compiler {
    fn emit(&self, source: &Path, out: &Path, loader: &str, emitter: &str) -> Result<()> {
        let source = prolog_atom(source)?;
        let target = prolog_atom(out)?;
        let goal = format!("compile_dl6('{source}','{target}',[emitter({emitter})])");
        let output = Command::new("swipl")
            .arg("-q")
            .arg("-l")
            .arg(self.repo_root.join("v6/prolog/compile.pl"))
            .arg("-l")
            .arg(self.repo_root.join(loader))
            .args(["-g", &goal, "-g", "halt"])
            .output()
            .context("run swipl")?;
        if !output.status.success() {
            bail!(
                "dl6 compile failed ({}): {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    fn program(&self, source: &Path, out: &Path) -> Result<()> {
        self.emit(
            source,
            out,
            "v6/prolog/emit_rust.pl",
            "emit_rust:emit_program",
        )
    }

    fn types(&self, source: &Path, out: &Path) -> Result<()> {
        self.emit(
            source,
            out,
            "v6/sprefa-engine-rs/dl6_build.pl",
            "dl6_build:emit_types",
        )
    }
}

// The goal is a prolog term, so a path carrying a quote would close the atom.
fn prolog_atom(path: &Path) -> Result<String> {
    let text = path
        .to_str()
        .with_context(|| format!("path is not utf-8: {}", path.display()))?;
    if text.contains('\'') || text.contains('\\') {
        bail!("path {text} carries a quote or a backslash; the compiler goal cannot name it");
    }
    Ok(text.to_string())
}

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    engine_dir().join("../..")
}

fn program_name(source: &Path) -> Result<String> {
    source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .with_context(|| format!("{} has no program name", source.display()))
}

fn compiler_sha(repo_root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn adapters_text(source: &Path, chosen: Option<&Path>) -> Result<String> {
    let sidecar = match chosen {
        Some(path) => path.to_path_buf(),
        None => source.with_extension("adapters.json"),
    };
    match std::fs::read_to_string(&sidecar) {
        Ok(text) => Ok(text),
        Err(error) if chosen.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
            Ok("[]\n".to_string())
        }
        Err(error) => Err(error).with_context(|| format!("read {}", sidecar.display())),
    }
}

struct Step {
    started: Instant,
}

impl Step {
    fn start() -> Step {
        Step {
            started: Instant::now(),
        }
    }

    fn done(self, name: &str) {
        println!(
            "dl6 build: {name} {:.2}s",
            self.started.elapsed().as_secs_f64()
        );
    }
}

fn build(args: BuildArgs) -> Result<()> {
    let source = args
        .source
        .canonicalize()
        .with_context(|| format!("read {}", args.source.display()))?;
    let name = program_name(&source)?;
    let engine = engine_dir();
    let repo_root = repo_root();
    let compiler = Dl6Compiler {
        repo_root: repo_root.clone(),
    };

    let crate_dir = engine.join("target/dl6-build").join(&name);
    let source_dir = crate_dir.join("src");
    std::fs::create_dir_all(&source_dir)
        .with_context(|| format!("create {}", source_dir.display()))?;

    let step = Step::start();
    compiler.program(&source, &source_dir.join("program.rs"))?;
    step.done("compile program");

    let step = Step::start();
    compiler.types(&source, &source_dir.join("types.rs"))?;
    step.done("compile types");

    let step = Step::start();
    std::fs::write(
        source_dir.join("adapters.json"),
        adapters_text(&source, args.adapters.as_deref())?,
    )
    .context("write adapters.json")?;
    std::fs::write(
        source_dir.join("main.rs"),
        TEMPLATE_MAIN
            .replace("__DL6_PROGRAM_NAME__", &name)
            .replace("__DL6_COMPILER_SHA__", &compiler_sha(&repo_root)),
    )
    .context("write main.rs")?;
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        TEMPLATE_CARGO
            .replace("__DL6_CRATE_NAME__", &name)
            .replace("__DL6_ENGINE_PATH__", &engine.display().to_string()),
    )
    .context("write Cargo.toml")?;
    step.done("write crate");

    // One shared target directory across programs, never the engine crate's
    // own: a `dl6 build` must not block on a `cargo test` holding that lock.
    let target_dir = match std::env::var_os("DL6_BUILD_TARGET_DIR") {
        Some(directory) => PathBuf::from(directory),
        None => engine.join("target/dl6-build/_target"),
    };
    let step = Step::start();
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(crate_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .context("run cargo build")?;
    if !status.success() {
        bail!("cargo build --release failed ({status})");
    }
    step.done("cargo build --release");

    let built = target_dir.join("release").join(&name);
    let out = args.out.unwrap_or_else(|| crate_dir.join(&name));
    let step = Step::start();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    // rm before cp: overwriting in place leaves the old macOS signature on new
    // bytes and the next run dies "Killed: 9" (docs/failure-modes.md:56).
    let _ = std::fs::remove_file(&out);
    std::fs::copy(&built, &out)
        .with_context(|| format!("copy {} to {}", built.display(), out.display()))?;
    step.done("copy binary");

    println!("dl6 build: wrote {}", out.display());
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().verb {
        Verb::Build(args) => build(args),
    }
}
