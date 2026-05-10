// sprefa-run — single-file v4 driver dispatching every action through
// the unified `SprfClient` (axum::Router via tower::oneshot in-process,
// or reqwest at a URL with --remote).
//
// Usage:
//   sprefa-run <path-to-sprf-file> [--show-rows | --no-show-rows]
//                                  [--max-diags N]
//                                  [--remote http://host:port]
//                                  [--fact-db path]
//                                  [--queue-db path]
//
// Diag format (one per line, suitable for VS Code problem matchers):
//   <path>:<line>:<col>:<severity>:<code>: <message>
//
// Exit codes: 0 ok, 1 io/parse/walk error (after diags flushed), 2 cli usage.

use std::path::PathBuf;
use std::process::ExitCode;

use v4::app::{
    build_in_process, build_router, GetFactTableReq, HttpClient, InProcessClient, RunReq,
    SprfClient, SprfDiag, SprfError, SprfState,
};
use v4::config::SprfConfig;

#[derive(Debug)]
struct Args {
    path:      PathBuf,
    show_rows: bool,
    max_diags: usize,
    remote:    Option<String>,
    root:      Option<PathBuf>,
    fact_db:   Option<PathBuf>,
    queue_db:  Option<PathBuf>,
    telemetry: bool,
    batch_cap: Option<usize>,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1), &SprfConfig::load_default())
}

fn parse_args_from<I, S>(raw: I, cfg: &SprfConfig) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw: Vec<String> = raw.into_iter().map(Into::into).collect();
    let mut path:      Option<PathBuf> = None;
    let mut show_rows: bool            = cfg.run.show_rows.unwrap_or(true);
    let mut max_diags: usize           = cfg.run.max_diags.unwrap_or(50);
    let mut remote:    Option<String>  = cfg.run.remote.clone();
    let mut root:      Option<PathBuf> = cfg.run.root.clone();
    let mut fact_db:   Option<PathBuf> = cfg.run_fact_db();
    let mut queue_db:  Option<PathBuf> = cfg.run_queue_db();
    let mut telemetry: bool            = false;
    let mut batch_cap: Option<usize>   = None;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--show-rows"    => { show_rows = true;  i += 1; }
            "--no-show-rows" => { show_rows = false; i += 1; }
            "--max-diags" => {
                let v = raw.get(i+1).ok_or("--max-diags needs a value")?;
                max_diags = v.parse().map_err(|_| format!("bad --max-diags: {v}"))?;
                i += 2;
            }
            "--remote" => {
                let v = raw.get(i+1).ok_or("--remote needs URL")?;
                remote = Some(v.clone());
                i += 2;
            }
            "--root" => {
                let v = raw.get(i+1).ok_or("--root needs a path")?;
                root = Some(PathBuf::from(v));
                i += 2;
            }
            "--fact-db" => {
                let v = raw.get(i+1).ok_or("--fact-db needs a path")?;
                fact_db = Some(PathBuf::from(v));
                i += 2;
            }
            "--queue-db" => {
                let v = raw.get(i+1).ok_or("--queue-db needs a path")?;
                queue_db = Some(PathBuf::from(v));
                i += 2;
            }
            "--batch" | "--batch-cap" => {
                let v = raw.get(i+1).ok_or("--batch needs a value")?;
                batch_cap = Some(v.parse().map_err(|_| format!("bad --batch: {v}"))?);
                i += 2;
            }
            "--telemetry" => { telemetry = true; i += 1; }
            "-h" | "--help" => { print_usage(); std::process::exit(0); }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {other}"));
            }
            other => {
                if path.is_some() {
                    return Err(format!("unexpected positional: {other}"));
                }
                path = Some(PathBuf::from(other));
                i += 1;
            }
        }
    }
    Ok(Args {
        path: path.ok_or("missing <path-to-sprf-file>")?,
        show_rows, max_diags, remote, root, fact_db, queue_db, telemetry, batch_cap,
    })
}

fn print_usage() {
    eprintln!("sprefa-run <path-to-sprf-file> \
[--show-rows|--no-show-rows] [--telemetry] [--batch N] [--max-diags N] [--remote URL] [--root PATH] [--fact-db PATH] [--queue-db PATH]");
}

/// 1-indexed (line, col) for byte offset `off` in `src`.
fn line_col(src: &str, off: u32) -> (u32, u32) {
    let off = (off as usize).min(src.len());
    let mut line: u32 = 1;
    let mut col:  u32 = 1;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}

