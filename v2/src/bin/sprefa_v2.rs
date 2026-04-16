use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, EventSink, GitBlobReader, InMemoryLocator,
    MemWriter, OpCtx, OpId, OperatorRegistry, ProgramCtx, RunId,
    RuntimeConfig, host_parse, lower_rules,
};
use v2::ops::default_registry;
use v2::_0_types::Cursor;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "sprefa-v2", about = "sprefa v2 dogfood CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Run {
        file: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

// ---------------------------------------------------------------------------
// Repo discovery
// ---------------------------------------------------------------------------

/// Scan `root` for git repos. If `root` itself has `.git`, return it as the
/// sole repo (slug = basename). Otherwise do a 1-level scan of subdirs.
fn discover_repos(root: &std::path::Path) -> Vec<(String, PathBuf)> {
    if root.join(".git").exists() {
        let slug = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string());
        return vec![(slug, root.to_path_buf())];
    }

    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join(".git").exists() {
                let slug = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repo".to_string());
                found.push((slug, path));
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Run { file, root } => {
            let root = root
                .unwrap_or_else(|| std::env::current_dir().expect("cannot read cwd"));

            // Read .sprf source
            let src = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read {}: {e}", file.display());
                    std::process::exit(1);
                }
            };

            // Discover repos under root
            let repos = discover_repos(&root);

            // Build locator — expose HEAD as "main" for each found repo
            let mut locator = InMemoryLocator::new();
            for (slug, path) in &repos {
                // Detect branch names from git
                let revs = detect_revs(path);
                let rev_refs: Vec<&str> = revs.iter().map(|s| s.as_str()).collect();
                locator = locator.with_repo(slug, path.clone(), &rev_refs);
            }

            let cfg = Arc::new(Config {
                repos:        vec![],
                revs:         vec![],
                fs_exclude:   vec![],
                sprf_files:   vec![],
                shell_allow:  vec![],
                runtime: RuntimeConfig {
                    worker_threads:    1,
                    buffer_size:       256,
                    flush_interval_ms: 100,
                    collect_witnesses: true,
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
                },
                content_hash: 0,
            });

            let reader = Arc::new(GitBlobReader::new(Arc::new(locator), cfg.clone()));
            let writer = Arc::new(MemWriter::new());

            // Parse
            let file_arc: Arc<std::path::Path> = Arc::from(file.as_path());
            let invs = match host_parse(&src, file_arc) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("parse error: {e:?}");
                    std::process::exit(1);
                }
            };

            // Extract rule names from pipe[i].ops[0].paren_src before moving into lower
            let rule_names: Vec<String> = invs.iter().map(|p| {
                p.ops.first()
                    .and_then(|op| op.paren_src.as_ref())
                    .map(|ps| ps.src.trim().to_string())
                    .unwrap_or_else(|| "?".to_string())
            }).collect();

            // Lower
            let pctx = ProgramCtx::new(cfg.clone(), make_registry());
            let outcome = lower_rules(invs, pctx);

            if !outcome.diags.is_empty() {
                for d in &outcome.diags {
                    eprintln!("diag [{}]", d.code());
                }
                std::process::exit(1);
            }

            // Run each pipeline
            for (pipeline, rule_name) in outcome.pipelines.iter().zip(rule_names.iter()) {
                let (result_store, xref_seen) = OpCtx::fresh_xref_state();
                let (__mtx, __mrx) = tokio::sync::mpsc::channel::<v2::mutations::MutationRequest>(32);
                std::mem::drop(__mrx);
                let ctx = OpCtx {
                    run_id: RunId(1),
                    op_id:  OpId(0),
                    reader: reader.clone(),
                    writer: writer.clone(),
                    config: cfg.clone(),
                    diags:  DiagSink(Arc::new(|_| {})),
                    events: EventSink(Arc::new(|_| {})),
                    result_store,
                    xref_seen,
                    store:        std::sync::Arc::new(v2::store::NoopStore::new())
                                  as std::sync::Arc<dyn v2::store::Store>,
                    mutations:    __mtx,
                    cancel:       tokio_util::sync::CancellationToken::new(),
                    expr_name:    None,
                    current_site: std::sync::Arc::new(v2::ParseSite {
                        file:       std::sync::Arc::from(std::path::Path::new("")),
                        path:       std::sync::Arc::from(Vec::<v2::ParseSeg>::new().into_boxed_slice()),
                        byte_range: 0..0,
                    }),
                };

                let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
                    futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();

                let batches: Vec<Arc<[Cursor]>> =
                    block_on(pipeline.run(empty, ctx).collect());
                let cursors: Vec<Cursor> = batches.into_iter()
                    .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
                    .collect();

                println!("rule {rule_name} — {} rows", cursors.len());
                for c in &cursors {
                    // Sort capture keys for deterministic output
                    let mut keys: Vec<&str> = c.captures.keys().map(|k| k.as_ref()).collect();
                    keys.sort();
                    let pairs: Vec<String> = keys
                        .iter()
                        .map(|k| format!("{k}={}", c.captures[*k].value))
                        .collect();
                    let kv = pairs.join(", ");

                    let loc = match &c.fs {
                        Some(fp) => format!(" {}", fp.0.display()),
                        None => String::new(),
                    };
                    println!("  {}@{}{}: {kv}", c.repo, c.rev, loc);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rev detection
// ---------------------------------------------------------------------------

/// Return branch names from a git repo. Falls back to ["HEAD"] on any error.
fn detect_revs(path: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(path)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let branches: Vec<String> = std::str::from_utf8(&o.stdout)
                .unwrap_or("")
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            if branches.is_empty() {
                vec!["HEAD".to_string()]
            } else {
                branches
            }
        }
        _ => vec!["HEAD".to_string()],
    }
}
