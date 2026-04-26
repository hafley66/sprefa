//! sprefa-run — smallest v3 end-to-end driver.
//!
//! Usage:
//!   sprefa-run <file.sprf> --root <dir> [--rev <rev>]
//!
//! Supported ops (Stage A surface): repo, rev, fs, void.
//! Unsupported ops skip with a warning so a partial pipe still runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::{BTreeSet, HashMap, HashSet};

use effect_runtime::{RtCtx, RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
use pipeline::_0_cursor::{CaptureKind, Cursor};
use pipeline::_2_pipeline::Pipeline;
use effect_runtime::batchers::{BoundedWorkSteal, CacheLayer};
use pipeline::effects::{
    ast_parse, AstParseEffect, FsListFilesBatcher, FsListFilesEffect,
    PrintBatcher, PrintEffect, ReadBytesBatchBatcher, ReadBytesBatchEffect,
    ReadBytesBatcher, ReadBytesEffect,
};
use pipeline::cache_key::OpCache;
use pipeline::ops::{RuleCallOp, RulePredicateOp};
use pipeline::rule_def;
use pipeline::rule_hash::{self, RuleHashes};
use pipeline::ops::relation::RelationArg;
use pipeline::registry::lower_paren_slot;
use pipeline::relation_store::{RelationStore, RelationWake, WriteBatcher, WriteEffect};
use pipeline::readers::FileSource;
use pipeline::registry::Registry;
use pipeline::value::{TermMode, Value};
use server::config::{self, Config, Seed};
use sprefa_parse::{host_parse_with_injections, OpInvocation, Pipe, PipeStep};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        eprintln!(
            "usage: sprefa-run <file.sprf> [--root <dir>] [--rev <rev>] [--config <.sprefa.toml>]"
        );
        std::process::exit(2);
    }
    let sprf_path = PathBuf::from(&args[0]);
    let mut root: Option<PathBuf> = None;
    let mut rev: String = "HEAD".to_string();
    let mut config_path: Option<PathBuf> = None;
    let mut out_db: Option<PathBuf> = None;
    let mut no_cache: bool = false;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = it.next().map(PathBuf::from),
            "--rev" => rev = it.next().cloned().unwrap_or("HEAD".into()),
            "--config" => config_path = it.next().map(PathBuf::from),
            "--out" => out_db = it.next().map(PathBuf::from),
            "--no-cache" => no_cache = true,
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
    }

    let source = std::fs::read_to_string(&sprf_path).unwrap_or_else(|e| {
        eprintln!("read {sprf_path:?}: {e}");
        std::process::exit(1);
    });

    // Precedence: explicit --config > CLI --root/--rev.
    // CLI flags (--out, --no-cache) overlay onto cfg.run so every
    // driver (CLI here, LSP/HTTP later) reads the same shape.
    let mut cfg = match config_path {
        Some(p) => config::from_path(&p).unwrap_or_else(|e| {
            eprintln!("config error: {e}");
            std::process::exit(1);
        }),
        None => {
            let root = root.unwrap_or_else(|| std::env::current_dir().unwrap());
            config::from_cli(root, rev.clone())
        }
    };
    if let Some(p) = out_db { cfg.run.out_db = Some(p); }
    if no_cache { cfg.run.cache = false; }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        run(&source, &sprf_path, &cfg).await
    });
}

