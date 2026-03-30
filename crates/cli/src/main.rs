use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sprefa_config::{load_config, load_config_from, default_config_toml, Config};
use sprefa_js::JsExtractor;
use sprefa_rs::RsExtractor;
use sprefa_rules::extractor::RuleExtractor;
use sprefa_scan::Scanner;
use sprefa_schema::{BranchScope, init_db, list_repos, count_files_for_repo, count_refs_for_repo, upsert_repo, search_refs};
use sprefa_watch::plan::{self, PathRewriter};
use sprefa_watch::js_path::JsPathRewriter;
use sprefa_watch::rs_path::RsPathRewriter;

const README: &str = include_str!("../../../README.md");

#[derive(Parser)]
#[command(
    name = "sprefa",
    about = "Cross-repo code intelligence indexer",
    long_about = "\
sprefa (super-refactor) indexes source files from multiple git repositories \
into a single SQLite database and watches for changes. When a file moves or a \
symbol is renamed, sprefa rewrites every affected import path and use statement \
automatically. JS/TS and Rust are both supported.

Every interesting string -- imports, exports, dependency names, JSON keys, \
YAML values -- is extracted with byte-level spans, deduplicated, normalized \
for fuzzy matching, and linked back to its source file.

QUICK START:
  sprefa init                    Create sprefa.toml and initialize the DB
  sprefa add /path/to/repo       Register a repo for indexing
  sprefa daemon                  Scan + watch + serve, all in one process

  That's it. The daemon scans all repos on startup, starts filesystem \
  watchers for auto-rewrite, and runs the HTTP server for queries.

COMMANDS:
  sprefa scan                    Index repos (one-shot, no watching)
  sprefa watch                   Watch and auto-rewrite (no HTTP, no scan)
  sprefa serve                   HTTP server only (no watching, no scan)
  sprefa daemon                  All three combined
  sprefa query <term>            Trigram substring search
  sprefa sql \"<SELECT ...>\"      Run read-only SQL against the index DB
  sprefa status                  Show indexed repos with file/ref counts

TYPICAL WORKFLOWS:
  Full daemon:     sprefa init && sprefa add . && sprefa daemon
  Step-by-step:    sprefa scan && sprefa watch  (separate terminals)
  Re-scan only:    sprefa scan --once
  Skip scan:       sprefa daemon --no-scan      (index already populated)

  The watch loop detects file moves (by content hash), declaration renames \
  (by span proximity diffing), and rewrites all affected references:
    JS/TS:  import paths, named imports, re-exports
    Rust:   use statements (crate::, self::, super:: prefixes preserved)

CONFIG:
  Config is loaded from (first match wins):
    1. $SPREFA_CONFIG environment variable
    2. ./sprefa.toml (current directory)
    3. ~/.config/sprefa/sprefa.toml

  Use -c/--config to override with a specific path.

FILTERING:
  Global filters in [filter] apply to all repos. Per-repo filters in
  [[repos]].filter override globals. Per-branch filters in
  [[repos.branch_overrides]] override repo-level filters.

  Modes: \"exclude\" (default) skips matched globs, \"include\" only indexes
  matched globs.

HTTP DELEGATION:
  When [daemon].url is set in config, the scan and query commands \
  delegate to a running sprefa serve/daemon over HTTP instead of \
  opening the DB directly. Use --once to bypass delegation.

DATABASE:
  SQLite with FTS5 trigram indexes for substring search. Location is
  configured in [db].path (default ~/.sprefa/index.db). WAL mode is
  enabled for concurrent reads.

VERBOSITY:
  Set RUST_LOG=sprefa=debug or RUST_LOG=sprefa=trace for detailed output \
  during watch and scan. Default level is info.",
    after_help = "Use --readme to print the full project documentation."
)]
struct Cli {
    /// Path to config file (overrides discovery)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Print the full README documentation and exit
    #[arg(long)]
    readme: bool,

    /// Emit structured JSON logs instead of human-readable output.
    /// Each log line is a JSON object with timestamp, level, target, span,
    /// and fields. Useful for piping into jq, datadog, or log aggregators.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create sprefa.toml in the current directory and initialize the SQLite database
    Init,

