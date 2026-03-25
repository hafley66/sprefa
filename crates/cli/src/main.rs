use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sprefa_config::{load_config, load_config_from, default_config_toml, Config};
use sprefa_js::JsExtractor;
use sprefa_rules::extractor::RuleExtractor;
use sprefa_scan::Scanner;
use sprefa_schema::{init_db, list_repos, count_files_for_repo, count_refs_for_repo, upsert_repo, search_strings};

const README: &str = include_str!("../../../README.md");

#[derive(Parser)]
#[command(
    name = "sprefa",
    about = "Cross-repo code intelligence indexer",
    long_about = "\
sprefa (super-refactor) indexes source files from multiple git repositories \
into a single SQLite database. Every interesting string -- imports, exports, \
dependency names, JSON keys, YAML values -- is extracted with byte-level spans, \
deduplicated, normalized for fuzzy matching, and linked back to its source file.

This enables cross-repo queries like \"who imports this module\" or \"which \
repos reference this config key\" across an entire codebase.

QUICK START:
  sprefa init                    Create sprefa.toml and initialize the DB
  sprefa add /path/to/repo       Register a repo for indexing
  sprefa scan                    Index all registered repos
  sprefa query <term>            Trigram substring search across all strings
  sprefa status                  Show indexed repos with file/ref counts
  sprefa serve                   Start the HTTP daemon

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

DAEMON MODE:
  sprefa serve starts an HTTP server (default 127.0.0.1:9400). When
  [daemon].url is set in config, CLI commands delegate to the daemon
  instead of opening the DB directly.

DATABASE:
  SQLite with FTS5 trigram indexes for substring search. Location is
  configured in [db].path (default ~/.sprefa/index.db). WAL mode is
  enabled for concurrent reads.",
    after_help = "Use --readme to print the full project documentation."
)]
struct Cli {
    /// Path to config file (overrides discovery)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Print the full README documentation and exit
    #[arg(long)]
    readme: bool,

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
    },

    /// Trigram substring search across all indexed strings
    ///
    /// Uses SQLite FTS5 with trigram tokenization. The search term is matched
    /// as a substring against normalized string values. Returns up to 100 results
    /// ranked by relevance.
    Query {
        /// Search term (minimum 3 characters for trigram match)
        term: String,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sprefa=info".into()),
        )
        .init();

    let cli = Cli::parse();

    if cli.readme {
        print!("{}", README);
        return Ok(());
    }

    match cli.command {
        Some(Command::Init) => cmd_init().await?,
        Some(Command::Add { path, name }) => cmd_add(&cli.config, path, name).await?,
        Some(Command::Scan { repo }) => cmd_scan(&cli.config, repo.as_deref()).await?,
        Some(Command::Status) => cmd_status(&cli.config).await?,
        Some(Command::Query { term }) => cmd_query(&cli.config, &term).await?,
        Some(Command::Serve) => cmd_serve(&cli.config).await?,
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

async fn cmd_scan(config_path: &Option<PathBuf>, only_repo: Option<&str>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;

    let rules_path = find_rules_file()?;
    let extractor = RuleExtractor::from_json(&rules_path)
        .or_else(|_| RuleExtractor::from_yaml(&rules_path))
        .map_err(|e| anyhow::anyhow!("failed to load rules from {}: {}", rules_path.display(), e))?;

    let scanner = Scanner {
        extractors: vec![
            Box::new(extractor),
            Box::new(JsExtractor),
        ],
        db: pool,
        normalize_config: config.scan.as_ref().and_then(|s| s.normalize.clone()),
        global_filter: config.filter.clone(),
    };

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
                        "{}/{}: {} files scanned, {} refs inserted",
                        result.repo, result.branch, result.files_scanned, result.refs_inserted
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

async fn cmd_query(config_path: &Option<PathBuf>, term: &str) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;

    let results = search_strings(&pool, term).await?;
    if results.is_empty() {
        println!("no matches for '{}'", term);
        return Ok(());
    }

    for s in &results {
        println!("{:<6} {}", s.id, s.value);
    }
    println!("\n{} results", results.len());

    Ok(())
}

async fn cmd_serve(config_path: &Option<PathBuf>) -> anyhow::Result<()> {
    let config = load_cfg(config_path)?;
    let pool = init_db(&config.db_path()).await?;
    let bind = config.daemon_bind().to_string();
    sprefa_server::serve(pool, &bind).await?;
    Ok(())
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
