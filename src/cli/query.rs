//! `dl what <anchor>` and `dl summary <path>` — the turnkey query verbs. Both
//! are daemon-first: when a daemon serves this root the answer comes from its
//! warm engine over a `what`/`summary` RPC; otherwise a fresh in-process engine
//! ticks a family-forcing probe (see [`crate::anchor::probe_program`]) and reads
//! the same tables. Output goes through the shared `daemon::print_rows` plus a
//! total/paging + notes footer.

use anyhow::Result;

use super::daemon as cli_daemon; // print_rows (the CLI output helper)
use super::root;
use crate::anchor::QueryOut;
use crate::daemon; // status / what / summary / enabled / load_repos_eager (the server module)

/// Parsed verb flags. `positional` is the anchor (for `what`) or the path (for
/// `summary`); the rest are the shared knobs.
#[derive(Default)]
struct Opts {
    positional: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    no_daemon: bool,
    db: Option<String>,
}

/// Hand-parse the verb args (the pre-clap subcommands never reach clap). A bare
/// value is the positional; `--limit`/`--offset`/`--db` take a value;
/// `--no-daemon` is a switch.
fn parse_opts(args: &[String]) -> Opts {
    let mut o = Opts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                o.limit = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--offset" => {
                o.offset = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--db" => {
                o.db = args.get(i + 1).cloned();
                i += 2;
            }
            "--no-daemon" => {
                o.no_daemon = true;
                i += 1;
            }
            s if !s.starts_with("--") && o.positional.is_none() => {
                o.positional = Some(s.to_string());
                i += 1;
            }
            _ => i += 1,
        }
    }
    o
}

/// True iff the daemon path should be attempted (not forced off and enabled).
fn daemon_first(o: &Opts) -> bool {
    !o.no_daemon && daemon::enabled()
}

/// Build a fresh in-process engine, tick the family-forcing probe so the
/// type/call/dataflow/module/comment/doc (and any available SCIP) tables
/// populate, and return it. Root is the current directory (the tree to scan);
/// `--db` reuses a cache file, else it is in-memory.
fn warm_engine(o: &Opts) -> Result<crate::engine::Engine> {
    let cwd = std::env::current_dir()?;
    let root = cwd.canonicalize().unwrap_or(cwd);
    let conn = crate::db::open(o.db.as_deref())?;
    let mut eng = crate::engine::Engine::new(conn, root);
    eng.set_repos(daemon::load_repos_eager());
    let prog = crate::anchor::probe_program()?;
    eng.tick(&prog, true)?;
    Ok(eng)
}

/// Print a `QueryOut`-shaped answer: the rows via the shared helper, then a
/// paging footer (when paged or offset) and any advisory notes.
fn print_answer(columns: &[String], rows: &[Vec<String>], total: usize, notes: &[String], offset: Option<usize>) {
    cli_daemon::print_rows(columns, rows);
    let off = offset.unwrap_or(0);
    if total != rows.len() || off > 0 {
        println!("(showing {}-{} of {total} total)", off, off + rows.len());
    }
    for note in notes {
        println!("{note}");
    }
}

/// `dl what <anchor> [--limit N] [--offset M] [--no-daemon] [--db PATH]`.
pub fn run_what(args: &[String]) -> Result<i32> {
    let o = parse_opts(args);
    let Some(anchor) = o.positional.clone() else {
        eprintln!("usage: dl what <anchor> [--limit N] [--offset M] [--no-daemon]");
        return Ok(2);
    };
    let target = root::daemon_target()?;
    if daemon_first(&o) && daemon::status(Some(&target))?.is_some() {
        let ans = daemon::what(Some(&target), &anchor, o.limit, o.offset)?;
        print_answer(&ans.columns, &ans.rows, ans.total, &ans.notes, o.offset);
    } else {
        let eng = warm_engine(&o)?;
        let out: QueryOut = crate::anchor::what(&eng, &anchor, o.limit, o.offset);
        print_answer(&out.columns, &out.rows, out.total, &out.notes, o.offset);
    }
    Ok(0)
}

/// `dl summary <path> [--no-daemon] [--db PATH]`.
pub fn run_summary(args: &[String]) -> Result<i32> {
    let o = parse_opts(args);
    let Some(path) = o.positional.clone() else {
        eprintln!("usage: dl summary <path> [--no-daemon]");
        return Ok(2);
    };
    let target = root::daemon_target()?;
    if daemon_first(&o) && daemon::status(Some(&target))?.is_some() {
        let ans = daemon::summary(Some(&target), &path)?;
        print_answer(&ans.columns, &ans.rows, ans.total, &ans.notes, None);
    } else {
        let eng = warm_engine(&o)?;
        let out: QueryOut = crate::anchor::summary(&eng, &path);
        print_answer(&out.columns, &out.rows, out.total, &out.notes, None);
    }
    Ok(0)
}