async fn run(source: &str, file: &Path, cfg: &Config) {
    let (parsed, errors) = host_parse_with_injections(
        source,
        Arc::from(file),
        &pipeline::op_languages::language_of,
    );
    for e in &errors {
        eprintln!(
            "parse: {:?} at {}..{}: {}",
            e.kind, e.byte_range.start, e.byte_range.end, e.message
        );
    }
    if parsed.pipes.is_empty() {
        std::process::exit(1);
    }

    let registry = Registry::with_stdlib();
    // RelationStore lives at run scope: rule defs, rule rows, and
    // SubjectRegistry pending entries persist across seeds so the SQLite
    // drain at end-of-run sees every relation row produced.
    let relation_store = Arc::new(RelationStore::new());
    let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());

    // Rule defs + sprf_meta hashes are deterministic from source and
    // shared across seeds. Compute once before the seed loop so the
    // per-seed pass-1 picks them up and the SQLite drain reuses them.
    let file_arc: Arc<Path> = Arc::from(file);
    let rule_defs = rule_def::collect(&parsed, source.as_bytes(), &registry, &file_arc);
    let rec_errors = rule_def::self_recursion_errors(&rule_defs);
    if !rec_errors.is_empty() {
        for e in &rec_errors {
            eprintln!(
                "parse: {:?} at {}..{}: {}",
                e.kind, e.byte_range.start, e.byte_range.end, e.message,
            );
        }
        std::process::exit(1);
    }
    let rule_hashes = rule_hash::compute(&rule_defs, source.as_bytes());

    for seed in &cfg.seeds {
        // One RtCtx per seed. The FsListFilesEffect batcher is bound to
        // the seed's (root, rev) — the effect runtime caches listings
        // by (repo, rev), so repeat `fs(...)` calls in the same seed
        // amortize one fs walk across all call-sites. PrintBatcher
        // targets stdout so `print()` ops surface in the CLI output.
        let file_source: Arc<dyn FileSource> =
            Arc::new(DiskFileSource::new(seed.root.clone(), seed.rev.clone()));
        // ast parse worker pool: 8 rayon threads, inbox cap 256.
        // Mirrors FINDINGS §5.1's W=8, cap=256 sweet spot for the
        // kernel printk scan. Tunable via env later.
        let ast_workers: usize = std::env::var("SPREFA_AST_WORKERS")
            .ok().and_then(|s| s.parse().ok())
            .unwrap_or(8);
        let ast_cap: usize = std::env::var("SPREFA_AST_CAP")
            .ok().and_then(|s| s.parse().ok())
            .unwrap_or(256);
        // Build CacheLayer<ReadBytesEffect> by hand so the run loop can
        // surface its hit/miss counters in stderr after each pipe — the
        // sugar `register_pure` swallows the layer handle.
        let listing_cache: CacheLayer<FsListFilesEffect> =
            CacheLayer::new(1024, FsListFilesBatcher::new(file_source.clone()));
        let read_cache: CacheLayer<ReadBytesEffect> =
            CacheLayer::new(65_536, ReadBytesBatcher::new(file_source.clone()));
        let op_cache = Arc::new(OpCache::new(cfg.run.cache));
        let ctx = RtCtxBuilder::new()
            .with_store(relation_store.clone())
            .with_store(subject_registry.clone())
            .with_store(op_cache.clone())
            .register_domain_aware::<FsListFilesEffect, _>(listing_cache.clone())
            .register_domain_aware::<ReadBytesEffect, _>(read_cache.clone())
            .register::<ReadBytesBatchEffect, _>(
                ReadBytesBatchBatcher::new(file_source),
            )
            .register::<AstParseEffect, _>(
                BoundedWorkSteal::<AstParseEffect>::new(
                    ast_cap, ast_workers, ast_parse,
                ),
            )
            .register::<PrintEffect, _>(PrintBatcher::stdout())
            .register::<Yield<RelationWake>, _>(YieldBatcher::new(subject_registry.clone()))
            .register::<WriteEffect, _>(
                WriteBatcher::new(relation_store.clone(), subject_registry.clone()),
            )
            .build();
        let seed_tag = if cfg.seeds.len() > 1 {
            format!("[{}] ", seed.slug)
        } else {
            String::new()
        };

        // Pass 1: scan top-level pipes for rule definitions. Metadata
        // (name + params + brace range) comes from the shared
        // `rule_def::collect` walker also used by `server::DocSession`.
        // Body Pipeline lowering is sprefa-run's add-on (DocSession just
        // needs the metadata for hover).
        let mut known_rules: HashSet<Arc<str>> = HashSet::new();
        let mut auto_fire: Vec<Arc<str>> = Vec::new();
        for def in rule_defs.values() {
            let Some(brace_range) = def.brace_range.clone() else { continue };
            let Some(body) = lower_rule_body(
                source,
                brace_range,
                &registry,
                &def.name,
                &seed_tag,
            ) else { continue };
            relation_store.bind_params(def.name.clone(), def.params.clone());
            relation_store.bind_body(def.name.clone(), Arc::new(body));
            known_rules.insert(def.name.clone());
            // Top-level rule defs auto-fire once after pass 1 so an
            // arity-0 rule produces rows without an explicit caller.
            // For arity>=1 the auto-fire binds empty-string params; user
            // can still call the rule from another pipe with real args.
            auto_fire.push(def.name.clone());
        }

        for name in &auto_fire {
            let params = relation_store.params(name);
            let args: Vec<RelationArg> = params
                .iter()
                .map(|_| RelationArg::Literal(Arc::from("")))
                .collect();
            let call = Pipeline::Op(Arc::new(RuleCallOp::new(name.clone(), args)));
            run_pipeline(
                &format!("{seed_tag}rule {name} (auto)"),
                Pipeline::Seq(vec![call]),
                &ctx,
                seed,
            ).await;
        }

        // Pass 2: run non-rule top-level pipes, lowering with rule-call
        // awareness for bare-name refs.
        for (i, pipe) in parsed.pipes.iter().enumerate() {
            let Some(head) = pipe.ops.first() else { continue };
            if &*head.name == "rule" { continue; }
            let Some(p) = lower_pipe(pipe, source, &registry, &known_rules) else {
                continue;
            };
            run_pipeline(&format!("{seed_tag}pipe {i}"), p, &ctx, seed).await;
        }
        // Cache telemetry: dump hit/miss snapshot to stderr so the
        // wall numbers in `time` are paired with the cache state that
        // produced them. SPREFA_TELEMETRY=quiet suppresses.
        if std::env::var("SPREFA_TELEMETRY").as_deref() != Ok("quiet") {
            let r = read_cache.stats();
            let l = listing_cache.stats();
            eprintln!(
                "# cache: read[hits={} misses={} entries={} hit%={:.1}] listing[hits={} misses={} entries={}]",
                r.hits, r.misses, r.entries, r.hit_ratio() * 100.0,
                l.hits, l.misses, l.entries,
            );
        }
    }

    // SQLite drain: dump rule rows. One table per rule, columns =
    // union of all capture names seen across rows for that rule, all
    // VARCHAR. Schema discovered at drain time (no fixed shape).
    let db_path = cfg.run.out_db.clone().unwrap_or_else(|| default_out_db(file));
    if let Err(e) = dump_to_sqlite(
        &relation_store, &db_path, file, &rule_hashes, &cfg.run,
    ).await {
        eprintln!("sqlite drain: {e}");
        std::process::exit(1);
    }
}