    /// Register a git repository for indexing
    ///
    /// Resolves the path to an absolute path, adds the repo to the database,
    /// and appends a [[repos]] entry to sprefa.toml.
    Add {
        /// Path to the git repository root
        path: PathBuf,
        /// Name for the repo (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Index all registered repos (or a specific one)
    ///
    /// Walks each repo's files through git ls-files, applies filter rules,
    /// runs language-specific extractors, and writes refs to the database.
    Scan {
        /// Only scan this repo (by name)
        #[arg(short, long)]
        repo: Option<String>,
        /// Skip daemon delegation and scan directly (even if [daemon].url is set)
        #[arg(long)]
        once: bool,
    },

    /// Trigram substring search across all indexed strings
    ///
    /// Uses SQLite FTS5 with trigram tokenization. The search term is matched
    /// as a substring against normalized string values. Returns up to 500 results
    /// ranked by relevance.
    Query {
        /// Search term (minimum 3 characters for trigram match)
        term: String,
        /// Filter by branch scope: committed, local, or all (default: all)
        #[arg(long)]
        scope: Option<String>,
        /// Skip daemon delegation and query directly (even if [daemon].url is set)
        #[arg(long)]
        once: bool,
    },

    /// Show indexed repos with file and ref counts
    Status,

    /// Start the HTTP daemon server
    ///
    /// Binds to the address in [daemon].bind (default 127.0.0.1:9400).
    /// Exposes the same operations as the CLI over HTTP:
    ///   GET  /status        - repo summary
    ///   GET  /repos         - list repos
    ///   GET  /query?q=term  - search strings
    ///   POST /scan          - trigger indexing
    Serve,

    /// Watch repos for file changes and auto-rewrite imports
    ///
    /// Monitors registered repos using OS filesystem notifications (fsevents on
    /// macOS, inotify on Linux). Events are debounced into 100ms batches and
    /// classified:
    ///
    ///   File move:    delete+create with matching content hash -> rewrite all
    ///                 import paths (JS/TS) and use statements (Rust) that
    ///                 reference the moved file.
    ///
    ///   Decl rename:  re-extract the changed file, diff declarations by span
    ///                 proximity. If a symbol at the same position changed name,
    ///                 rewrite all importing references.
    ///
    ///   File delete:  log a warning with the count of now-broken references.
    ///
    /// Requires an initial `sprefa scan` to populate the index. The watcher
    /// queries the index to find affected references, so stale indexes produce
    /// stale rewrites. Re-scan periodically or after large branch switches.
    ///
    /// JS/TS rewrites preserve the original import style (with/without extension,
    /// directory index stripping). Rust rewrites preserve prefix style (crate::,
    /// self::, super::) when the new path is expressible that way.
    Watch {
        /// Only watch this repo (by name). Watches all repos if omitted.
        #[arg(short, long)]
        repo: Option<String>,
    },

    /// Run a read-only SQL query against the index database
    ///
    /// Opens the sprefa SQLite database and executes the given SQL statement.
    /// Only SELECT statements are allowed (no INSERT, UPDATE, DELETE, DROP, etc).
    /// Results are printed as tab-separated values with a header row.
    ///
    /// The database location is resolved from config (default ~/.sprefa/index.db).
    ///
    /// Examples:
    ///   sprefa sql "SELECT COUNT(*) FROM refs"
    ///   sprefa sql "SELECT s.value, m.kind FROM strings s JOIN refs r ON r.string_id = s.id JOIN matches m ON m.ref_id = r.id LIMIT 20"
    ///   sprefa sql "SELECT s.value, COUNT(*) as cnt FROM strings s JOIN refs r ON r.string_id = s.id GROUP BY s.value ORDER BY cnt DESC LIMIT 10"
    Sql {
        /// SQL SELECT statement to execute
        sql: String,
    },

    /// All-in-one: scan + watch + serve
    ///
    /// Runs the full pipeline in a single process:
    ///   1. Initial scan of all registered repos (builds/updates the index)
    ///   2. Start filesystem watchers on all repos (auto-rewrite on changes)
    ///   3. Start the HTTP server (query, status, trigger re-scans)
    ///
    /// This is the recommended way to run sprefa in the background. It
    /// replaces the three-command sequence of `scan && watch & serve`.
    ///
    /// The initial scan runs to completion before watchers and the server
    /// start, ensuring the index is populated before any rewrites fire.
    ///
    /// Combine with --json for structured logs suitable for process managers
    /// or log aggregators.
    Daemon {
        /// Only include this repo (by name). Includes all repos if omitted.
        #[arg(short, long)]
        repo: Option<String>,
        /// Skip the initial scan (assume index is already populated)
        #[arg(long)]
        no_scan: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "sprefa=info".into());

    if cli.json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    if cli.readme {
        print!("{}", README);
        return Ok(());
    }

    match cli.command {
        Some(Command::Init) => cmd_init().await?,
        Some(Command::Add { path, name }) => cmd_add(&cli.config, path, name).await?,
        Some(Command::Scan { repo, once }) => cmd_scan(&cli.config, repo.as_deref(), once).await?,
        Some(Command::Status) => cmd_status(&cli.config).await?,
        Some(Command::Query { term, scope, once }) => cmd_query(&cli.config, &term, scope.as_deref(), once).await?,
        Some(Command::Sql { sql }) => cmd_sql(&cli.config, &sql).await?,
        Some(Command::Serve) => cmd_serve(&cli.config).await?,
        Some(Command::Watch { repo }) => cmd_watch(&cli.config, repo.as_deref()).await?,
        Some(Command::Daemon { repo, no_scan }) => cmd_daemon(&cli.config, repo.as_deref(), no_scan).await?,
        None => {
            // No subcommand: print help
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
        }
    }

    Ok(())
}

async fn cmd_init() -> anyhow::Result<()> {
    let config_path = PathBuf::from("sprefa.toml");
    if config_path.exists() {
        println!("sprefa.toml already exists");
    } else {
        std::fs::write(&config_path, default_config_toml())?;
        println!("created sprefa.toml");
    }

    let config: Config = toml::from_str(&default_config_toml())?;
    let db_path = config.db_path();
    let _pool = init_db(&db_path).await?;
    println!("initialized database at {}", db_path);

    Ok(())
}

async fn cmd_add(config_path: &Option<PathBuf>, path: PathBuf, name: Option<String>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;

    let abs_path = std::fs::canonicalize(&path)?;
    let repo_name = name.unwrap_or_else(|| {
        abs_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    });

    let id = upsert_repo(&pool, &repo_name, &abs_path.to_string_lossy()).await?;
    println!("added repo '{}' (id={}) at {}", repo_name, id, abs_path.display());

    // Append to config file
    let config_file = find_config_file(config_path)?;
    let mut content = std::fs::read_to_string(&config_file)?;
    content.push_str(&format!(
        "\n[[repos]]\nname = \"{}\"\npath = \"{}\"\nbranches = [\"main\"]\n",
        repo_name,
        abs_path.display()
    ));
    std::fs::write(&config_file, content)?;
    println!("updated {}", config_file.display());

    Ok(())
}

fn build_scanner(config: &sprefa_config::Config, pool: sqlx::SqlitePool) -> anyhow::Result<Scanner> {
    let rules_path = find_rules_file()?;
    let ruleset = load_ruleset(&rules_path)?;
    let link_rules = ruleset.link_rules.clone();
    let extractor = RuleExtractor::from_ruleset(&ruleset)?;
    Ok(Scanner {
        extractors: Arc::new(vec![
            Box::new(extractor) as Box<dyn sprefa_scan::Extractor>,
            Box::new(JsExtractor),
            Box::new(RsExtractor),
        ]),
        db: pool,
        normalize_config: config.scan.as_ref().and_then(|s| s.normalize.clone()),
        global_filter: config.filter.clone(),
        link_rules,
    })
}

fn load_ruleset(path: &std::path::Path) -> anyhow::Result<sprefa_rules::RuleSet> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse rules from {}: {}", path.display(), e))
}

