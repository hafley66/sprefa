//! `dl what <anchor>` and `dl summary <path>` — the turnkey query verbs. Both
//! are daemon-first: when a daemon serves this root the answer comes from its
//! warm engine over a `what`/`summary` RPC; otherwise a fresh in-process engine
//! ticks a family-forcing probe (see [`crate::anchor::probe_program`]) and reads
//! the same tables. Output goes through the shared `daemon_cmd::print_rows`
//! plus a total/paging + notes footer.

use anyhow::Result;

use super::daemon_cmd as cli_daemon; // print_rows (the CLI output helper)
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
/// `--no-daemon` is an INTERNAL-ONLY switch (one server code path — user
/// directive 2026-07-18): still parsed for tests and daemon-spawned children,
/// absent from every usage line.
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
    crate::anchor::warm_engine(root, o.db.as_deref())
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

/// `dl what <anchor> [--limit N] [--offset M] [--db PATH]`.
pub fn run_what(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("usage: dl what <anchor> [--limit N] [--offset N] [--db PATH]"); // @eprintln-ok: usage/help text
        eprintln!("  resolve a name, glob, path, or path:line and show its graph neighborhood"); // @eprintln-ok: usage/help text
        return Ok(0);
    }
    let o = parse_opts(args);
    let Some(anchor) = o.positional.clone() else {
        eprintln!("usage: dl what <anchor> [--limit N] [--offset N] [--db PATH]"); // @eprintln-ok: usage/help text
        return Ok(2);
    };
    let target = root::daemon_target()?;
    let root_opt = target.root();
    if daemon_first(&o) && daemon::status(None)?.is_some() {
        let ans = daemon::what(root_opt, &anchor, o.limit, o.offset)?;
        print_answer(&ans.columns, &ans.rows, ans.total, &ans.notes, o.offset);
    } else {
        let eng = warm_engine(&o)?;
        let out: QueryOut = crate::anchor::what(&eng, &anchor, o.limit, o.offset);
        print_answer(&out.columns, &out.rows, out.total, &out.notes, o.offset);
    }
    Ok(0)
}

/// Parse the `dl q` args: two positionals (verb, then target) plus the shared
/// knobs. Reuses [`parse_opts`] for the flags, then splits the positional stream
/// into (verb, target).
fn parse_q_opts(args: &[String]) -> (Option<String>, Option<String>, Opts) {
    let mut positionals: Vec<String> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" | "--offset" | "--db" => {
                flags.push(args[i].clone());
                if let Some(v) = args.get(i + 1) {
                    flags.push(v.clone());
                }
                i += 2;
            }
            "--no-daemon" => {
                flags.push(args[i].clone());
                i += 1;
            }
            other if !other.starts_with("--") => {
                positionals.push(other.to_string());
                i += 1;
            }
            _ => i += 1,
        }
    }
    let o = parse_opts(&flags);
    let mut it = positionals.into_iter();
    (it.next(), it.next(), o)
}

/// `dl q <verb> <target> [--limit N] [--offset M] [--db PATH]`.
/// A bare `dl q` (no verb) lists the available verbs. Verbs are embedded `.dl`
/// programs with a `q_target` fact injected; see [`crate::verbs`].
pub fn run_q(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("usage: dl q <verb> <name> [--limit N] [--offset N] [--db PATH]"); // @eprintln-ok: usage/help text
        eprintln!("  verbs: who-calls, where-defined"); // @eprintln-ok: usage/help text
        return Ok(0);
    }
    let (verb, target, o) = parse_q_opts(args);
    let Some(verb) = verb else {
        list_verbs();
        return Ok(0);
    };
    if crate::verbs::find(&verb).is_none() {
        eprintln!("dl q: unknown verb {verb:?}; available: {}", crate::verbs::verb_list()); // @eprintln-ok: final user-facing error print at CLI top level
        return Ok(2);
    }
    let Some(target) = target else {
        eprintln!("usage: dl q {verb} <name> [--limit N] [--offset M]"); // @eprintln-ok: usage/help text
        return Ok(2);
    };
    let daemon_target = root::daemon_target()?;
    let root_opt = daemon_target.root();
    if daemon_first(&o) && daemon::status(None)?.is_some() {
        let ans = daemon::q(root_opt, &verb, &target, o.limit, o.offset)?;
        print_answer(&ans.columns, &ans.rows, ans.total, &ans.notes, o.offset);
    } else {
        let cwd = std::env::current_dir()?;
        let root = cwd.canonicalize().unwrap_or(cwd);
        let out = crate::verbs::run_inproc(root, o.db.as_deref(), &verb, &target, o.limit, o.offset)?;
        print_answer(&out.columns, &out.rows, out.total, &out.notes, o.offset);
    }
    Ok(0)
}

/// Print the `dl q` verb table (the bare-`dl q` help).
fn list_verbs() {
    println!("dl q <verb> <name> — concept verbs over the code graph:");
    for spec in crate::verbs::verb_specs() {
        println!("  {:<16} {:<8} {}", spec.name, spec.args, spec.doc);
    }
    println!("(also queryable as `? verb_catalog(verb, args, doc).`)");
}

/// `dl summary <path> [--db PATH]`.
pub fn run_summary(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("usage: dl summary <path> [--db PATH]"); // @eprintln-ok: usage/help text
        eprintln!("  report entities, imports, callable fan, and documentation for a file"); // @eprintln-ok: usage/help text
        return Ok(0);
    }
    let o = parse_opts(args);
    let Some(path) = o.positional.clone() else {
        eprintln!("usage: dl summary <path> [--db PATH]"); // @eprintln-ok: usage/help text
        return Ok(2);
    };
    let target = root::daemon_target()?;
    let root_opt = target.root();
    if daemon_first(&o) && daemon::status(None)?.is_some() {
        let ans = daemon::summary(root_opt, &path)?;
        print_answer(&ans.columns, &ans.rows, ans.total, &ans.notes, None);
    } else {
        let eng = warm_engine(&o)?;
        let out: QueryOut = crate::anchor::summary(&eng, &path);
        print_answer(&out.columns, &out.rows, out.total, &out.notes, None);
    }
    Ok(0)
}
