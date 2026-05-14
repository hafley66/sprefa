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

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug)]
struct Args {
    path: PathBuf,
    show_rows: bool,
    max_diags: usize,
    remote: Option<String>,
    root: Option<PathBuf>,
    fact_db: Option<PathBuf>,
    queue_db: Option<PathBuf>,
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
    let mut path: Option<PathBuf> = None;
    let mut show_rows: bool = cfg.run.show_rows.unwrap_or(true);
    let mut max_diags: usize = cfg.run.max_diags.unwrap_or(50);
    let mut remote: Option<String> = cfg.run.remote.clone();
    let mut root: Option<PathBuf> = cfg.run.root.clone();
    let mut fact_db: Option<PathBuf> = cfg.run_fact_db();
    let mut queue_db: Option<PathBuf> = cfg.run_queue_db();
    let mut telemetry: bool = false;
    let mut batch_cap: Option<usize> = None;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--show-rows" => {
                show_rows = true;
                i += 1;
            }
            "--no-show-rows" => {
                show_rows = false;
                i += 1;
            }
            "--max-diags" => {
                let v = raw.get(i + 1).ok_or("--max-diags needs a value")?;
                max_diags = v.parse().map_err(|_| format!("bad --max-diags: {v}"))?;
                i += 2;
            }
            "--remote" => {
                let v = raw.get(i + 1).ok_or("--remote needs URL")?;
                remote = Some(v.clone());
                i += 2;
            }
            "--root" => {
                let v = raw.get(i + 1).ok_or("--root needs a path")?;
                root = Some(PathBuf::from(v));
                i += 2;
            }
            "--fact-db" => {
                let v = raw.get(i + 1).ok_or("--fact-db needs a path")?;
                fact_db = Some(PathBuf::from(v));
                i += 2;
            }
            "--queue-db" => {
                let v = raw.get(i + 1).ok_or("--queue-db needs a path")?;
                queue_db = Some(PathBuf::from(v));
                i += 2;
            }
            "--batch" | "--batch-cap" => {
                let v = raw.get(i + 1).ok_or("--batch needs a value")?;
                batch_cap = Some(v.parse().map_err(|_| format!("bad --batch: {v}"))?);
                i += 2;
            }
            "--telemetry" => {
                telemetry = true;
                i += 1;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
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
        show_rows,
        max_diags,
        remote,
        root,
        fact_db,
        queue_db,
        telemetry,
        batch_cap,
    })
}

fn print_usage() {
    eprintln!("sprefa-run <path-to-sprf-file> \
[--show-rows|--no-show-rows] [--telemetry] [--batch N] [--max-diags N] [--remote URL] [--root PATH] [--fact-db PATH] [--queue-db PATH]");
}

fn rss_peak_kb() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return 0;
        }
        #[cfg(target_os = "macos")]
        {
            (u.ru_maxrss as u64) / 1024
        }
        #[cfg(not(target_os = "macos"))]
        {
            u.ru_maxrss as u64
        }
    }
}