async fn cmd_scan(config_path: &Option<PathBuf>, only_repo: Option<&str>, once: bool) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;

    if !once {
        if let Some(url) = config.daemon_url() {
            let client = reqwest::Client::new();
            let body = only_repo.map(|r| serde_json::json!({ "repo": r }));
            let req = client.post(format!("{}/scan", url));
            let req = match body {
                Some(b) => req.json(&b),
                None => req,
            };
            let resp = req.send().await?.error_for_status()?;
            let items: Vec<serde_json::Value> = resp.json().await?;
            for item in &items {
                println!(
                    "{}/{}: {} files, {} refs, {} targets resolved, {} links",
                    item["repo"].as_str().unwrap_or(""),
                    item["branch"].as_str().unwrap_or(""),
                    item["files_scanned"].as_u64().unwrap_or(0),
                    item["refs_inserted"].as_u64().unwrap_or(0),
                    item["targets_resolved"].as_u64().unwrap_or(0),
                    item["links_created"].as_u64().unwrap_or(0),
                );
            }
            return Ok(());
        }
    }

    let pool = init_db(&config.db_path()).await?;
    let scanner = build_scanner(&config, pool)?;

    let repos: Vec<_> = config
        .repos
        .iter()
        .filter(|r| only_repo.map(|name| r.name == name).unwrap_or(true))
        .collect();

    if repos.is_empty() {
        if let Some(name) = only_repo {
            anyhow::bail!("no repo named '{}' in config", name);
        } else {
            println!("no repos configured. use `sprefa add <path>` to add one.");
            return Ok(());
        }
    }

    let mut total_files = 0usize;
    let mut total_refs = 0usize;

    for repo in repos {
        for branch in repo.branch_list() {
            match scanner.scan_repo(repo, &branch).await {
                Ok(result) => {
                    println!(
                        "{}/{}: {} files scanned, {} refs inserted, {} targets resolved, {} links",
                        result.repo, result.branch, result.files_scanned, result.refs_inserted,
                        result.targets_resolved, result.links_created
                    );
                    total_files += result.files_scanned;
                    total_refs += result.refs_inserted;
                }
                Err(e) => {
                    tracing::warn!("{}/{}: scan failed: {}", repo.name, branch, e);
                }
            }
        }
    }

    println!("\ntotal: {} files, {} refs", total_files, total_refs);
    Ok(())
}