/// Drive a pre-lowered Pipeline against a single seed cursor. Prints
/// terminal rows and a row-count summary header.
async fn run_pipeline(header: &str, pipeline: Pipeline, ctx: &RtCtx, seed: &Seed) {
    let mut c = Cursor::default();
    c.repo = Arc::from(seed.slug.as_str());
    c.rev = Arc::from(seed.rev.as_str());
    let upstream: futures::stream::BoxStream<'_, std::sync::Arc<[Cursor]>> = Box::pin(
        futures::stream::iter(vec![std::sync::Arc::<[Cursor]>::from(vec![c])]),
    );
    let mut s = pipeline.run(ctx, upstream);
    let mut total = 0usize;
    println!("{header}");
    while let Some(b) = futures::StreamExt::next(&mut s).await {
        for c in b.iter() {
            total += 1;
            print_row(c);
        }
    }
    println!("{header} — {} rows", total);
}

/// Lower one top-level (non-rule) Pipe to a Pipeline. Steps with names
/// in `known_rules` build [`RuleCallOp`]; steps with names registered
/// in the stdlib build via the registry; unregistered names print a
/// skip warning.
fn lower_pipe(
    pipe:        &Pipe,
    source:      &str,
    registry:    &Registry,
    known_rules: &HashSet<Arc<str>>,
) -> Option<Pipeline> {
    let mut steps: Vec<Pipeline> = Vec::new();
    for step in &pipe.steps {
        match step {
            PipeStep::Op(inv) => {
                if let Some(p) = lower_op_invocation(inv, source, registry, known_rules) {
                    steps.push(p);
                }
            }
            PipeStep::Fork(arms) => {
                let lowered_arms: Vec<Pipeline> = arms
                    .iter()
                    .filter_map(|a| lower_pipe(a, source, registry, known_rules))
                    .collect();
                if !lowered_arms.is_empty() {
                    steps.push(Pipeline::Fork(lowered_arms));
                }
            }
        }
    }
    if steps.is_empty() { None } else { Some(Pipeline::Seq(steps)) }
}