/// 1-indexed (line, col) for byte offset `off` in `src`.
fn line_col(src: &str, off: u32) -> (u32, u32) {
    let off = (off as usize).min(src.len());
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == off {
            break;
        }
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn print_diags(path: &str, src: &str, diags: &[SprfDiag], cap: usize) -> usize {
    let n_err = diags.iter().filter(|d| d.severity == "error").count();
    let printed = diags.len().min(cap);
    for d in diags.iter().take(printed) {
        let (line, col) = match d.lo {
            Some(lo) => line_col(src, lo),
            None => (1, 1),
        };
        println!(
            "{path}:{line}:{col}:{sev}:{code}: {msg}",
            sev = d.severity,
            code = d.code,
            msg = d.message
        );
    }
    if diags.len() > cap {
        println!(
            "{path}:1:1:info:sprefa-run/diag-truncated: \
                  omitted {n} more diagnostic(s) (--max-diags {cap})",
            n = diags.len() - cap
        );
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
        Ok(a) => a,
        Err(e) => {
            eprintln!("sprefa-run: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    if args.telemetry {
        std::env::set_var("SPREFA_TELEMETRY", "1");
    }
    if let Some(batch_cap) = args.batch_cap {
        std::env::set_var("SPREFA_BATCH_CAP", batch_cap.to_string());
    }

    let path_disp = args.path.display().to_string();
    let src = match std::fs::read_to_string(&args.path) {
        Ok(s) => s,
        Err(e) => {
            println!("{path_disp}:1:1:error:sprefa-run/io: {e}");
            return ExitCode::from(1);
        }
    };

    // One client interface, two transports. Same call sites.
    let root = args
        .root
        .clone()
        .or_else(|| args.path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let client: Box<dyn SprfClient> = match args.remote.clone() {
        Some(url) => Box::new(HttpClient::new(url)),
        None => match (args.fact_db.as_ref(), args.queue_db.as_ref()) {
            (Some(fact_db), Some(queue_db)) => {
                let state = std::sync::Arc::new(SprfState::new_with_sqlite_backends(
                    root, fact_db, queue_db,
                ));
                Box::new(InProcessClient::new(build_router(state)))
            }
            (Some(fact_db), None) => {
                let state = std::sync::Arc::new(SprfState::new_with_sqlite_facts(root, fact_db));
                Box::new(InProcessClient::new(build_router(state)))
            }
            (None, Some(queue_db)) => {
                let state = std::sync::Arc::new(SprfState::new_with_sqlite_queue(root, queue_db));
                Box::new(InProcessClient::new(build_router(state)))
            }
            (None, None) => {
                let (_state, c) = build_in_process(root);
                Box::new(c)
            }
        },
    };

    let report = match client
        .run(RunReq {
            path: args.path.clone(),
            root: args.root.clone(),
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("{path_disp}:1:1:error:sprefa-run/run: {e}");
            return ExitCode::from(1);
        }
    };

    let parse_errs = print_diags(&path_disp, &src, &report.parse_diags, args.max_diags);
    let walk_errs = print_diags(&path_disp, &src, &report.walk_diags, args.max_diags);
    if parse_errs + walk_errs > 0 {
        return ExitCode::from(1);
    }
    let runtime_errs = print_diags(&path_disp, &src, &report.runtime_diags, args.max_diags);
    if runtime_errs > 0 {
        return ExitCode::from(1);
    }

    if let Some(t) = &report.telemetry {
        print_telemetry(t);
        if let Some(fact_db) = &args.fact_db {
            print_sqlite_db_telemetry(fact_db);
        }
    }

    if report.tables.is_empty() {
        println!("(no rules — FactStore not introspected)");
        return ExitCode::from(0);
    }
    println!("── facts ──");
    for name in &report.tables {
        let fetch_t = std::time::Instant::now();
        let tbl = match client
            .get_fact_table(GetFactTableReq {
                name: name.clone(),
                limit: Some(5),
            })
            .await
        {
            Ok(t) => t,
            Err(SprfError::UnknownDoc(_)) => continue,
            Err(e) => {
                eprintln!("get_fact_table {name}: {e}");
                continue;
            }
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
        "wall_ms={:.1} rendered={} emitted={} terminal={} parked={} rss_peak_MB={}",
        t.wall_ms,
        t.rendered,
        t.emitted,
        t.terminal,
        t.parked,
        rss_peak_kb() / 1024,
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
        "fs seen={} emitted={} ext_skipped={} filter_skipped={} walk_ms={:.1} filter_ms={:.1} cursor_ms={:.1} splice_ms={:.1}",
        t.fs.seen_files,
        t.fs.emitted,
        t.fs.ext_skipped,
        t.fs.filter_skipped,
        t.fs.walk_ms,
        t.fs.filter_ms,
        t.fs.cursor_ms,
        t.fs.splice_ms,
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
    if let Some(s) = &t.fact_store {
        println!(
            "store insert_calls={} insert_rows={} insert_batch_calls={} insert_batch_rows={} commit_calls={} string_intern_calls={} string_intern_rows={} sqlite_insert_exec_calls={} sqlite_insert_exec_rows={} transactions={} index_rebuilds={} index_rebuild_indexes={} insert_batch_ms={:.1} string_intern_ms={:.1} sqlite_insert_ms={:.1} index_drop_ms={:.1} index_create_ms={:.1} index_rebuild_ms={:.1} transaction_commit_ms={:.1} commit_ms={:.1}",
            s.insert_calls,
            s.insert_rows,
            s.insert_batch_calls,
            s.insert_batch_rows,
            s.commit_calls,
            s.string_intern_calls,
            s.string_intern_rows,
            s.sqlite_insert_exec_calls,
            s.sqlite_insert_exec_rows,
            s.transactions,
            s.index_rebuilds,
            s.index_rebuild_indexes,
            s.insert_batch_ms,
            s.string_intern_ms,
            s.sqlite_insert_ms,
            s.index_drop_ms,
            s.index_create_ms,
            s.index_rebuild_ms,
            s.transaction_commit_ms,
            s.commit_ms,
        );
        let mut warnings = Vec::new();
        if s.insert_calls > 1_000 {
            warnings.push(format!(
                "n_plus_one_insert_calls={} insert_rows={}",
                s.insert_calls, s.insert_rows
            ));
        }
        if s.insert_batch_calls > 0 {
            let avg_batch = s.insert_batch_rows as f64 / s.insert_batch_calls as f64;
            if s.insert_batch_rows > 10_000 && avg_batch < 1_000.0 {
                warnings.push(format!(
                    "small_insert_batches avg_batch={avg_batch:.1} calls={} rows={}",
                    s.insert_batch_calls, s.insert_batch_rows
                ));
            }
        }
        if s.sqlite_insert_exec_calls > 0 {
            let avg_sqlite_insert =
                s.sqlite_insert_exec_rows as f64 / s.sqlite_insert_exec_calls as f64;
            if s.sqlite_insert_exec_rows > 10_000 && avg_sqlite_insert < 100.0 {
                warnings.push(format!(
                    "n_plus_one_sqlite_insert_execs avg_rows_per_exec={avg_sqlite_insert:.1} execs={} rows={}",
                    s.sqlite_insert_exec_calls, s.sqlite_insert_exec_rows
                ));
            }
        }
        if s.index_rebuilds > 0 {
            warnings.push(format!(
                "index_rebuilds={} indexes={} rebuild_ms={:.1}",
                s.index_rebuilds, s.index_rebuild_indexes, s.index_rebuild_ms
            ));
        }
        if !warnings.is_empty() {
            println!("store_warnings {}", warnings.join(" "));
        }
    }
    if let Some(s) = &t.effect_runtime {
        println!(
            "effect_runtime pull_calls={} pull_rows={} pull_ms={:.1} dispatch_calls={} dispatch_rows={} dispatch_ms={:.1} put_calls={} put_ms={:.1}",
            s.pull_calls,
            s.pull_rows,
            s.pull_ms,
            s.dispatch_calls,
            s.dispatch_rows,
            s.dispatch_ms,
            s.put_calls,
            s.put_ms,
        );
    }
    if let Some(s) = &t.runtime_graph {
        println!(
            "runtime_graph name={} calls={} rows={} MB={:.1} transactions={} wall_ms={:.1} avg_rows_per_tx={:.1}",
            s.name,
            s.calls,
            s.rows,
            s.bytes as f64 / (1024.0 * 1024.0),
            s.transactions,
            s.wall_ms(),
            if s.transactions == 0 {
                0.0
            } else {
                s.rows as f64 / s.transactions as f64
            },
        );
    }
}

fn print_sqlite_db_telemetry(path: &PathBuf) {
    let Ok(conn) = rusqlite::Connection::open(path) else {
        return;
    };
    let db_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let page_size = sqlite_page_size(&conn).unwrap_or(4096);
    let page_bytes = sqlite_page_bytes(&conn).unwrap_or(0);
    println!(
        "db file_MB={:.1} page_MB={:.1}",
        db_bytes as f64 / (1024.0 * 1024.0),
        page_bytes as f64 / (1024.0 * 1024.0),
    );
    for obj in sqlite_objects(&conn) {
        if obj.kind == "index" && obj.bytes <= page_size {
            continue;
        }
        let rows = if obj.kind == "table" {
            sqlite_count_rows(&conn, obj.name.as_str())
        } else {
            None
        };
        println!(
            "db_{} name={} rows={} bytes={} MB={:.1}",
            obj.kind,
            obj.name,
            rows.map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            obj.bytes,
            obj.bytes as f64 / (1024.0 * 1024.0),
        );
    }
}

struct SqliteObjectStat {
    kind: String,
    name: String,
    bytes: u64,
}

fn sqlite_page_bytes(conn: &rusqlite::Connection) -> Option<u64> {
    let page_count: u64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get::<_, u64>(0))
        .ok()?;
    let page_size = sqlite_page_size(conn)?;
    Some(page_count * page_size)
}

fn sqlite_page_size(conn: &rusqlite::Connection) -> Option<u64> {
    conn.query_row("PRAGMA page_size", [], |row| row.get::<_, u64>(0))
        .ok()
}

fn sqlite_objects(conn: &rusqlite::Connection) -> Vec<SqliteObjectStat> {
    let mut objects = sqlite_schema_objects(conn);
    let bytes_by_name = sqlite_dbstat_bytes(conn);
    for object in &mut objects {
        object.bytes = *bytes_by_name.get(object.name.as_str()).unwrap_or(&0);
    }
    objects.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
    objects
}

fn sqlite_schema_objects(conn: &rusqlite::Connection) -> Vec<SqliteObjectStat> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT type, name
         FROM sqlite_master
         WHERE type IN ('table', 'index')
           AND name NOT LIKE 'sqlite_%'
         ORDER BY type, name",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok(SqliteObjectStat {
            kind: row.get(0)?,
            name: row.get(1)?,
            bytes: 0,
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}

fn sqlite_dbstat_bytes(conn: &rusqlite::Connection) -> std::collections::BTreeMap<String, u64> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(mut stmt) = conn.prepare("SELECT name, SUM(pgsize) FROM dbstat GROUP BY name") else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let bytes: Option<u64> = row.get(1)?;
        Ok((name, bytes.unwrap_or(0)))
    }) else {
        return out;
    };
    for row in rows.flatten() {
        out.insert(row.0, row.1);
    }
    out
}

fn sqlite_count_rows(conn: &rusqlite::Connection, table: &str) -> Option<u64> {
    let quoted = sqlite_quote_ident(table);
    conn.query_row(
        format!("SELECT COUNT(*) FROM {quoted}").as_str(),
        [],
        |row| row.get::<_, u64>(0),
    )
    .ok()
}

fn sqlite_quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
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

        let args = parse_args_from(
            [
                "dev.sprf",
                "--show-rows",
                "--telemetry",
                "--batch",
                "65536",
                "--max-diags",
                "3",
                "--fact-db",
                "/tmp/cli-facts.db",
            ],
            &cfg,
        )
        .unwrap();
        assert!(args.show_rows);
        assert!(args.telemetry);
        assert_eq!(args.batch_cap, Some(65536));
        assert_eq!(args.max_diags, 3);
        assert_eq!(args.fact_db, Some(PathBuf::from("/tmp/cli-facts.db")));
        assert_eq!(args.queue_db, Some(PathBuf::from("/tmp/queue.db")));
    }
}