/// Rules file lookup: $SPREFA_RULES > ./sprefa-rules.json > ./sprefa-rules.yaml
/// > ~/.config/sprefa/rules.json > ~/.config/sprefa/rules.yaml
fn find_rules_file() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("SPREFA_RULES") {
        return Ok(PathBuf::from(path));
    }

    let candidates = [
        PathBuf::from("sprefa-rules.json"),
        PathBuf::from("sprefa-rules.yaml"),
        {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(format!("{}/.config/sprefa/rules.json", home))
        },
        {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(format!("{}/.config/sprefa/rules.yaml", home))
        },
    ];

    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!(
            "no rules file found. set $SPREFA_RULES or create sprefa-rules.json"
        ))
}

async fn cmd_status(config_path: &Option<PathBuf>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;

    // TODO: if daemon URL is configured, delegate via HTTP client
    let pool = init_db(&config.db_path()).await?;
    let repos = list_repos(&pool).await?;

    if repos.is_empty() {
        println!("no repos indexed yet. use `sprefa add <path>` to add one.");
        return Ok(());
    }

    for repo in &repos {
        let files = count_files_for_repo(&pool, repo.id).await?;
        let refs = count_refs_for_repo(&pool, repo.id).await?;
        println!(
            "{:<20} {:>6} files  {:>8} refs  {}",
            repo.name, files, refs, repo.root_path
        );
    }

    Ok(())
}

fn parse_scope(s: Option<&str>) -> anyhow::Result<Option<BranchScope>> {
    match s {
        None => Ok(None), // default: Committed (set in search_refs)
        Some("all") => Ok(Some(BranchScope::All)),
        Some("committed") => Ok(Some(BranchScope::Committed)),
        Some("local") => Ok(Some(BranchScope::Local)),
        Some(other) => anyhow::bail!("unknown scope '{}' (expected: committed, local, all)", other),
    }
}

async fn cmd_query(config_path: &Option<PathBuf>, term: &str, scope: Option<&str>, once: bool) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let scope = parse_scope(scope)?;

    if !once {
        if let Some(url) = config.daemon_url() {
            let client = reqwest::Client::new();
            let mut params: Vec<(&str, &str)> = vec![("q", term)];
            let scope_str;
            if let Some(s) = &scope {
                scope_str = match s {
                    BranchScope::Committed => "committed",
                    BranchScope::Local => "local",
                    BranchScope::All => "all",
                };
                params.push(("scope", scope_str));
            }
            let resp = client
                .get(format!("{}/query", url))
                .query(&params)
                .send()
                .await?
                .error_for_status()?;
            let hits: Vec<sprefa_schema::QueryHit> = resp.json().await?;
            print_query_hits(&hits, term);
            return Ok(());
        }
    }

    let pool = init_db(&config.db_path()).await?;
    let hits = search_refs(&pool, term, scope).await?;
    print_query_hits(&hits, term);
    Ok(())
}