/// Lower one `OpInvocation` to a single-step Pipeline. Same dispatch
/// rules as [`lower_pipe`], factored so rule bodies can recurse without
/// duplicating the rule-call vs registry branching.
fn lower_op_invocation(
    inv:         &OpInvocation,
    source:      &str,
    registry:    &Registry,
    known_rules: &HashSet<Arc<str>>,
) -> Option<Pipeline> {
    // Rule call: bare name, no `?` suffix, present in known_rules and not
    // already registered as a stdlib op (stdlib wins on collision; user
    // intent matches the registered name).
    if !inv.predicate
        && known_rules.contains(&inv.name)
    {
        let args = lower_call_args(inv, source, registry);
        return Some(Pipeline::Op(Arc::new(RuleCallOp::new(inv.name.clone(), args))));
    }
    // Rule predicate: bare name with `?` suffix, present in known_rules.
    // Classify args by bound/unbound and emit RulePredicateOp.
    if inv.predicate && known_rules.contains(&inv.name) {
        let values = lower_predicate_values(inv, source, registry);
        return match RulePredicateOp::classify(inv.name.clone(), values) {
            Ok(op) => Some(Pipeline::Op(Arc::new(op))),
            Err(errs) => {
                for d in &errs {
                    eprintln!("lower {}?: {}: {}", inv.name, d.code, d.message);
                }
                std::process::exit(1);
            }
        };
    }
    let lookup_name: Arc<str> = if inv.predicate {
        Arc::from(format!("{}?", inv.name))
    } else {
        inv.name.clone()
    };
    let mut diags = Vec::new();
    match registry.build_from_node(&lookup_name, inv.node(), source.as_bytes(), &mut diags) {
        Some(Ok(op)) => {
            if !diags.is_empty() {
                for d in &diags {
                    eprintln!("lower {}: {}: {}", inv.name, d.code, d.message);
                }
                std::process::exit(1);
            }
            Some(Pipeline::Op(op))
        }
        Some(Err(errs)) => {
            for d in &errs {
                eprintln!("lower {}: {}: {}", inv.name, d.code, d.message);
            }
            std::process::exit(1);
        }
        None => {
            eprintln!("skip step (unregistered): name={}", inv.name);
            None
        }
    }
}

/// Lower the paren-args of a rule-predicate site to raw `Value`s for
/// [`RulePredicateOp::classify`]. Mirrors `lower_paren_slot` but returns
/// the unclassified value list so the op's classifier can decide
/// Probe / Join / Query.
fn lower_predicate_values(
    inv:      &OpInvocation,
    source:   &str,
    registry: &Registry,
) -> Vec<Value> {
    let Some(paren) = inv.node().child_by_field_name("paren") else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    lower_paren_slot(paren, source.as_bytes(), registry, &mut diags)
}

/// Lower the paren-args of a rule-call site to `RelationArg` values.
/// `${X}` → `Capture(X)`, atoms / strings / ints / floats → `Literal`.
/// Op invocations and `${X?}` introducers in call position are not
/// supported for v0.
fn lower_call_args(
    inv:      &OpInvocation,
    source:   &str,
    registry: &Registry,
) -> Vec<RelationArg> {
    let Some(paren) = inv.node().child_by_field_name("paren") else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    let values = lower_paren_slot(paren, source.as_bytes(), registry, &mut diags);
    let mut args: Vec<RelationArg> = Vec::with_capacity(values.len());
    for v in values {
        match v {
            Value::Term { name, mode: TermMode::Read } => {
                args.push(RelationArg::Capture(name));
            }
            Value::Atom(s) | Value::Str(s) => args.push(RelationArg::Literal(s)),
            Value::Int(n) => args.push(RelationArg::Literal(Arc::from(n.to_string()))),
            Value::Float(n) => args.push(RelationArg::Literal(Arc::from(n.to_string()))),
            Value::Term { name, mode: TermMode::Unbound } => {
                eprintln!(
                    "rule-call: ${{{name}?}} introducer in call position is not supported"
                );
            }
            Value::Op(_) => {
                eprintln!("rule-call: nested op in call args is not supported");
            }
        }
    }
    args
}

