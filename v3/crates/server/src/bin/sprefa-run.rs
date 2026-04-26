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

use effect_runtime::{RtCtx, RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
use pipeline::_0_cursor::{CaptureKind, Cursor};
use pipeline::_2_pipeline::Pipeline;
use effect_runtime::batchers::{BoundedWorkSteal, CacheLayer};
use pipeline::effects::{
    ast_parse, AstParseEffect, FsListFilesBatcher, FsListFilesEffect,
    PrintBatcher, PrintEffect, ReadBytesBatchBatcher, ReadBytesBatchEffect,
    ReadBytesBatcher, ReadBytesEffect,
};
use pipeline::relation_store::{RelationStore, RelationWake, WriteBatcher, WriteEffect};
use pipeline::readers::FileSource;
use pipeline::registry::Registry;
use server::config::{self, Config, Seed};
use sprefa_parse::{host_parse_with_injections, OpInvocation, Pipe};
use tree_sitter::Node;

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
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = it.next().map(PathBuf::from),
            "--rev" => rev = it.next().cloned().unwrap_or("HEAD".into()),
            "--config" => config_path = it.next().map(PathBuf::from),
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
    let cfg = match config_path {
        Some(p) => config::from_path(&p).unwrap_or_else(|e| {
            eprintln!("config error: {e}");
            std::process::exit(1);
        }),
        None => {
            let root = root.unwrap_or_else(|| std::env::current_dir().unwrap());
            config::from_cli(root, rev.clone())
        }
    };

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
        let relation_store = Arc::new(RelationStore::new());
        let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let ctx = RtCtxBuilder::new()
            .with_store(relation_store.clone())
            .with_store(subject_registry.clone())
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
                WriteBatcher::new(relation_store, subject_registry.clone()),
            )
            .build();
        let seed_tag = if cfg.seeds.len() > 1 {
            format!("[{}] ", seed.slug)
        } else {
            String::new()
        };

        for (i, pipe) in parsed.pipes.iter().enumerate() {
            let Some(head) = pipe.ops.first() else { continue };
            let is_rule = &*head.name == "rule";

            if is_rule {
                let rule_name = rule_name_of(head, source)
                    .unwrap_or_else(|| format!("<unnamed-{i}>"));

                if let Some((brace_src, brace_offset)) = brace_body(head, source) {
                    let (sub, sub_errs) = host_parse_with_injections(
                        brace_src,
                        Arc::from(Path::new("<rule-body>")),
                        &pipeline::op_languages::language_of,
                    );
                    for e in &sub_errs {
                        let start = e.byte_range.start + brace_offset;
                        let end = e.byte_range.end + brace_offset;
                        eprintln!(
                            "{seed_tag}rule {rule_name} body parse: {:?} at {}..{}: {}",
                            e.kind, start, end, e.message
                        );
                    }
                    for (j, sub_pipe) in sub.pipes.iter().enumerate() {
                        run_and_print(
                            &format!("{seed_tag}rule {rule_name} pipe {j}"),
                            sub_pipe,
                            brace_src,
                            &ctx,
                            seed,
                            &registry,
                        ).await;
                    }
                    continue;
                }

                let tail = Pipe { ops: pipe.ops[1..].to_vec() };
                run_and_print(
                    &format!("{seed_tag}rule {rule_name}"),
                    &tail,
                    source,
                    &ctx,
                    seed,
                    &registry,
                ).await;
                continue;
            }

            run_and_print(
                &format!("{seed_tag}pipe {i}"),
                pipe,
                source,
                &ctx,
                seed,
                &registry,
            ).await;
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
}

async fn run_and_print(
    header: &str,
    pipe: &Pipe,
    source: &str,
    ctx: &RtCtx,
    seed: &Seed,
    registry: &Registry,
) {
    let mut ops: Vec<Pipeline> = Vec::new();
    for inv in &pipe.ops {
        let mut diags = Vec::new();
        let lookup_name: Arc<str> = if inv.predicate {
            Arc::from(format!("{}?", inv.name))
        } else {
            inv.name.clone()
        };
        match registry.build_from_node(&lookup_name, inv.node(), source.as_bytes(), &mut diags) {
            Some(Ok(op)) => {
                if !diags.is_empty() {
                    for d in &diags {
                        eprintln!("lower {}: {}: {}", inv.name, d.code, d.message);
                    }
                    std::process::exit(1);
                }
                ops.push(Pipeline::Op(op))
            }
            Some(Err(errs)) => {
                for d in &errs {
                    eprintln!("lower {}: {}: {}", inv.name, d.code, d.message);
                }
                std::process::exit(1);
            }
            None => eprintln!(
                "skip step (unregistered): name={}",
                inv.name
            ),
        }
    }
    if ops.is_empty() {
        return;
    }
    let pipeline = Pipeline::Seq(ops);

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

fn rule_name_of(inv: &OpInvocation, source: &str) -> Option<String> {
    let body = paren_body(inv, source)?.trim();
    if body.is_empty() { return None; }
    Some(body.to_string())
}

fn brace_body<'a>(inv: &OpInvocation, source: &'a str) -> Option<(&'a str, usize)> {
    let node: Node<'_> = inv.node();
    let brace = node.child_by_field_name("brace")?;
    let start = brace.start_byte() + 1;
    let end = brace.end_byte().saturating_sub(1);
    if start > end || end > source.len() { return None; }
    Some((&source[start..end], start))
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

fn paren_body<'a>(inv: &OpInvocation, source: &'a str) -> Option<&'a str> {
    let node: Node<'_> = inv.node();
    let paren = node.child_by_field_name("paren")?;
    let start = paren.start_byte() + 1;
    let end = paren.end_byte().saturating_sub(1);
    if start > end || end > source.len() {
        return None;
    }
    Some(&source[start..end])
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