fn print_query_hits(hits: &[sprefa_schema::QueryHit], term: &str) {
    if hits.is_empty() {
        println!("no matches for '{}'", term);
        return;
    }
    for hit in hits {
        println!("{} ({} refs)", hit.value, hit.refs.len());
        for loc in &hit.refs {
            println!("  {}  {}  kind={}", loc.repo, loc.file_path, loc.kind);
        }
    }
    println!("\n{} strings matched", hits.len());
}

async fn cmd_sql(config_path: &Option<PathBuf>, sql: &str) -> anyhow::Result<()> {
    // Block anything that isn't a SELECT.
    let trimmed = sql.trim();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    if !first_word.eq_ignore_ascii_case("SELECT")
        && !first_word.eq_ignore_ascii_case("WITH")
        && !first_word.eq_ignore_ascii_case("EXPLAIN")
        && !first_word.eq_ignore_ascii_case("PRAGMA")
    {
        anyhow::bail!("only SELECT, WITH, EXPLAIN, and PRAGMA statements are allowed");
    }

    // Reject semicolons that could chain a second statement.
    let without_trailing = trimmed.trim_end_matches(';').trim();
    if without_trailing.contains(';') {
        anyhow::bail!("multiple statements are not allowed");
    }

    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;

    let rows: Vec<sqlx::sqlite::SqliteRow> =
        sqlx::query(trimmed).fetch_all(&pool).await?;

    if rows.is_empty() {
        println!("(0 rows)");
        return Ok(());
    }

    // Print header from column names.
    use sqlx::Row;
    let columns = rows[0].columns();
    let col_names: Vec<&str> = columns.iter().map(|c| c.name()).collect();
    println!("{}", col_names.join("\t"));

    // Print each row. Try to decode each column as text; fall back to integer then to raw display.
    use sqlx::Column;
    use sqlx::TypeInfo;
    for row in &rows {
        let vals: Vec<String> = columns
            .iter()
            .map(|col| {
                let idx = col.ordinal();
                let type_name = col.type_info().name();
                match type_name {
                    "INTEGER" | "BIGINT" | "INT" | "INT8" => {
                        row.try_get::<i64, _>(idx)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string())
                    }
                    "REAL" | "DOUBLE" | "FLOAT" => {
                        row.try_get::<f64, _>(idx)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string())
                    }
                    _ => {
                        // TEXT, BLOB, or unknown -- try string first
                        row.try_get::<String, _>(idx)
                            .unwrap_or_else(|_| {
                                row.try_get::<i64, _>(idx)
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|_| "NULL".to_string())
                            })
                    }
                }
            })
            .collect();
        println!("{}", vals.join("\t"));
    }
    println!("\n({} rows)", rows.len());
    Ok(())
}

async fn cmd_serve(config_path: &Option<PathBuf>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;
    let bind = config.daemon_bind().to_string();
    let scanner = build_scanner(&config, pool.clone()).ok().map(Arc::new);
    sprefa_server::serve(pool, scanner, config.repos.clone(), &bind).await?;
    Ok(())
}

async fn cmd_watch(config_path: &Option<PathBuf>, only_repo: Option<&str>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;
    let scanner = build_scanner(&config, pool.clone())?;

    let rewriters: Vec<Box<dyn PathRewriter>> = vec![
        Box::new(JsPathRewriter),
        Box::new(RsPathRewriter),
    ];

    let repos: Vec<_> = config
        .repos
        .iter()
        .filter(|r| only_repo.map(|name| r.name == name).unwrap_or(true))
        .collect();

    if repos.is_empty() {
        if let Some(name) = only_repo {
            anyhow::bail!("no repo named '{}' in config", name);
        } else {
            println!("no repos configured. use `sprefa add <path>` to add one.");
            return Ok(());
        }
    }

    let rewriters = Arc::new(rewriters);
    let _pauses = spawn_watchers(&repos, &pool, &scanner.extractors, &rewriters).await?;

    println!("press ctrl-c to stop");
    tokio::signal::ctrl_c().await?;
    println!("\nshutting down");

    Ok(())
}

