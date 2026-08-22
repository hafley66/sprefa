// @comment-ok: the binary's usage contract, the one doc site for its flags.
// dl6 build <prog>.dl6 [--out <path>] [--adapters <file>]
// dl6 run   <prog>.dl6 [--arrive <rel>=<v>[,<v>]]... [--final-tsv]
//
// `build` compiles the program for the Rust target, writes a cargo bin crate
// from src/build_template/ under <engine>/target/dl6-build/<prog>/, builds it
// release, and copies the binary to --out. Wall time per step on stdout.
//
// @comment-ok: the run contract, continued.
// `run` skips cargo: the compiled program is a text the engine loads, cached
// under $XDG_CACHE_HOME/sprefa/dl6 by the blake3 of the source and the compiler
// tree, so a second run spawns no swipl. The program's own declarations decide
// whether it folds once and exits or stays up turning each `bind` push into
// one tick; `--once` takes the first branch either way.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use sprefa_engine_rs::run::{self, FinalRequest, RunOptions, SeedSpec, WatchOptions, DRAIN_CAP};

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
    /// Fold one `.dl6` program. The FILE decides whether the process stays: a
    /// rel routed to a continuing source keeps it resident, `--once` never does.
    Run(ProgramArgs),
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

/// One option table, because there is one verb: the program's own declarations
/// decide whether it stays resident, and nothing on this line has to say so.
#[derive(Args)]
struct ProgramArgs {
    /// The program source.
    source: PathBuf,
    /// Seed one arrival row. Repeat it for more.
    #[arg(long = "arrive", value_name = "REL=VALUE[,VALUE...]")]
    arrive: Vec<SeedSpec>,
    /// An arrival schedule to fold before the seeds, as emit_rust_harness reads it.
    #[arg(long, value_name = "FILE")]
    schedule: Option<PathBuf>,
    /// Print every `?` rel as one json document per rel.
    #[arg(long = "final")]
    final_all: bool,
    /// Print the `?` rows and drop the tick log.
    #[arg(long)]
    final_only: bool,
    /// Print the `?` rows as `rel<TAB>col...`, so no shell parses json.
    #[arg(long)]
    final_tsv: bool,
    /// Name and order the rels to print; without it every `?` rel prints, sorted.
    #[arg(long, value_name = "REL[,REL...]", value_delimiter = ',')]
    final_rels: Option<Vec<String>>,
    /// Fold in memory instead of into the one db, for a golden or a probe.
    #[arg(long)]
    in_memory: bool,
    /// Exit 1 when this `?` query answers any row.
    #[arg(long, value_name = "QUERY")]
    fail_on: Option<String>,
    /// The tree the hosts read and a `bind watch` glob is resolved against.
    #[arg(long, value_name = "DIR", default_value = ".")]
    root: PathBuf,
    /// Read the adapters sidecar from here instead of beside the source.
    #[arg(long, value_name = "FILE")]
    adapters: Option<PathBuf>,
    /// Fold `sh` decls from a scripted schedule instead of running them live.
    #[arg(long)]
    no_live_hosts: bool,
    /// Fold a resident program's tick 0 and exit, for a snapshot of one that
    /// would otherwise stay up.
    #[arg(long)]
    once: bool,
}

impl ProgramArgs {
    fn finals(&self) -> FinalRequest {
        FinalRequest {
            wanted: self.final_all
                || self.final_only
                || self.final_tsv
                || self.final_rels.is_some(),
            only: self.final_only,
            tsv: self.final_tsv,
            rels: self.final_rels.clone(),
        }
    }
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
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
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

// ═══ the compile cache ═══════════════════════════════════════════════════════

/// The key a compiled program is stored under: the source's own bytes and a
/// digest of every `.pl` the compiler is built from, so an edit to either misses.
struct CacheKey {
    source: blake3::Hash,
    compiler: blake3::Hash,
}

impl CacheKey {
    fn read(source: &Path, repo_root: &Path) -> Result<CacheKey> {
        let bytes = std::fs::read(source)
            .with_context(|| format!("read {}", source.display()))?;
        Ok(CacheKey {
            source: blake3::hash(&bytes),
            compiler: compiler_tree_digest(repo_root)?,
        })
    }

