// sprefa-run — single-file v4 driver: parse → walk → expand → dump FactStore.
//
// Usage:
//   sprefa-run <path-to-sprf-file> [--store mem|sqlite] [--show-rows]
//                                  [--max-diags N] [--no-show-rows]
//
// Flags:
//   --store mem|sqlite   FactStore backend. Default: mem.
//   --show-rows          Print sample rows per table at the end (default on).
//   --no-show-rows       Suppress per-row sample.
//   --max-diags N        Cap diags printed per phase. Default: 50.
//
// Diag format (one per line, suitable for VS Code problem matchers):
//   <path>:<line>:<col>:<severity>:<code>: <message>
//
// Exit codes:
//   0   no errors
//   1   parse, walk, or fs error (all diags still flushed first)
//   2   bad CLI usage
//
// VS Code wiring lives in v4/.vscode/tasks.json. The "sprefa-run: current
// file" task runs this against ${file} on Cmd-Shift-B.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
    Severity, SqliteFactStore,
};

use v4::Cursor;
use v4::compile::ast::PipeAst;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

#[derive(Debug)]
struct Args {
    path:       PathBuf,
    store_kind: StoreKind,
    show_rows:  bool,
    max_diags:  usize,
}

#[derive(Debug, Clone, Copy)]
enum StoreKind { Mem, Sqlite }

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut path:       Option<PathBuf> = None;
    let mut store_kind: StoreKind        = StoreKind::Mem;
    let mut show_rows:  bool             = true;
    let mut max_diags:  usize            = 50;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--store" => {
                let v = raw.get(i+1).ok_or("--store needs a value")?;
                store_kind = match v.as_str() {
                    "mem"    => StoreKind::Mem,
                    "sqlite" => StoreKind::Sqlite,
                    other    => return Err(format!("--store must be mem|sqlite, got {other}")),
                };
                i += 2;
            }
            "--show-rows"    => { show_rows = true;  i += 1; }
            "--no-show-rows" => { show_rows = false; i += 1; }
            "--max-diags" => {
                let v = raw.get(i+1).ok_or("--max-diags needs a value")?;
                max_diags = v.parse().map_err(|_| format!("bad --max-diags: {v}"))?;
                i += 2;
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

    let path = path.ok_or("missing <path-to-sprf-file>")?;
    Ok(Args { path, store_kind, show_rows, max_diags })
}

fn print_usage() {
    eprintln!("sprefa-run <path-to-sprf-file> [--store mem|sqlite] \
[--show-rows|--no-show-rows] [--max-diags N]");
}

/// 1-indexed (line, col) for byte offset `off` in `src`. Tabs count as 1.
/// Out-of-range offsets clamp to the final line/col.
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

fn sev_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warn  => "warning",
        Severity::Info  => "info",
        Severity::Hint  => "info",
    }
}

fn print_diags(
    path:  &str,
    src:   &str,
    diags: &[effect_runtime::v2::Diag],
    cap:   usize,
) -> usize {
    let n_err = diags.iter().filter(|d| d.severity == Severity::Error).count();
    let printed_max = diags.len().min(cap);
    for d in diags.iter().take(printed_max) {
        let (line, col) = match d.span {
            Some(s) => line_col(src, s.lo),
            None    => (1, 1),
        };
        println!(
            "{path}:{line}:{col}:{sev}:{code}: {msg}",
            sev  = sev_str(d.severity),
            code = d.code,
            msg  = d.message,
        );
    }
    if diags.len() > cap {
        println!(
            "{path}:1:1:info:sprefa-run/diag-truncated: \
             omitted {n} more diagnostic(s) (--max-diags {cap})",
            n = diags.len() - cap,
        );
    }
    n_err
}

/// Pull rule-table names from the AST. The walker registers a sink
/// table per `rule(:NAME, ...)` op, and the rule's first arg is an
/// Atom that becomes the table name. Scan recursively because rules
/// can nest other rules in their block.
fn collect_rule_tables(program: &[PipeAst], out: &mut Vec<String>) {
    for p in program {
        for op in &p.steps {
            if &*op.name == "rule" {
                if let Some(first) = op.args.first() {
                    let raw = first.raw.trim();
                    let name = raw.strip_prefix(':').unwrap_or(raw).trim();
                    if !name.is_empty() && !out.iter().any(|n| n == name) {
                        out.push(name.to_string());
                    }
                }
            }
            if let Some(block) = &op.block {
                collect_rule_tables(std::slice::from_ref(block), out);
            }
        }
    }
}

fn main() -> ExitCode {
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

    // 1. parse
    let (program, parse_diags) = host_parse(&src);
    let parse_errs = print_diags(&path_disp, &src, &parse_diags, args.max_diags);
    if parse_errs > 0 {
        return ExitCode::from(1);
    }

    // 2. walk
    let store: Arc<dyn FactStore<Cursor>> = match args.store_kind {
        StoreKind::Mem    => Arc::new(MemFactStore::<Cursor>::new()),
        StoreKind::Sqlite => Arc::new(
            SqliteFactStore::<Cursor>::open_in_memory()
                .expect("sqlite open_in_memory")
        ),
    };
    let dir = args.path.parent().map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), dir);

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    let walk_errs = print_diags(&path_disp, &src, &walk_diags, args.max_diags);
    if walk_errs > 0 {
        return ExitCode::from(1);
    }

    // 3. expand each pipe
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let opts = ExpandOpts::default();
    for pipe in pipes {
        let inst = pipe.into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            opts.clone(),
        );
    }
    // Flush any buffered store writes (sqlite path needs this).
    store.commit(1, None);

    // 4. dump FactStore. Tables harvested from rule ops in the AST.
    let mut tables: Vec<String> = Vec::new();
    collect_rule_tables(&program, &mut tables);
    if tables.is_empty() {
        println!("(no rules — FactStore not introspected)");
    } else {
        println!("── facts ──");
        for t in &tables {
            let n = store.len(t);
            println!("{t}: {n} rows");
            if args.show_rows && n > 0 {
                let rows = store.rows_of(t);
                for (i, r) in rows.iter().take(5).enumerate() {
                    println!("  [{i}] {:?}", r);
                }
                if n > 5 {
                    println!("  … {} more", n - 5);
                }
            }
        }
    }

    ExitCode::from(0)
}