async fn cmd_daemon(config_path: &Option<PathBuf>, only_repo: Option<&str>, no_scan: bool) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;
    let scanner = build_scanner(&config, pool.clone())?;

    let repos: Vec<_> = config
        .repos
        .iter()
        .filter(|r| only_repo.map(|name| r.name == name).unwrap_or(true))
        .collect();

    if repos.is_empty() {
        if let Some(name) = only_repo {
            anyhow::bail!("no repo named '{}' in config", name);
        } else {
            println!("no repos configured. use `sprefa add <path>` to add one.");
            return Ok(());
        }
    }

    // Phase 1: initial scan (committed branches + working-tree branches)
    if !no_scan {
        tracing::info!(phase = "initial_scan", repo_count = repos.len(), "starting initial scan");
        let mut total_files = 0usize;
        let mut total_refs = 0usize;
        for repo in &repos {
            for branch in repo.branch_list() {
                // Committed scan
                match scanner.scan_repo(repo, &branch).await {
                    Ok(result) => {
                        println!(
                            "scan {}/{}: {} files, {} refs, {} targets",
                            result.repo, result.branch, result.files_scanned,
                            result.refs_inserted, result.targets_resolved,
                        );
                        total_files += result.files_scanned;
                        total_refs += result.refs_inserted;
                        // Persist HEAD sha for future incremental scans.
                        if let Some(sha) = &result.new_git_hash {
                            let rid = upsert_repo(&pool, &repo.name, &repo.path).await.ok();
                            if let Some(rid) = rid {
                                let _ = sprefa_schema::upsert_repo_branch(&pool, rid, &branch, Some(sha)).await;
                            }
                        }
                    }
                    Err(e) => tracing::warn!(repo = %repo.name, branch = %branch, error = %e, "scan failed"),
                }
                // Working-tree scan (same files, tagged under +wt branch)
                let wt = sprefa_watch::wt_branch(&branch);
                match scanner.scan_repo(repo, &wt).await {
                    Ok(result) => {
                        println!(
                            "scan {}/{}: {} files, {} refs, {} targets",
                            result.repo, result.branch, result.files_scanned,
                            result.refs_inserted, result.targets_resolved,
                        );
                    }
                    Err(e) => tracing::warn!(repo = %repo.name, branch = %wt, error = %e, "wt scan failed"),
                }
            }
        }
        tracing::info!(phase = "initial_scan_complete", total_files, total_refs, "scan done");
    } else {
        tracing::info!(phase = "initial_scan_skipped", "skipping initial scan (--no-scan)");
    }

    // Phase 2: start watchers
    let rewriters: Arc<Vec<Box<dyn PathRewriter>>> = Arc::new(vec![
        Box::new(JsPathRewriter),
        Box::new(RsPathRewriter),
    ]);

    let pauses = spawn_watchers(&repos, &pool, &scanner.extractors, &rewriters).await?;

    // Phase 3: start ghcache subscriber (if configured)
    let scanner_arc = Arc::new(scanner);
    if let Some(ghcache) = &config.ghcache {
        let ghcache_db = ghcache.db_path();
        let scanner_for_sub = scanner_arc.clone();
        let pauses_for_sub = pauses;
        let sources = config.sources.clone();
        tokio::spawn(async move {
            if let Err(e) = ghcache_subscribe(
                &ghcache_db,
                &scanner_for_sub,
                &pauses_for_sub,
                &sources,
            ).await {
                tracing::error!(error = %e, "ghcache subscriber exited");
            }
        });
    }

    // Phase 4: start HTTP server (blocks until shutdown)
    let bind = config.daemon_bind().to_string();
    tracing::info!(phase = "server_starting", bind = %bind, "starting HTTP server");
    sprefa_server::serve(pool, Some(scanner_arc), config.repos.clone(), &bind).await?;

    Ok(())
}

/// Pause flag for a single repo's watcher, keyed by repo name.
type PauseFlags = std::collections::HashMap<String, Arc<std::sync::atomic::AtomicBool>>;