    fn path(&self) -> PathBuf {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.source.as_bytes());
        hasher.update(self.compiler.as_bytes());
        cache_directory().join(format!("{}.rs", hasher.finalize().to_hex()))
    }
}

// Size and mtime rather than content: the compiler tree is 170 files and this
// key only has to miss when one of them moves, which a stat answers.
fn compiler_tree_digest(repo_root: &Path) -> Result<blake3::Hash> {
    let mut stamps: Vec<String> = Vec::new();
    collect_prolog_stamps(&repo_root.join("v6/prolog"), repo_root, &mut stamps)?;
    stamps.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for stamp in &stamps {
        hasher.update(stamp.as_bytes());
    }
    Ok(hasher.finalize())
}

fn collect_prolog_stamps(
    directory: &Path,
    repo_root: &Path,
    stamps: &mut Vec<String>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `labs/` is scratch and `out/` is emitted, so neither moves the compiler.
        let skip = matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("labs") | Some("out") | Some("node_modules")
        );
        if skip {
            continue;
        }
        if path.is_dir() {
            collect_prolog_stamps(&path, repo_root, stamps)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("pl") {
            let metadata = entry.metadata().with_context(|| format!("stat {}", path.display()))?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_nanos())
                .unwrap_or(0);
            let relative = path.strip_prefix(repo_root).unwrap_or(&path);
            stamps.push(format!("{} {} {modified}", relative.display(), metadata.len()));
        }
    }
    Ok(())
}

fn cache_directory() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("sprefa/dl6")
}

/// The compiled program text, from the cache when the key hits. The write goes
/// to a sibling temp path first, so a killed compile leaves no half program.
fn compiled_program(source: &Path, repo_root: &Path) -> Result<(PathBuf, CacheKey, bool)> {
    let key = CacheKey::read(source, repo_root)?;
    let cached = key.path();
    if cached.is_file() {
        return Ok((cached, key, true));
    }
    let directory = cached.parent().expect("the cache path has a parent");
    std::fs::create_dir_all(directory)
        .with_context(|| format!("create {}", directory.display()))?;
    let staged = cached.with_extension("rs.partial");
    let compiler = Dl6Compiler {
        repo_root: repo_root.to_path_buf(),
    };
    compiler.program(source, &staged)?;
    std::fs::rename(&staged, &cached)
        .with_context(|| format!("install {}", cached.display()))?;
    Ok((cached, key, false))
}

// ═══ run and watch ═══════════════════════════════════════════════════════════

/// The sidecar lives beside the source, exactly as `dl6 build` embeds it, and
/// the engine's loader reads it out of `DL_ADAPTERS_DIR`.
fn point_at_adapters(args: &ProgramArgs) -> Result<()> {
    let directory = match &args.adapters {
        Some(sidecar) => sidecar
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        None => args
            .source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let absolute = directory
        .canonicalize()
        .with_context(|| format!("read the adapters directory {}", directory.display()))?;
    std::env::set_var("DL_ADAPTERS_DIR", absolute);
    Ok(())
}

fn read_schedule(path: &Path) -> Result<Vec<Vec<sprefa_engine_rs::types::Arrival>>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read the schedule {}", path.display()))?;
    let batches: Vec<Vec<sprefa_engine_rs::serve::ArrivalDto>> = serde_json::from_str(&text)
        .with_context(|| format!("parse the schedule {}", path.display()))?;
    batches
        .into_iter()
        .map(|batch| sprefa_engine_rs::serve::arrival_batch(batch).map_err(anyhow::Error::from))
        .collect()
}