fn print_diags(path: &str, src: &str, diags: &[SprfDiag], cap: usize) -> usize {
    let n_err = diags.iter().filter(|d| d.severity == "error").count();
    let printed = diags.len().min(cap);
    for d in diags.iter().take(printed) {
        let (line, col) = match d.lo {
            Some(lo) => line_col(src, lo),
            None     => (1, 1),
        };
        println!("{path}:{line}:{col}:{sev}:{code}: {msg}",
                 sev = d.severity, code = d.code, msg = d.message);
    }
    if diags.len() > cap {
        println!("{path}:1:1:info:sprefa-run/diag-truncated: \
                  omitted {n} more diagnostic(s) (--max-diags {cap})",
                 n = diags.len() - cap);
    }
    n_err
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // Tracing: only initialize a subscriber when the env var is set.
    // No subscriber = `event_enabled!` returns false everywhere, the
    // runtime skips Instant::now() and span allocation. Activate with:
    //   SPREFA_LOG=expand=debug ./sprefa-run …
    //   SPREFA_LOG=trace ./sprefa-run …
    if let Ok(filter) = std::env::var("SPREFA_LOG") {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_target(true)
            .with_writer(std::io::stderr)
            .try_init();
    }

    let args = match parse_args() {
        Ok(a)  => a,
        Err(e) => { eprintln!("sprefa-run: {e}"); print_usage(); return ExitCode::from(2); }
    };
    if args.telemetry {
        std::env::set_var("SPREFA_TELEMETRY", "1");
    }
    if let Some(batch_cap) = args.batch_cap {
        std::env::set_var("SPREFA_BATCH_CAP", batch_cap.to_string());
    }

    let path_disp = args.path.display().to_string();
    let src = match std::fs::read_to_string(&args.path) {
        Ok(s)  => s,
        Err(e) => {
            println!("{path_disp}:1:1:error:sprefa-run/io: {e}");
            return ExitCode::from(1);
        }
    };

    // One client interface, two transports. Same call sites.
    let root = args.root.clone()
        .or_else(|| args.path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let client: Box<dyn SprfClient> = match args.remote.clone() {
        Some(url) => Box::new(HttpClient::new(url)),
        None => {
            match (args.fact_db.as_ref(), args.queue_db.as_ref()) {
                (Some(fact_db), Some(queue_db)) => {
                    let state = std::sync::Arc::new(
                        SprfState::new_with_sqlite_backends(root, fact_db, queue_db)
                    );
                    Box::new(InProcessClient::new(build_router(state)))
                }
                (Some(fact_db), None) => {
                    let state = std::sync::Arc::new(
                        SprfState::new_with_sqlite_facts(root, fact_db)
                    );
                    Box::new(InProcessClient::new(build_router(state)))
                }
                (None, Some(queue_db)) => {
                    let state = std::sync::Arc::new(
                        SprfState::new_with_sqlite_queue(root, queue_db)
                    );
                    Box::new(InProcessClient::new(build_router(state)))
                }
                (None, None) => {
                    let (_state, c) = build_in_process(root);
                    Box::new(c)
                }
            }
        }
    };

    let report = match client.run(RunReq { path: args.path.clone(), root: args.root.clone() }).await {
        Ok(r)  => r,
        Err(e) => { println!("{path_disp}:1:1:error:sprefa-run/run: {e}"); return ExitCode::from(1); }
    };

    let parse_errs = print_diags(&path_disp, &src, &report.parse_diags, args.max_diags);
    let walk_errs  = print_diags(&path_disp, &src, &report.walk_diags,  args.max_diags);
    if parse_errs + walk_errs > 0 { return ExitCode::from(1); }
    let runtime_errs = print_diags(&path_disp, &src, &report.runtime_diags, args.max_diags);
    if runtime_errs > 0 { return ExitCode::from(1); }

    if let Some(t) = &report.telemetry {
        print_telemetry(t);
    }

    if report.tables.is_empty() {
        println!("(no rules — FactStore not introspected)");
        return ExitCode::from(0);
    }
    println!("── facts ──");
    for name in &report.tables {
        let fetch_t = std::time::Instant::now();
        let tbl = match client.get_fact_table(GetFactTableReq {
            name: name.clone(), limit: Some(5),
        }).await {
            Ok(t)  => t,
            Err(SprfError::UnknownDoc(_)) => continue,
            Err(e) => { eprintln!("get_fact_table {name}: {e}"); continue; }
        };
        if args.telemetry {
            println!(
                "{}: {} rows fetch_ms={:.1}",
                tbl.name,
                tbl.total,
                fetch_t.elapsed().as_secs_f64() * 1000.0,
            );
        } else {
            println!("{}: {} rows", tbl.name, tbl.total);
        }
        if args.show_rows {
            for (i, r) in tbl.rows.iter().enumerate() {
                println!("  [{i}] {:?}", r.fields);
            }
            if tbl.total > tbl.rows.len() {
                println!("  … {} more", tbl.total - tbl.rows.len());
            }
        }
    }
    ExitCode::from(0)
}

fn print_telemetry(t: &v4::telemetry::RunTelemetry) {
    println!("── telemetry ──");
    println!(
        "wall_ms={:.1} rendered={} emitted={} terminal={} parked={}",
        t.wall_ms, t.rendered, t.emitted, t.terminal, t.parked,
    );
    println!(
        "run_ms read_sprf={:.1} parse={:.1} lower={:.1} wrap_mount={:.1} expand={:.1} commit={:.1} resume={:.1} collect_tables={:.1} report={:.1}",
        t.phases.read_sprf_ms,
        t.phases.parse_ms,
        t.phases.lower_ms,
        t.phases.wrap_mount_ms,
        t.phases.expand_ms,
        t.phases.commit_ms,
        t.phases.resume_ms,
        t.phases.collect_tables_ms,
        t.phases.report_ms,
    );
    println!(
        "fs seen={} emitted={} ext_skipped={} filter_skipped={}",
        t.fs.seen_files, t.fs.emitted, t.fs.ext_skipped, t.fs.filter_skipped,
    );
    println!(
        "ast inputs={} source_reads={} source_MB={:.1} utf8_rows={} utf8_MB={:.1} prefilter_skips={} parses={} matches={}",
        t.ast.input_rows,
        t.ast.source_read_rows,
        t.ast.source_read_bytes as f64 / (1024.0 * 1024.0),
        t.ast.source_utf8_rows,
        t.ast.source_utf8_bytes as f64 / (1024.0 * 1024.0),
        t.ast.prefilter_skips,
        t.ast.parses,
        t.ast.matches,
    );
    println!(
        "ast_ms pattern={:.1} read={:.1} prefilter={:.1} utf8={:.1} legacy={:.1} parse={:.1} match={:.1} emit_stamp={:.1}",
        t.ast.pattern_ms,
        t.ast.source_read_ms,
        t.ast.prefilter_ms,
        t.ast.utf8_ms,
        t.ast.legacy_ms,
        t.ast.parse_ms,
        t.ast.match_ms,
        t.ast.emit_stamp_ms,
    );
    for s in &t.stages {
        println!(
            "stage {:>24}: calls={} rows={} avg_batch={:.1} max_batch={} wall_ms={:.1} rows_per_sec={:.0}",
            s.name,
            s.calls,
            s.rows,
            if s.calls == 0 { 0.0 } else { s.rows as f64 / s.calls as f64 },
            s.max_batch,
            s.wall_ms,
            s.rows_per_sec,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use v4::config::{RunConfig, StoreConfig};

    #[test]
    fn parse_args_uses_config_defaults() {
        let cfg = SprfConfig {
            store: StoreConfig {
                fact_db: Some(PathBuf::from("/tmp/facts.db")),
                queue_db: Some(PathBuf::from("/tmp/queue.db")),
            },
            run: RunConfig {
                root: Some(PathBuf::from("/tmp/root")),
                remote: Some("http://127.0.0.1:8787".into()),
                show_rows: Some(false),
                max_diags: Some(7),
                fact_db: None,
                queue_db: None,
            },
            ..Default::default()
        };

        let args = parse_args_from(["dev.sprf"], &cfg).unwrap();
        assert_eq!(args.path, PathBuf::from("dev.sprf"));
        assert!(!args.show_rows);
        assert_eq!(args.max_diags, 7);
        assert_eq!(args.remote.as_deref(), Some("http://127.0.0.1:8787"));
        assert_eq!(args.root, Some(PathBuf::from("/tmp/root")));
        assert_eq!(args.fact_db, Some(PathBuf::from("/tmp/facts.db")));
        assert_eq!(args.queue_db, Some(PathBuf::from("/tmp/queue.db")));
        assert!(!args.telemetry);
        assert_eq!(args.batch_cap, None);
    }

    #[test]
    fn parse_args_cli_overrides_config_defaults() {
        let cfg = SprfConfig {
            store: StoreConfig {
                fact_db: Some(PathBuf::from("/tmp/facts.db")),
                queue_db: Some(PathBuf::from("/tmp/queue.db")),
            },
            run: RunConfig {
                show_rows: Some(false),
                max_diags: Some(7),
                ..Default::default()
            },
            ..Default::default()
        };

        let args = parse_args_from([
            "dev.sprf",
            "--show-rows",
            "--telemetry",
            "--batch", "65536",
            "--max-diags", "3",
            "--fact-db", "/tmp/cli-facts.db",
        ], &cfg).unwrap();
        assert!(args.show_rows);
        assert!(args.telemetry);
        assert_eq!(args.batch_cap, Some(65536));
        assert_eq!(args.max_diags, 3);
        assert_eq!(args.fact_db, Some(PathBuf::from("/tmp/cli-facts.db")));
        assert_eq!(args.queue_db, Some(PathBuf::from("/tmp/queue.db")));
    }
}