/// Start a watcher + rewrite loop for each repo. Shared by cmd_watch and cmd_daemon.
/// Returns the per-repo pause flags so callers can suppress watcher activity
/// during external checkout updates.
async fn spawn_watchers(
    repos: &[&sprefa_config::RepoConfig],
    pool: &sqlx::SqlitePool,
    extractors: &Arc<Vec<Box<dyn sprefa_scan::Extractor>>>,
    rewriters: &Arc<Vec<Box<dyn PathRewriter>>>,
) -> anyhow::Result<PauseFlags> {
    let mut pauses = PauseFlags::new();
    for repo in repos {
        let abs_path = std::fs::canonicalize(&repo.path)?;
        let repo_id = upsert_repo(pool, &repo.name, &abs_path.to_string_lossy()).await?;

        let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
        pauses.insert(repo.name.clone(), pause.clone());

        let wt = repo.branch_list().first().map(|b| sprefa_watch::wt_branch(b));
        let watch_config = sprefa_watch::watcher::WatchConfig {
            root_path: abs_path.clone(),
            repo_id,
            debounce: Duration::from_millis(100),
            wt_branch: wt,
            pause,
        };

        let mut rx = sprefa_watch::watcher::watch(
            watch_config,
            pool.clone(),
            extractors.clone(),
        )
        .await?;

        let pool = pool.clone();
        let rewriters = rewriters.clone();
        let repo_name = repo.name.clone();

        tokio::spawn(async move {
            while let Some(changes) = rx.recv().await {
                tracing::info!(
                    repo = %repo_name, phase = "changes_detected",
                    change_count = changes.len(),
                    "batch received"
                );
                for change in &changes {
                    tracing::info!(repo = %repo_name, phase = "change_detail", ?change);
                }
                match plan::plan_rewrites(&pool, &changes, &rewriters).await {
                    Ok(edits) if edits.is_empty() => {
                        tracing::info!(repo = %repo_name, phase = "plan_complete", edit_count = 0, "no rewrites needed");
                    }
                    Ok(edits) => {
                        tracing::info!(repo = %repo_name, phase = "plan_complete", edit_count = edits.len(), "edits planned");
                        for edit in &edits {
                            tracing::info!(
                                repo = %repo_name, phase = "edit_detail",
                                file = %edit.file_path,
                                span_start = edit.span_start, span_end = edit.span_end,
                                reason = ?edit.reason,
                            );
                        }
                        let result = sprefa_watch::rewrite::apply(&edits);
                        for path in &result.rewritten {
                            tracing::info!(repo = %repo_name, phase = "rewrite_applied", file = %path);
                        }
                        for (edit, err) in &result.failed {
                            tracing::error!(
                                repo = %repo_name, phase = "rewrite_failed",
                                file = %edit.file_path, error = %err,
                            );
                        }
                    }
                    Err(e) => tracing::error!(repo = %repo_name, phase = "plan_error", error = %e),
                }
            }
        });

        tracing::info!(repo = %repo.name, path = %abs_path.display(), "watching");
    }
    Ok(pauses)
}

fn load_cfg(config_path: &Option<PathBuf>) -> anyhow::Result<Config> {
    match config_path {
        Some(p) => Ok(load_config_from(p)?),
        None => {
            let (config, _path) = load_config()?;
            Ok(config)
        }
    }
}

fn find_config_file(config_path: &Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match config_path {
        Some(p) => Ok(p.clone()),
        None => {
            let (_config, path) = load_config()?;
            Ok(path)
        }
    }
}