/// Everything both verbs do before their first tick: compile (or hit the cache),
/// load the program, resolve the seeds, and move to the tree the hosts read.
fn prepare(args: &ProgramArgs) -> Result<(run::LoadedProgram, Vec<sprefa_engine_rs::types::Arrival>, RunOptions)> {
    let source = args
        .source
        .canonicalize()
        .with_context(|| format!("read {}", args.source.display()))?;
    let repo_root = repo_root();
    let step = Instant::now();
    let (compiled, key, hit) = compiled_program(&source, &repo_root)?;
    eprintln!(
        // @eprintln-ok CLI-UX: the compile wall is what the cache claim is read from
        "dl6: compile {} {:.2}s",
        if hit { "cached" } else { "swipl" },
        step.elapsed().as_secs_f64()
    );
    // run.rs writes both digests into `__meta`, so a `--db` file says which
    // source and which compiler produced the rows a cold reader is looking at.
    std::env::set_var("DL6_SOURCE_DIGEST", key.source.to_hex().as_str());
    std::env::set_var("DL6_COMPILER_DIGEST", key.compiler.to_hex().as_str());
    point_at_adapters(args)?;
    // The trace table is what `tick_cost` reads, and it only records when armed
    // before the first fold.
    if adapters_text(&source, args.adapters.as_deref())?.contains("dl_tick_cost") {
        sprefa_engine_rs::trace::force_summary();
    }
    let db = if args.in_memory { None } else { Some(one_db_path()?) };
    let loaded = run::load_program(&compiled)?;
    let schedule = match &args.schedule {
        Some(path) => read_schedule(&path.canonicalize().with_context(|| {
            format!("read the schedule {}", path.display())
        })?)?,
        None => Vec::new(),
    };
    std::env::set_current_dir(&args.root)
        .with_context(|| format!("move to {}", args.root.display()))?;
    let seeds = run::seed_arrivals(&loaded.program, &args.arrive)?;
    Ok((
        loaded,
        seeds,
        RunOptions {
            schedule,
            live_hosts: !args.no_live_hosts,
            finals: args.finals(),
            db,
            fail_on: args.fail_on.clone(),
            drain_cap: DRAIN_CAP,
        },
    ))
}

// ONE SERVER, ONE DB (CLAUDE.md 2026-08-21): every program this runtime folds
// writes into `~/.agent/dl6.db`, its tables carrying the program's own name.
// `DL6_DB` moves the file for a test; nothing on the command line does.
const ONE_DB: &str = "dl6.db";

fn one_db_path() -> Result<PathBuf> {
    if let Some(named) = std::env::var_os("DL6_DB") {
        return Ok(PathBuf::from(named));
    }
    let home = std::env::var_os("HOME").context("read HOME for the one db")?;
    Ok(PathBuf::from(home).join(".agent").join(ONE_DB))
}

fn run(args: ProgramArgs) -> Result<()> {
    let finals = args.finals();
    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("read {}", args.root.display()))?;
    let (loaded, mut seeds, options) = prepare(&args)?;
    if !args.once && run::stays_resident(&loaded.binds) {
        let (stop, listen) = tokio::sync::watch::channel(false);
        // SIGINT is the one way a resident run ends, and the handler only flips
        // the flag: the loop finishes the tick it is in rather than dying mid-fold.
        ctrlc_flag(stop)?;
        let options = WatchOptions::new(options, loaded.binds.clone(), root);
        if run::watch(&loaded.program, seeds, options, listen)? {
            std::process::exit(1);
        }
        return Ok(());
    }
    // A one-shot fold of a resident program still reads its world: the
    // enumeration is tick 0's rows with no watcher armed behind it.
    seeds.extend(run::bind_seeds(&loaded.binds, Path::new("."))?);
    let outcome = run::run_once(&loaded.program, seeds, options)?;
    run::print_outcome(&loaded.program, &outcome, &finals)?;
    if outcome.failed() {
        std::process::exit(1);
    }
    Ok(())
}

fn ctrlc_flag(stop: tokio::sync::watch::Sender<bool>) -> Result<()> {
    std::thread::Builder::new()
        .name("dl6-signal".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(_) => return,
            };
            runtime.block_on(async {
                let _ = tokio::signal::ctrl_c().await;
                let _ = stop.send(true);
            });
        })
        .context("spawn the signal thread")?;
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    match Cli::parse().verb {
        Verb::Build(args) => build(args),
        Verb::Run(args) => run(args),
    }
}