/// Lower a rule body (the slice between `{` and `}`) to a runnable
/// Pipeline. Body parsing reuses the host parser with injections
/// enabled so nested pattern ops (re, glob) inside the body lower
/// correctly. Self-recursion is the only forward-resolvable rule ref
/// inside a body; cross-rule refs work only at pipe-time when
/// RelationStore::body() is populated for both rules.
fn lower_rule_body(
    source:      &str,
    brace_range: std::ops::Range<usize>,
    registry:    &Registry,
    name:        &Arc<str>,
    seed_tag:    &str,
) -> Option<Pipeline> {
    let brace_offset = brace_range.start;
    let brace_src = source.get(brace_range)?;
    let (sub, sub_errs) = host_parse_with_injections(
        brace_src,
        Arc::from(Path::new("<rule-body>")),
        &pipeline::op_languages::language_of,
    );
    for e in &sub_errs {
        let start = e.byte_range.start + brace_offset;
        let end = e.byte_range.end + brace_offset;
        eprintln!(
            "{seed_tag}rule {name} body parse: {:?} at {}..{}: {}",
            e.kind, start, end, e.message
        );
    }
    let mut body_known_rules: HashSet<Arc<str>> = HashSet::new();
    body_known_rules.insert(name.clone());

    let lowered_pipes: Vec<Pipeline> = sub
        .pipes
        .iter()
        .filter_map(|p| lower_pipe(p, brace_src, registry, &body_known_rules))
        .collect();
    match lowered_pipes.len() {
        0 => None,
        1 => Some(lowered_pipes.into_iter().next().unwrap()),
        _ => Some(Pipeline::Fork(lowered_pipes)),
    }
}


// ---------------------------------------------------------------------------
// SQLite drain
// ---------------------------------------------------------------------------

/// Default db file path: `<sprf_path with .db extension>`. Honors
/// the parent dir of the .sprf file so multiple .sprf files in the
/// same dir get sibling .db files.
fn default_out_db(sprf_path: &Path) -> PathBuf {
    let stem = sprf_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sprefa".to_string());
    let parent = sprf_path.parent().unwrap_or(Path::new("."));
    parent.join(format!("{stem}.db"))
}

/// File stem of the .sprf source — the prefix every per-rule table
/// gets. `foo.sprf` rules `bar`, `baz` produce tables
/// `foo__bar`, `foo__baz`.
fn db_table_prefix(sprf_path: &Path) -> String {
    sprf_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sprefa".to_string())
}

/// v1 parity normalizer (`crates/config/src/normalize.rs`): strip every
/// non-ASCII-alphanumeric char and lowercase. Collapses naming conventions
/// (`AuthService`, `auth_service`, `auth-service`, `auth.service`) onto a
/// single FTS-friendly form. Used at INSERT to populate the `<col>_norm`
/// columns and as the canonical form a downstream consumer should run a
/// query string through before issuing FTS5 MATCH.
fn sprf_norm(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Drain every rule's `RuleRow`s into a SQLite database (v1 parity shape).
///
/// Per-rule table layout:
///   * name: `<sprf_stem>__<rule_name>`
///   * columns: `id INTEGER PRIMARY KEY AUTOINCREMENT` +
///     `(<cap>, <cap>_norm)` TEXT pair for every distinct capture name
///     seen across rows. `<cap>_norm` is `sprf_norm(<cap>)` computed in
///     Rust at INSERT.
///   * `CREATE INDEX <table>_<cap>_norm_idx ON <table>(<cap>_norm)` per
///     normalized column for equality lookups (`auth_service` → match).
///   * FTS5 trigram virtual table `<table>_fts` shadowing every `_norm`
///     column with `content='<table>'` + auto-sync triggers (ai/ad/au)
///     so fuzzy `MATCH` queries against any normalized value work
///     out-of-the-box (matches v1 `strings_fts` shape).
///
/// PRAGMAs set on the pool: `journal_mode=WAL`, `synchronous=NORMAL`,
/// `foreign_keys=ON`. Tables are dropped + recreated each run; the
/// run is the source of truth for its rule rows.
/// sprf_meta diff classification per rule. Drives the drain path:
/// New + SchemaChanged force DROP+CREATE; ExtractChanged keeps the
/// table and only refreshes rows; Unchanged short-circuits the rule.
#[derive(Debug, Clone, Copy)]
enum MetaDiff { New, SchemaChanged, ExtractChanged, Unchanged }

impl MetaDiff {
    fn label(self) -> &'static str {
        match self {
            MetaDiff::New            => "New",
            MetaDiff::SchemaChanged  => "SchemaChanged",
            MetaDiff::ExtractChanged => "ExtractChanged",
            MetaDiff::Unchanged      => "Unchanged",
        }
    }
}

