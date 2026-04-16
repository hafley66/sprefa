use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use futures_util::StreamExt;

use v2::_0_types::{CursorExpr, RunEvent, RunId};
use v2::_7_init_cursors::{init_cursors, InitInputs};
use v2::mutations::{spawn_handler, AutoApprove};
use v2::{
    Config, GitBlobReader, InMemoryLocator, MemWriter, OperatorRegistry,
    ProgramCtx, RuntimeConfig, host_parse, lower_rules,
};
use v2::ops::default_registry;
use v2::store::{SqliteStore, Store};

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
        /// SQLite store path. Defaults to in-memory.
        #[arg(long)]
        store: Option<PathBuf>,
    },
}

// ---------------------------------------------------------------------------
// Repo discovery
// ---------------------------------------------------------------------------

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
            if branches.is_empty() { vec!["HEAD".to_string()] } else { branches }
        }
        _ => vec!["HEAD".to_string()],
    }
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Run { file, root, store } => {
            let root = root
                .unwrap_or_else(|| std::env::current_dir().expect("cannot read cwd"));

            let src = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read {}: {e}", file.display());
                    std::process::exit(1);
                }
            };

            let repos = discover_repos(&root);
            let mut locator = InMemoryLocator::new();
            let mut cfg_repos: Vec<Arc<str>> = Vec::new();
            let mut cfg_revs:  Vec<Arc<str>> = Vec::new();
            for (slug, path) in &repos {
                let revs = detect_revs(path);
                let rev_refs: Vec<&str> = revs.iter().map(|s| s.as_str()).collect();
                locator = locator.with_repo(slug, path.clone(), &rev_refs);
                cfg_repos.push(Arc::<str>::from(slug.as_str()));
                for r in &revs {
                    let r_arc: Arc<str> = Arc::<str>::from(r.as_str());
                    if !cfg_revs.iter().any(|existing| existing == &r_arc) {
                        cfg_revs.push(r_arc);
                    }
                }
            }

            let cfg = Arc::new(Config {
                repos:        cfg_repos,
                revs:         cfg_revs,
                fs_exclude:   vec![],
                sprf_files:   vec![],
                shell_allow:  vec![],
                runtime: RuntimeConfig {
                    worker_threads:       1,
                    buffer_size:          256,
                    flush_interval_ms:    100,
                    collect_witnesses:    true,
                    xref_cartesian_limit: 10_000,
                    max_passes:           8,
                    max_claims_per_pass:  10_000,
                    max_cursors_per_root: 1_000_000,
                },
                content_hash: 0,
            });

            let reader = Arc::new(GitBlobReader::new(Arc::new(locator), cfg.clone()));
            let writer = Arc::new(MemWriter::new());

            let file_arc: Arc<std::path::Path> = Arc::from(file.as_path());
            let invs = match host_parse(&src, file_arc) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("parse error: {e:?}");
                    std::process::exit(1);
                }
            };

            let rule_names: Vec<Arc<str>> = invs.iter().map(|p| {
                p.ops.first()
                    .and_then(|op| op.paren_src.as_ref())
                    .map(|ps| Arc::<str>::from(ps.src.trim()))
                    .unwrap_or_else(|| Arc::<str>::from(""))
            }).collect();

            let pctx = ProgramCtx::new(cfg.clone(), make_registry());
            let outcome = lower_rules(invs, pctx);

            if !outcome.diags.is_empty() {
                for d in &outcome.diags {
                    eprintln!("diag [{}]", d.code());
                }
                std::process::exit(1);
            }

            let store_arc: Arc<dyn Store> = match store {
                Some(p) => match SqliteStore::open(&p).await {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: open store {}: {e:?}", p.display());
                        std::process::exit(1);
                    }
                }
                None => SqliteStore::open_memory().await.expect("open_memory"),
            };

            let exprs: Vec<CursorExpr> = outcome.pipelines.into_iter()
                .zip(rule_names.iter().cloned())
                .map(|(pipeline, name)| CursorExpr {
                    name: if name.is_empty() { None } else { Some(name) },
                    pipeline,
                })
                .collect();

            let cancel = tokio_util::sync::CancellationToken::new();
            let (mtx, mrx) = tokio::sync::mpsc::channel(64);
            let _handler = spawn_handler(Arc::new(AutoApprove), mrx, cancel.clone());

            let mut stream = init_cursors(InitInputs {
                exprs,
                config:       cfg.clone(),
                store:        store_arc,
                reader,
                writer,
                mutations_tx: mtx,
                cancel:       cancel.clone(),
                run_id:       RunId(1),
                scanner_hash: Arc::from(""),
                result_store: None,
            });

            let mut cursors_by_expr: std::collections::HashMap<Arc<str>, Vec<v2::_0_types::Cursor>> =
                std::collections::HashMap::new();
            while let Some(ev) = stream.next().await {
                match ev {
                    RunEvent::Cursor { expr_name, cursor } => {
                        let key = expr_name.unwrap_or_else(|| Arc::from(""));
                        cursors_by_expr.entry(key).or_default().push(cursor);
                    }
                    RunEvent::Diag { diag } => {
                        eprintln!("diag [{}]", diag.code());
                    }
                    RunEvent::ExprDone { .. } | RunEvent::MutationPrompt { .. } => {}
                    RunEvent::Done => break,
                }
            }

            for name in &rule_names {
                let cursors = cursors_by_expr.get(name)
                    .map(|v| v.as_slice()).unwrap_or(&[]);
                let display = if name.is_empty() { "?" } else { name.as_ref() };
                println!("rule {display} — {} rows", cursors.len());
                for c in cursors {
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

            cancel.cancel();
        }
    }
}