/// Subscribe to ghcache checkout events and rescan repos when their staging
/// directory changes. Pauses the watcher during rescan to avoid spurious
/// rewrite activity from the git checkout churn.
async fn ghcache_subscribe(
    ghcache_db: &str,
    scanner: &Arc<Scanner>,
    pauses: &PauseFlags,
    sources: &[sprefa_config::SourceConfig],
) -> anyhow::Result<()> {
    use std::sync::atomic::Ordering;

    tracing::info!(db = ghcache_db, "subscribing to ghcache checkout events");

    let subscriber = ghcache_client::Subscriber::new(ghcache_db)
        .interval(Duration::from_millis(500));

    subscriber.subscribe(|events| {
        let scanner = scanner.clone();
        let pauses = pauses.clone();
        let sources = sources.to_vec();
        async move {
            for event in events {
                if event.entity_type != "checkout" {
                    continue;
                }

                let repo_slug = match &event.repo_slug {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let branch = event.payload.get("branch")
                    .and_then(|v| v.as_str())
                    .unwrap_or("main")
                    .to_string();
                let local_path = match event.payload.get("local_path").and_then(|v| v.as_str()) {
                    Some(p) => p.to_string(),
                    None => {
                        tracing::warn!(
                            repo = %repo_slug, branch = %branch,
                            "checkout event missing local_path in payload, skipping"
                        );
                        continue;
                    }
                };

                // Match against source patterns to see if we should index this checkout.
                let repo_name = repo_slug.split('/').last().unwrap_or(&repo_slug);
                if !sources.is_empty() && !source_matches_checkout(&sources, &repo_slug, &branch) {
                    tracing::debug!(
                        repo = %repo_slug, branch = %branch,
                        "checkout does not match any source pattern, skipping"
                    );
                    continue;
                }

                tracing::info!(
                    repo = %repo_slug, branch = %branch, path = %local_path,
                    event = %event.event,
                    "ghcache checkout event, rescanning"
                );

                // Pause watcher if one exists for this repo.
                if let Some(flag) = pauses.get(repo_name) {
                    flag.store(true, Ordering::Relaxed);
                }

                // Build a temporary RepoConfig for the checkout.
                let repo_config = sprefa_config::RepoConfig {
                    name: repo_name.to_string(),
                    path: local_path.clone(),
                    branches: Some(vec![branch.clone()]),
                    filter: None,
                    branch_overrides: None,
                };

                // Scan committed branch (incremental if we have a previous sha).
                let repo_id = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM repos WHERE name = ?"
                ).bind(repo_name).fetch_optional(&scanner.db).await.ok().flatten();

                let prev_sha = match repo_id {
                    Some(rid) => sprefa_schema::get_repo_branch_hash(&scanner.db, rid, &branch)
                        .await.ok().flatten(),
                    None => None,
                };

                let scan_result = match &prev_sha {
                    Some(sha) => {
                        tracing::info!(
                            repo = %repo_slug, branch = %branch, old_sha = %sha,
                            "incremental scan_diff"
                        );
                        match scanner.scan_diff(&repo_config, &branch, sha).await {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                tracing::warn!(
                                    repo = %repo_slug, branch = %branch, error = %e,
                                    "scan_diff failed, falling back to full scan"
                                );
                                scanner.scan_repo(&repo_config, &branch).await
                            }
                        }
                    }
                    None => scanner.scan_repo(&repo_config, &branch).await,
                };

                match &scan_result {
                    Ok(r) => {
                        tracing::info!(
                            repo = %repo_slug, branch = %branch,
                            files = r.files_scanned, refs = r.refs_inserted,
                            deleted = r.files_deleted, renamed = r.files_renamed,
                            "committed scan complete"
                        );
                        // Persist the new HEAD sha for future incremental scans.
                        if let Some(sha) = &r.new_git_hash {
                            let rid = match repo_id {
                                Some(rid) => Some(rid),
                                None => sqlx::query_scalar::<_, i64>(
                                    "SELECT id FROM repos WHERE name = ?"
                                ).bind(repo_name).fetch_optional(&scanner.db).await.ok().flatten(),
                            };
                            if let Some(rid) = rid {
                                let _ = sprefa_schema::upsert_repo_branch(
                                    &scanner.db, rid, &branch, Some(sha),
                                ).await;
                            }
                        }
                    }
                    Err(e) => tracing::error!(
                        repo = %repo_slug, branch = %branch, error = %e,
                        "committed scan failed"
                    ),
                }

                // Scan working-tree branch.
                let wt = sprefa_watch::wt_branch(&branch);
                match scanner.scan_repo(&repo_config, &wt).await {
                    Ok(r) => tracing::info!(
                        repo = %repo_slug, branch = %wt,
                        files = r.files_scanned, refs = r.refs_inserted,
                        "wt scan complete"
                    ),
                    Err(e) => tracing::error!(
                        repo = %repo_slug, branch = %wt, error = %e,
                        "wt scan failed"
                    ),
                }

                // Unpause watcher.
                if let Some(flag) = pauses.get(repo_name) {
                    flag.store(false, Ordering::Relaxed);
                    tracing::info!(repo = %repo_name, "watcher unpaused after rescan");
                }
            }
            Ok(())
        }
    }).await?;

    Ok(())
}

/// Check whether a checkout (repo_slug + branch) matches any configured source.
/// A source matches if it has no branch_patterns (open policy) or the branch
/// matches at least one glob pattern.
fn source_matches_checkout(
    sources: &[sprefa_config::SourceConfig],
    _repo_slug: &str,
    branch: &str,
) -> bool {
    for source in sources {
        if source.branch_patterns.is_empty() {
            return true;
        }
        for pattern in &source.branch_patterns {
            if glob_match(pattern, branch) {
                return true;
            }
        }
    }
    false
}

/// Simple glob match supporting `*` (any segment chars) and `**` is not needed
/// since branch names are flat. Uses the `glob` crate's Pattern matching.
fn glob_match(pattern: &str, value: &str) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches(value))
        .unwrap_or(false)
}