async fn dump_to_sqlite(
    store:       &RelationStore,
    db_path:     &Path,
    sprf_path:   &Path,
    rule_hashes: &HashMap<Arc<str>, RuleHashes>,
    run:         &server::config::RunOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
    use std::str::FromStr;

    let snapshot = store.rule_rows_snapshot();
    if snapshot.is_empty() {
        eprintln!("# sqlite: no rule rows to drain");
        return Ok(());
    }

    let prefix = db_table_prefix(sprf_path);

    let url = format!("sqlite://{}", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    // sprf_meta: per-rule (schema_hash, extract_hash, last_scanned_at).
    // Schema/extract diff classifies each rule's drain path:
    //   New | SchemaChanged   -> DROP + CREATE + INSERT (full rebuild)
    //   ExtractChanged        -> DELETE + INSERT (schema stable)
    //   Unchanged             -> skip drain entirely
    // `--no-cache` collapses everything to SchemaChanged.
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS sprf_meta (
            rule_name TEXT PRIMARY KEY,
            schema_hash TEXT NOT NULL,
            extract_hash TEXT NOT NULL,
            last_scanned_at TEXT NOT NULL
        )"#
    ).execute(&pool).await?;

    for (rule_name, rows) in &snapshot {
        let table = format!("{prefix}__{rule_name}");
        if !is_safe_ident(&table) {
            eprintln!("sqlite: skipping unsafe table name {table:?}");
            continue;
        }

        // sprf_meta lookup + classify.
        let curr = rule_hashes.get(rule_name);
        let prev: Option<(String, String)> = match curr {
            Some(_) => sqlx::query_as::<_, (String, String)>(
                "SELECT schema_hash, extract_hash FROM sprf_meta WHERE rule_name = ?"
            )
            .bind(rule_name.as_ref())
            .fetch_optional(&pool).await?,
            None => None,
        };
        let diff: MetaDiff = match (run.cache, curr, prev) {
            (false, _, _)            => MetaDiff::SchemaChanged,
            (true, None, _)          => MetaDiff::SchemaChanged,
            (true, Some(_), None)    => MetaDiff::New,
            (true, Some(c), Some((ps, pe))) => {
                if c.schema_hash != ps        { MetaDiff::SchemaChanged }
                else if c.extract_hash != pe  { MetaDiff::ExtractChanged }
                else                          { MetaDiff::Unchanged }
            }
        };
        if matches!(diff, MetaDiff::Unchanged) {
            eprintln!("# sqlite: {table}: cached (Unchanged), skipped");
            continue;
        }

        // Column union, insertion-order preserving. First time a column
        // appears (across all rows in declaration order) fixes its
        // position. BTreeSet guards "have we seen this column" cheaply.
        let mut columns: Vec<Arc<str>> = Vec::new();
        let mut seen: BTreeSet<Arc<str>> = BTreeSet::new();
        for row in rows {
            for (k, _) in row {
                if seen.insert(k.clone()) {
                    columns.push(k.clone());
                }
            }
        }

        let safe_columns: Vec<&Arc<str>> =
            columns.iter().filter(|c| is_safe_ident(c)).collect();
        let unsafe_count = columns.len() - safe_columns.len();
        if unsafe_count > 0 {
            eprintln!(
                "sqlite: rule {rule_name}: dropped {unsafe_count} unsafe column name(s)"
            );
        }

        let full_rebuild = matches!(diff, MetaDiff::SchemaChanged | MetaDiff::New);
        if full_rebuild {
            // Drop FTS first (depends on table), then triggers, then table.
            // sqlx will silently no-op on missing triggers via IF EXISTS.
            for stmt in [
                format!(r#"DROP TRIGGER IF EXISTS "{table}_ai""#),
                format!(r#"DROP TRIGGER IF EXISTS "{table}_ad""#),
                format!(r#"DROP TRIGGER IF EXISTS "{table}_au""#),
                format!(r#"DROP TABLE IF EXISTS "{table}_fts""#),
                format!(r#"DROP TABLE IF EXISTS "{table}""#),
            ] {
                sqlx::query(&stmt).execute(&pool).await?;
            }
        } else {
            // ExtractChanged: schema stable, just truncate rows. FTS
            // mirrors via triggers, so DELETE from base also clears FTS.
            sqlx::query(&format!(r#"DELETE FROM "{table}""#))
                .execute(&pool).await?;
        }

        let mut col_decls: Vec<String> = Vec::new();
        for c in &safe_columns {
            col_decls.push(format!(r#""{c}" TEXT"#));
            col_decls.push(format!(r#""{c}_norm" TEXT"#));
        }
        let cols_sql = if col_decls.is_empty() {
            String::new()
        } else {
            format!(", {}", col_decls.join(", "))
        };
        if full_rebuild {
        let create_sql = format!(
            r#"CREATE TABLE "{table}" (id INTEGER PRIMARY KEY AUTOINCREMENT{cols_sql})"#
        );
        sqlx::query(&create_sql).execute(&pool).await?;

        // Per-norm-column index: equality lookups on the normalized form.
        for c in &safe_columns {
            let idx_sql = format!(
                r#"CREATE INDEX IF NOT EXISTS "{table}_{c}_norm_idx" ON "{table}"("{c}_norm")"#
            );
            sqlx::query(&idx_sql).execute(&pool).await?;
        }

        // FTS5 trigram virtual table mirroring every `_norm` column +
        // auto-sync triggers. Skip when the rule has no captures (arity-0
        // tables have nothing to fuzzy-match).
        if !safe_columns.is_empty() {
            let fts_cols: Vec<String> =
                safe_columns.iter().map(|c| format!(r#""{c}_norm""#)).collect();
            let fts_create = format!(
                r#"CREATE VIRTUAL TABLE "{table}_fts" USING fts5({}, content='{table}', content_rowid='id', tokenize='trigram')"#,
                fts_cols.join(", ")
            );
            sqlx::query(&fts_create).execute(&pool).await?;

            // Trigger: AFTER INSERT — push the just-inserted row's norm
            // columns into FTS. `length(...) < 1000` guard mirrors v1's
            // strings_ai trigger; oversized normalized values would blow
            // the FTS index for marginal search value.
            let cols_csv = fts_cols.join(", ");
            let new_csv = safe_columns
                .iter()
                .map(|c| format!(r#"new."{c}_norm""#))
                .collect::<Vec<_>>()
                .join(", ");
            let length_guard = safe_columns
                .iter()
                .map(|c| format!(r#"length(new."{c}_norm") < 1000"#))
                .collect::<Vec<_>>()
                .join(" AND ");
            let ai = format!(
                r#"CREATE TRIGGER "{table}_ai" AFTER INSERT ON "{table}" BEGIN
                    INSERT INTO "{table}_fts"(rowid, {cols_csv})
                    SELECT new.id, {new_csv} WHERE {length_guard};
                END"#
            );
            sqlx::query(&ai).execute(&pool).await?;

            let old_csv = safe_columns
                .iter()
                .map(|c| format!(r#"old."{c}_norm""#))
                .collect::<Vec<_>>()
                .join(", ");
            let ad = format!(
                r#"CREATE TRIGGER "{table}_ad" AFTER DELETE ON "{table}" BEGIN
                    INSERT INTO "{table}_fts"("{table}_fts", rowid, {cols_csv}) VALUES('delete', old.id, {old_csv});
                END"#
            );
            sqlx::query(&ad).execute(&pool).await?;

            let au = format!(
                r#"CREATE TRIGGER "{table}_au" AFTER UPDATE ON "{table}" BEGIN
                    INSERT INTO "{table}_fts"("{table}_fts", rowid, {cols_csv}) VALUES('delete', old.id, {old_csv});
                    INSERT INTO "{table}_fts"(rowid, {cols_csv})
                    SELECT new.id, {new_csv} WHERE {length_guard};
                END"#
            );
            sqlx::query(&au).execute(&pool).await?;
        }
        } // end if full_rebuild

        if safe_columns.is_empty() {
            // Arity-0 (or all-unsafe-named): one bare INSERT per fire.
            for _ in rows {
                let insert_sql = format!(r#"INSERT INTO "{table}" DEFAULT VALUES"#);
                sqlx::query(&insert_sql).execute(&pool).await?;
            }
        } else {
            let col_list = safe_columns
                .iter()
                .flat_map(|c| {
                    [format!(r#""{c}""#), format!(r#""{c}_norm""#)]
                })
                .collect::<Vec<_>>()
                .join(", ");
            let placeholders = (0..safe_columns.len() * 2)
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            let insert_sql = format!(
                r#"INSERT INTO "{table}" ({col_list}) VALUES ({placeholders})"#
            );
            for row in rows {
                let lookup: HashMap<&Arc<str>, &Arc<str>> =
                    row.iter().map(|(k, v)| (k, v)).collect();
                let mut q = sqlx::query(&insert_sql);
                for c in &safe_columns {
                    let raw: String = lookup
                        .get(c)
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let norm = sprf_norm(&raw);
                    q = q.bind(raw).bind(norm);
                }
                q.execute(&pool).await?;
            }
        }

        if let Some(c) = curr {
            sqlx::query(
                r#"INSERT INTO sprf_meta (rule_name, schema_hash, extract_hash, last_scanned_at)
                   VALUES (?, ?, ?, datetime('now'))
                   ON CONFLICT(rule_name) DO UPDATE SET
                     schema_hash = excluded.schema_hash,
                     extract_hash = excluded.extract_hash,
                     last_scanned_at = excluded.last_scanned_at"#
            )
            .bind(rule_name.as_ref())
            .bind(&c.schema_hash)
            .bind(&c.extract_hash)
            .execute(&pool).await?;
        }

        eprintln!(
            "# sqlite: {table}: {} rows, {} cols (×2 norm), fts={} [{}]",
            rows.len(),
            safe_columns.len(),
            !safe_columns.is_empty(),
            diff.label(),
        );
    }

    pool.close().await;
    eprintln!("# sqlite: wrote {}", db_path.display());
    Ok(())
}

/// SQLite identifier safety: only allow ASCII alphanumerics + `_`,
/// no leading digit. Strict for the v0 drain so quoted-identifier
/// injection paths stay closed; user can grow this later.
fn is_safe_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if bytes[0].is_ascii_digit() { return false; }
    bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn print_row(c: &Cursor) {
    let fs = c
        .fs
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".into());
    let mut caps = Vec::new();
    for cap in &c.captures {
        let val = match &cap.kind {
            CaptureKind::Synthesized { value } => value.to_string(),
            CaptureKind::SpanBacked => {
                String::from_utf8_lossy(&c.content[cap.byte_range.clone()]).into_owned()
            }
        };
        caps.push(format!("{}={}", cap.name, val));
    }
    println!("  {}@{} {} {}", c.repo, c.rev, fs, caps.join(" "));
}


// ---------------------------------------------------------------------------
// DiskFileSource: walk the fs under <root> and return repo-relative paths.
// rev is ignored; we only serve the working-tree listing. Good enough for
// smoke runs where the pipeline is `repo(.) > rev(HEAD) > fs(**/*.rs)`.
// ---------------------------------------------------------------------------

struct DiskFileSource {
    root: PathBuf,
    rev: String,
}

impl DiskFileSource {
    fn new(root: PathBuf, rev: String) -> Self {
        Self { root, rev }
    }
}

impl FileSource for DiskFileSource {
    fn files(&self, _repo: &str, rev: &str) -> Vec<Arc<Path>> {
        // Only serve the working tree when rev matches this source's rev.
        // Cross-rev reads without a real git backend return empty.
        if rev != self.rev {
            return Vec::new();
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out
    }

    fn file_bytes(&self, _repo: &str, rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        if rev != self.rev {
            return None;
        }
        let full = self.root.join(path);
        std::fs::read(full).ok().map(Arc::from)
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Arc<Path>>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        // Skip hidden dirs (`.git`, `.vscode`, …) and common heavy ones.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(Arc::from(rel));
            }
        }
    }
}
