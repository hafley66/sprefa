// sprefa-run — single-file v4 driver dispatching every action through
// the unified `SprfClient` (axum::Router via tower::oneshot in-process,
// or reqwest at a URL with --remote).
//
// Usage:
//   sprefa-run <path-to-sprf-file> [--show-rows | --no-show-rows]
//                                  [--max-diags N]
//                                  [--remote http://host:port]
//
// Diag format (one per line, suitable for VS Code problem matchers):
//   <path>:<line>:<col>:<severity>:<code>: <message>
//
// Exit codes: 0 ok, 1 io/parse/walk error (after diags flushed), 2 cli usage.

use std::path::PathBuf;
use std::process::ExitCode;

use v4::app::{
    build_in_process, GetFactTableReq, HttpClient, RunReq,
    SprfClient, SprfDiag, SprfError,
};

#[derive(Debug)]
struct Args {
    path:      PathBuf,
    show_rows: bool,
    max_diags: usize,
    remote:    Option<String>,
    root:      Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut path:      Option<PathBuf> = None;
    let mut show_rows: bool            = true;
    let mut max_diags: usize           = 50;
    let mut remote:    Option<String>  = None;
    let mut root:      Option<PathBuf> = None;

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
        show_rows, max_diags, remote, root,
    })
}

fn print_usage() {
    eprintln!("sprefa-run <path-to-sprf-file> \
[--show-rows|--no-show-rows] [--max-diags N] [--remote URL]");
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

    let path_disp = args.path.display().to_string();
    let src = match std::fs::read_to_string(&args.path) {
        Ok(s)  => s,
        Err(e) => {
            println!("{path_disp}:1:1:error:sprefa-run/io: {e}");
            return ExitCode::from(1);
        }
    };

    // One client interface, two transports. Same call sites.
    let root = args.path.parent().map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let client: Box<dyn SprfClient> = match args.remote.clone() {
        Some(url) => Box::new(HttpClient::new(url)),
        None => {
            let (_state, c) = build_in_process(root);
            Box::new(c)
        }
    };

    let report = match client.run(RunReq { path: args.path.clone(), root: args.root.clone() }).await {
        Ok(r)  => r,
        Err(e) => { println!("{path_disp}:1:1:error:sprefa-run/run: {e}"); return ExitCode::from(1); }
    };

    let parse_errs = print_diags(&path_disp, &src, &report.parse_diags, args.max_diags);
    let walk_errs  = print_diags(&path_disp, &src, &report.walk_diags,  args.max_diags);
    if parse_errs + walk_errs > 0 { return ExitCode::from(1); }

    if report.tables.is_empty() {
        println!("(no rules — FactStore not introspected)");
        return ExitCode::from(0);
    }
    println!("── facts ──");
    for name in &report.tables {
        let tbl = match client.get_fact_table(GetFactTableReq {
            name: name.clone(), limit: Some(5),
        }).await {
            Ok(t)  => t,
            Err(SprfError::UnknownDoc(_)) => continue,
            Err(e) => { eprintln!("get_fact_table {name}: {e}"); continue; }
        };
        println!("{}: {} rows", tbl.name, tbl.total);
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
