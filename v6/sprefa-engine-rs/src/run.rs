// @comment-ok: the shared contract behind `dl6 run`, the built binary's verbs
// and emit_rust_harness; the one doc site for their shape.
//
// `run_once` folds a program over a schedule and reads its `?` rows, which is
// what every one-shot rail script used to spell by hand. `watch` keeps the same
// seam resident and feeds it from the program's OWN `bind` sources, which is
// what `stays_resident` reads to decide the verb's shape for it: a
// `bind watch(glob, ...)` rel from soopy's file watcher, a
// `bind interval(period, ...)` rel from a monotonic clock. One external batch
// is one tick, and the finals re-print as `+`/`-` prefixed TSV deltas.
//
// rx: merge(watchSource(glob), timer(period)).pipe(
//       bufferTime(coalesceMs), filter(batch => batch.length > 0),
//       concatMap(batch => engine.submit(batch)), map(diffAgainstLastFinal))

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::driver::{drive_tick, format_deltas, run_schedule, run_schedule_live};
use crate::hosts::HostLiveRunner;
use crate::program::{run_boot, GenProgram};
use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, ProgramJson, RowColumnType, SqlStatement, TickDeltas, Value,
};

// ═══ the program document ════════════════════════════════════════════════════

/// One `bind` declaration as emit_rust.pl writes it. Column 1 is the
/// configuration column (registry.pl:309), so `literals` are what rules name there.
#[derive(Debug, Clone, Deserialize)]
pub struct BindPlanData {
    pub name: String,
    pub columns: Vec<BindColumn>,
    #[serde(default)]
    pub literals: Vec<serde_json::Value>,
    pub execution: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: String,
}

// Read off the emitted document rather than off GenProgram: ProgramJson is
// owned by another arc's audit, and `#[serde(default)]` keeps an older IR loading.
#[derive(Deserialize)]
struct BindPlansDocument {
    #[serde(default)]
    bind_plans: Vec<BindPlanData>,
}

/// A program as the two bins carry it: the runtime's own document plus the
/// bind plans `watch` drives its world sources from.
pub struct LoadedProgram {
    pub program: GenProgram,
    pub binds: Vec<BindPlanData>,
}

/// The JSON body inside an emitted module's raw string literal.
pub fn program_json_text(module_text: &str) -> Result<&str> {
    let start = module_text
        .find("r#\"")
        .ok_or_else(|| anyhow!("the module carries no r#\" delimiter"))?
        + 3;
    let end = module_text[start..]
        .find("\"#;")
        .ok_or_else(|| anyhow!("the module carries no \"#; delimiter"))?
        + start;
    Ok(&module_text[start..end])
}

pub fn load_program_text(text: &str) -> Result<LoadedProgram> {
    let json = program_json_text(text)?;
    let document: ProgramJson = serde_json::from_str(json).context("parse the program json")?;
    let binds: BindPlansDocument =
        serde_json::from_str(json).context("parse the program's bind plans")?;
    Ok(LoadedProgram {
        program: GenProgram::try_from_json(document)?,
        binds: binds.bind_plans,
    })
}

pub fn load_program(path: &Path) -> Result<LoadedProgram> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read the emitted program {}", path.display()))?;
    load_program_text(&text)
}

/// The bind plans alone, for a built binary that already holds its GenProgram.
pub fn bind_plans_from_json(json: &str) -> Result<Vec<BindPlanData>> {
    let document: BindPlansDocument =
        serde_json::from_str(json).context("parse the program's bind plans")?;
    Ok(document.bind_plans)
}

// ═══ seeds ═══════════════════════════════════════════════════════════════════

/// One `--arrive <rel>=<value>[,<value>...]` occurrence.
#[derive(Debug, Clone)]
pub struct SeedSpec {
    pub rel: String,
    pub cells: Vec<String>,
}

impl std::str::FromStr for SeedSpec {
    type Err = anyhow::Error;

    fn from_str(spec: &str) -> Result<SeedSpec> {
        let (rel, cells) = spec
            .split_once('=')
            .ok_or_else(|| anyhow!("--arrive wants <rel>=<value>[,<value>...], got `{spec}`"))?;
        Ok(SeedSpec {
            rel: rel.to_string(),
            cells: cells.split(',').map(str::to_string).collect(),
        })
    }
}

// A CLI cell carries no type of its own, so the program's declared column type
// decides; an int column that cannot read its cell is a stop, never a 0.
pub fn arrival_row(program: &GenProgram, rel: &str, cells: &[String]) -> Result<Vec<Value>> {
    let types = program
        .rel_column_types
        .get(rel)
        .ok_or_else(|| anyhow!("--arrive names {rel}, which the program does not declare"))?;
    if types.len() != cells.len() {
        bail!(
            "--arrive {rel} carries {} values and the rel declares {} columns",
            cells.len(),
            types.len()
        );
    }
    cells
        .iter()
        .zip(types.iter())
        .map(|(cell, column_type)| match column_type {
            RowColumnType::Int | RowColumnType::Ref | RowColumnType::RelationId => cell
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| anyhow!("--arrive {rel} wants an integer, got `{cell}`")),
            RowColumnType::Float => cell
                .parse::<f64>()
                .map(Value::Real)
                .map_err(|_| anyhow!("--arrive {rel} wants a float, got `{cell}`")),
            RowColumnType::Bool => Ok(Value::Bool(cell == "true")),
            _ => Ok(Value::Text(cell.clone())),
        })
        .collect()
}

pub fn seed_arrivals(program: &GenProgram, specs: &[SeedSpec]) -> Result<Vec<Arrival>> {
    specs
        .iter()
        .map(|spec| {
            Ok(Arrival {
                rel: spec.rel.clone(),
                sign: ArrivalSign::Add,
                row: arrival_row(program, &spec.rel, &spec.cells)?,
            })
        })
        .collect()
}

// ═══ options and outcome ═════════════════════════════════════════════════════

/// Which `?` rows to read after the fold, and how to render them.
#[derive(Debug, Clone, Default)]
pub struct FinalRequest {
    pub wanted: bool,
    pub only: bool,
    pub tsv: bool,
    pub rels: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct RunOptions {
    /// The batches a schedule file carries; the seeds join the first one.
    pub schedule: Vec<Vec<Arrival>>,
    pub live_hosts: bool,
    pub finals: FinalRequest,
    /// With a path the run leaves a plain SQLite file a cold `sqlite3` reads.
    pub db: Option<PathBuf>,
    /// A `?` query whose non-empty answer makes the process exit 1.
    pub fail_on: Option<String>,
    pub drain_cap: usize,
}

pub const DRAIN_CAP: usize = 100;

impl RunOptions {
    pub fn one_shot() -> RunOptions {
        RunOptions {
            drain_cap: DRAIN_CAP,
            ..RunOptions::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinalRel {
    pub rel: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

pub struct RunOutcome {
    /// The folded seam, still open: `--socket` serves it and `--db` has already
    /// written its views into it.
    pub seam: SqliteSeam,
    pub lines: Vec<String>,
    pub finals: Vec<FinalRel>,
    pub ticks: usize,
    /// `None` without `--fail-on`, else the named query's row count.
    pub fail_on_rows: Option<usize>,
}

impl RunOutcome {
    pub fn failed(&self) -> bool {
        self.fail_on_rows.is_some_and(|rows| rows > 0)
    }
}

// ═══ run once ════════════════════════════════════════════════════════════════

pub fn run_once(
    program: &GenProgram,
    seeds: Vec<Arrival>,
    options: RunOptions,
) -> Result<RunOutcome> {
    let seam = open_seam(options.db.as_deref())?;
    let mut schedule = options.schedule;
    if !seeds.is_empty() {
        match schedule.first_mut() {
            Some(first) => first.extend(seeds),
            None => schedule.push(seeds),
        }
    }
    if options.live_hosts {
        reject_scripted_responses(&schedule)?;
    }
    let runtime = current_thread_runtime()?;
    let fold = if options.live_hosts {
        runtime.block_on(run_schedule_live(
            program,
            &seam,
            &schedule,
            options.drain_cap,
        ))?
    } else {
        runtime.block_on(run_schedule(program, &seam, &schedule, options.drain_cap))?
    };
    let ticks = fold.lines.len();
    let finals = if options.finals.wanted {
        read_finals(program, &seam, &options.finals)?
    } else {
        Vec::new()
    };
    let fail_on_rows = match &options.fail_on {
        Some(query) => Some(count_query(program, &seam, query)?),
        None => None,
    };
    if let Some(path) = &options.db {
        install_db_views(program, &seam, ticks, path)?;
    }
    Ok(RunOutcome {
        seam,
        lines: fold.lines,
        finals,
        ticks,
        fail_on_rows,
    })
}

/// The runtime both verbs fold on. Current-thread by the one-subscribe law: the
/// tick engine is a single driver and a worker pool would only add contention.
pub fn current_thread_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build the tokio runtime")
}

// A live-host program produces its own response rows, so a scripted one in the
// schedule would double them; it is named before any tick runs.
pub fn reject_scripted_responses(schedule: &[Vec<Arrival>]) -> Result<()> {
    if let Some(scripted) = schedule
        .iter()
        .flatten()
        .find(|arrival| arrival.rel.starts_with("__host_response_"))
    {
        bail!(
            "--live-hosts forbids the scripted response row {} in the schedule; \
             the runtime produces it",
            scripted.rel
        );
    }
    Ok(())
}

/// A db path opens a file seam; without one the fold stays in memory, which is
/// what every golden uses. The file is SHARED by every program (CLAUDE.md, one
/// server one db), so it is opened and never replaced: `reset_program_objects`
/// clears this program's own tables at boot instead.
pub fn open_seam(db: Option<&Path>) -> Result<SqliteSeam> {
    let Some(path) = db else {
        return SqliteSeam::in_memory().context("open the in-memory seam");
    };
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let url = path
        .to_str()
        .ok_or_else(|| anyhow!("the db path is not utf-8: {}", path.display()))?;
    SqliteSeam::open(url).with_context(|| format!("open {url}"))
}


// ═══ reading the `?` rows ════════════════════════════════════════════════════

fn final_rels(program: &GenProgram, request: &FinalRequest) -> Vec<String> {
    match &request.rels {
        Some(named) => named.clone(),
        None => {
            let mut every: Vec<String> = program.final_select.keys().cloned().collect();
            every.sort();
            every
        }
    }
}

fn select_for(program: &GenProgram, rel: &str) -> Result<String> {
    program
        .final_select
        .get(rel)
        .cloned()
        .ok_or_else(|| anyhow!("no final_select for {rel}"))
}

pub fn read_finals(
    program: &GenProgram,
    seam: &SqliteSeam,
    request: &FinalRequest,
) -> Result<Vec<FinalRel>> {
    final_rels(program, request)
        .into_iter()
        .map(|rel| read_final(program, seam, &rel))
        .collect()
}

pub fn read_final(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Result<FinalRel> {
    let select = select_for(program, rel)?;
    // A `?` order tail rides final_select's own ORDER BY, so those rows are
    // already in the order asked for; the rest sort to stay reproducible.
    let cursor_orders = select.contains(" ORDER BY ");
    let result = seam
        .execute(&SqlStatement {
            sql: select,
            args: vec![],
        })
        .with_context(|| format!("final read of {rel}"))?;
    let mut rows = result.rows;
    if !cursor_orders {
        rows.sort_by_key(|row| row.iter().map(cell_text).collect::<Vec<_>>());
    }
    Ok(FinalRel {
        rel: rel.to_string(),
        columns: program.rel_columns.get(rel).cloned().unwrap_or_default(),
        rows,
    })
}

fn count_query(program: &GenProgram, seam: &SqliteSeam, query: &str) -> Result<usize> {
    if !program.final_select.contains_key(query) {
        let mut declared: Vec<&str> = program.queries.iter().map(String::as_str).collect();
        declared.sort_unstable();
        bail!(
            "--fail-on names {query}, which the program does not answer; its `?` queries are: {}",
            declared.join(", ")
        );
    }
    Ok(read_final(program, seam, query)?.rows.len())
}

// ═══ rendering ═══════════════════════════════════════════════════════════════

// A `bytes` column has no text transport on either seam, so the reader stops on
// one rather than invent an encoding the tick log does not use.
pub fn cell_text(value: &Value) -> String {
    match value {
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => crate::ticklog::js_float_text(*number),
        Value::Bool(flag) => flag.to_string(),
        Value::Text(text) => text.clone(),
        Value::List(items) => serde_json::Value::Array(items.clone()).to_string(),
        Value::Bytes(_) => "<bytes>".to_string(),
    }
}

fn cell_json(value: &Value, column_type: Option<RowColumnType>) -> serde_json::Value {
    match value {
        Value::Integer(number) => serde_json::json!(number),
        Value::Real(number) => serde_json::json!(number),
        Value::Bool(flag) => serde_json::json!(flag),
        Value::List(items) => serde_json::Value::Array(items.clone()),
        Value::Bytes(_) => serde_json::json!(cell_text(value)),
        Value::Text(text) => {
            if column_type == Some(RowColumnType::Json) || column_type == Some(RowColumnType::Ref) {
                serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!(text))
            } else {
                serde_json::json!(text)
            }
        }
    }
}

// A tab or a newline inside a value would forge a column, so the TSV writer
// stops on one rather than hand a shell a row it would mis-split.
fn tsv_cell(rel: &str, value: &Value) -> Result<String> {
    let text = cell_text(value);
    if let Value::Bytes(_) = value {
        bail!("--final cannot render the bytes column in {rel}");
    }
    if text.contains('\t') || text.contains('\n') {
        bail!("--final-tsv cannot carry the tab or newline in a {rel} value");
    }
    Ok(text)
}

fn bytes_column(final_rel: &FinalRel) -> Option<&str> {
    final_rel
        .rows
        .iter()
        .flatten()
        .any(|value| matches!(value, Value::Bytes(_)))
        .then_some(final_rel.rel.as_str())
}

/// Every line the finals print, in the order they print, so `dl6 run`, the built
/// binary and emit_rust_harness render one program identically.
pub fn final_lines(
    program: &GenProgram,
    finals: &[FinalRel],
    request: &FinalRequest,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for final_rel in finals {
        let types = program
            .rel_column_types
            .get(&final_rel.rel)
            .cloned()
            .unwrap_or_default();
        if request.tsv {
            for row in &final_rel.rows {
                let cells: Vec<String> = row
                    .iter()
                    .map(|value| tsv_cell(&final_rel.rel, value))
                    .collect::<Result<_>>()?;
                lines.push(format!("{}\t{}", final_rel.rel, cells.join("\t")));
            }
        } else {
            if let Some(rel) = bytes_column(final_rel) {
                bail!("--final cannot render the bytes column in {rel}");
            }
            let bodies: Vec<serde_json::Value> = final_rel
                .rows
                .iter()
                .map(|row| {
                    serde_json::Value::Array(
                        row.iter()
                            .enumerate()
                            .map(|(index, value)| cell_json(value, types.get(index).copied()))
                            .collect(),
                    )
                })
                .collect();
            lines.push(
                serde_json::json!({
                    "rel": final_rel.rel,
                    "columns": final_rel.columns,
                    "rows": bodies
                })
                .to_string(),
            );
        }
    }
    Ok(lines)
}

/// stdout for a one-shot run: the tick log unless `--final-only`, then the finals.
pub fn print_outcome(
    program: &GenProgram,
    outcome: &RunOutcome,
    request: &FinalRequest,
) -> Result<()> {
    if !request.only {
        for line in &outcome.lines {
            println!("{line}");
        }
    }
    if request.wanted {
        for line in final_lines(program, &outcome.finals, request)? {
            println!("{line}");
        }
    }
    Ok(())
}

// ═══ the database is the receipt ═════════════════════════════════════════════

/// The metadata table a `--db` file carries, so a cold reader knows which
/// program wrote it and how far the fold got.
pub const META_TABLE: &str = "__meta";

/// The TEMP decoded-text views re-created as persistent ones, a `v_<query>` view
/// per `?` carrying its `ORDER BY`, and `__meta`. TEMP stays lower.pl's default.
fn install_db_views(
    program: &GenProgram,
    seam: &SqliteSeam,
    ticks: usize,
    path: &Path,
) -> Result<()> {
    let span = tracing::info_span!("db_views", db = %path.display());
    let _entered = span.enter();
    let mut created = 0usize;
    for statement in &program.ddl {
        let Some(tail) = statement.strip_prefix("CREATE TEMP VIEW ") else {
            continue;
        };
        seam.execute_multiple(&format!("CREATE VIEW IF NOT EXISTS {tail}"))
            .with_context(|| format!("re-create a persistent view from `{}`", &statement[..80.min(statement.len())]))?;
        created += 1;
    }
    for query in &program.queries {
        let select = select_for(program, query)?;
        seam.execute_multiple(&format!(
            "CREATE VIEW IF NOT EXISTS \"v_{query}\" AS {select}"
        ))
        .with_context(|| format!("create the v_{query} view"))?;
        created += 1;
    }
    seam.execute_multiple(&format!(
        "CREATE TABLE IF NOT EXISTS \"{META_TABLE}\" (\
         \"__id\" INTEGER PRIMARY KEY, \"program\" TEXT NOT NULL, \
         \"source_digest\" TEXT NOT NULL, \"compiler_digest\" TEXT NOT NULL, \
         \"tick\" INTEGER NOT NULL, \"finished_at\" TEXT NOT NULL)"
    ))
    .context("create the __meta table")?;
    let finished_at = unix_seconds();
    seam.execute(&SqlStatement {
        sql: format!(
            "INSERT INTO \"{META_TABLE}\" (\"program\", \"source_digest\", \
             \"compiler_digest\", \"tick\", \"finished_at\") VALUES (?, ?, ?, ?, ?)"
        ),
        args: vec![
            crate::types::ScalarValue::Text(program.name.clone()),
            crate::types::ScalarValue::Text(digest_of("DL6_SOURCE_DIGEST")),
            crate::types::ScalarValue::Text(digest_of("DL6_COMPILER_DIGEST")),
            crate::types::ScalarValue::Integer(ticks as i64),
            crate::types::ScalarValue::Text(finished_at.to_string()),
        ],
    })
    .context("record the run in __meta")?;
    tracing::info!(views = created, ticks, "wrote the persistent read surface");
    Ok(())
}

// `dl6 run` knows both digests and passes them down through the environment
// rather than through a signature the built binary cannot fill.
fn digest_of(variable: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| "unknown".to_string())
}

// ═══ the resident live loop ══════════════════════════════════════════════════

/// One external batch and every host-response and carry-drain tick it implies.
/// `HostLiveRunner`'s `claimed` set is why one runner spans every tick.
struct LiveLoop<'p> {
    program: &'p GenProgram,
    runner: Option<HostLiveRunner<'p>>,
    tick: usize,
    drain_cap: usize,
}

impl<'p> LiveLoop<'p> {
    fn open(program: &'p GenProgram, live_hosts: bool, drain_cap: usize) -> Result<LiveLoop<'p>> {
        let runner = if live_hosts {
            let adapter_rows = crate::types::load_program_host_adapter_rows(&program.name)
                .context("read the process adapter sidecar")?;
            Some(
                HostLiveRunner::with_adapter_rows(
                    &program.host_plans,
                    &program.rel_columns,
                    &adapter_rows,
                )
                .map_err(|failure| anyhow!("{failure}"))?,
            )
        } else {
            None
        };
        Ok(LiveLoop {
            program,
            runner,
            tick: 0,
            drain_cap,
        })
    }

    fn boot(&self, seam: &SqliteSeam) -> Result<()> {
        crate::trace::arm();
        seam.size_statement_cache(self.program.stable_sql_count() + 64);
        seam.run_program_ddl(&self.program.ddl, &self.program.queries)
            .context("run the DDL")?;
        run_boot(seam, &self.program.boot);
        Ok(())
    }

    async fn fold(&mut self, seam: &SqliteSeam, batch: Vec<Arrival>) -> Result<Vec<String>> {
        let mut pending: VecDeque<Vec<Arrival>> = VecDeque::new();
        pending.push_back(batch);
        let mut lines = Vec::new();
        let mut carry_pending = false;
        let mut off_batch = 0usize;
        loop {
            let arrivals = match pending.pop_front() {
                Some(arrivals) => arrivals,
                None if carry_pending => Vec::new(),
                None => break,
            };
            if lines.len() > 0 {
                off_batch += 1;
                if off_batch > self.drain_cap {
                    bail!(
                        "drain overflow: {} exceeded {} host/drain ticks in one batch",
                        self.program.name,
                        self.drain_cap
                    );
                }
            }
            let deltas = self.drive(seam, arrivals).await?;
            self.tick += 1;
            carry_pending = deltas.carry_pending;
            lines.push(format_deltas(self.program, self.tick, &deltas));
            if let Some(runner) = self.runner.as_mut() {
                let responses = {
                    let _scope = crate::trace::Scope::phase("host_collect");
                    runner.collect(&deltas).map_err(|failure| anyhow!("{failure}"))?
                };
                if !responses.is_empty() {
                    pending.push_back(responses);
                }
            }
        }
        Ok(lines)
    }

    async fn drive(&self, seam: &SqliteSeam, arrivals: Vec<Arrival>) -> Result<TickDeltas> {
        let span = tracing::info_span!("tick", tick = self.tick, arrivals = arrivals.len());
        let _entered = span.enter();
        Ok(drive_tick(self.program, seam, arrivals).await?)
    }
}

// ═══ watch ═══════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct WatchOptions {
    pub run: RunOptions,
    /// The bind plans the program declares; empty means no live source at all.
    pub binds: Vec<BindPlanData>,
    /// The tree a `bind watch` glob is read against.
    pub root: PathBuf,
    /// How long a burst of filesystem events is gathered into one tick.
    pub coalesce: Duration,
}

impl WatchOptions {
    pub fn new(run: RunOptions, binds: Vec<BindPlanData>, root: PathBuf) -> WatchOptions {
        WatchOptions {
            run,
            binds,
            root,
            coalesce: Duration::from_millis(120),
        }
    }
}

/// The names registry.pl gives the two live world sources.
const WATCH_EXECUTOR: &str = "live_watch";
const INTERVAL_EXECUTOR: &str = "live_interval";

/// The one question `dl6 run` asks the file: a rel routed to a continuing
/// source is what makes the process stay, so the verb never has to.
pub fn stays_resident(binds: &[BindPlanData]) -> bool {
    [WATCH_EXECUTOR, INTERVAL_EXECUTOR]
        .iter()
        .any(|executor| bind_rel(binds, executor).is_some())
}

/// How often the loop wakes to notice a stop request while nothing else moves.
const STOP_POLL: Duration = Duration::from_millis(250);

// The glob a wakeup came from does not narrow the answer, because one
// enumeration reads every open glob, so `Moved` carries no payload.
enum WatchEvent {
    Moved,
    Closed(String, String),
}

/// Tick 0's bind rows without a watcher: `bind watch` becomes the enumeration of
/// each glob, `bind interval` its current bucket. A one-shot run seeds with this.
pub fn bind_seeds(binds: &[BindPlanData], root: &Path) -> Result<Vec<Arrival>> {
    let mut seeds = Vec::new();
    if let Some(rel) = bind_rel(binds, WATCH_EXECUTOR) {
        let globs = literal_texts(binds, WATCH_EXECUTOR);
        seeds.extend(FileSources::new(root, &globs).snapshot_arrivals(&rel)?);
    }
    if let Some(rel) = bind_rel(binds, INTERVAL_EXECUTOR) {
        let periods = literal_integers(binds, INTERVAL_EXECUTOR);
        seeds.extend(IntervalClocks::open(&periods).arrivals(&rel));
    }
    Ok(seeds)
}

/// Fold, print the finals, then stay up: one `bind` push is one tick and the
/// finals re-print as `+`/`-` TSV deltas. Returns on `stop` or on every close.
/// `true` means `--fail-on` answered rows, so the caller exits 1.
pub fn watch(
    program: &GenProgram,
    seeds: Vec<Arrival>,
    options: WatchOptions,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<bool> {
    let seam = open_seam(options.run.db.as_deref())?;
    let runtime = current_thread_runtime()?;
    let mut live = LiveLoop::open(program, options.run.live_hosts, options.run.drain_cap)?;
    live.boot(&seam)?;

    let globs = literal_texts(&options.binds, WATCH_EXECUTOR);
    let periods = literal_integers(&options.binds, INTERVAL_EXECUTOR);
    let watch_rel = bind_rel(&options.binds, WATCH_EXECUTOR);
    let interval_rel = bind_rel(&options.binds, INTERVAL_EXECUTOR);

    let (events, inbox) = mpsc::channel::<WatchEvent>();
    let mut sources = FileSources::new(&options.root, &globs);
    sources.arm(events)?;
    let mut clocks = IntervalClocks::open(&periods);

    let mut first = seeds;
    if let Some(rel) = &watch_rel {
        first.extend(sources.snapshot_arrivals(rel)?);
    }
    if let Some(rel) = &interval_rel {
        first.extend(clocks.arrivals(rel));
    }

    let started = Instant::now();
    let lines = runtime.block_on(live.fold(&seam, first))?;
    let mut snapshot = FinalSnapshot::read(program, &seam, &options.run.finals)?;
    for line in snapshot.plus_lines()? {
        println!("{line}");
    }
    // The views go in after the first fold, not at the end: a cold `sqlite3`
    // has to read a resident program WHILE it runs, which is the whole point.
    if let Some(path) = &options.run.db {
        install_db_views(program, &seam, live.tick, path)?;
    }
    let mut failed = fail_on_rows(program, &seam, &options.run.fail_on)?;
    tracing::info!(
        tick = live.tick,
        ticks = lines.len(),
        wall_ms = started.elapsed().as_millis() as u64,
        "watch: first fold"
    );
    if globs.is_empty() && periods.is_empty() {
        bail!(
            "{} declares no `bind watch` or `bind interval` source, so a watch would never tick; \
             run it once with `dl6 run`",
            program.name
        );
    }

    loop {
        if *stop.borrow_and_update() {
            break;
        }
        let deadline = clocks.next_fire().unwrap_or(STOP_POLL).min(STOP_POLL);
        let mut batch: Vec<Arrival> = Vec::new();
        match inbox.recv_timeout(deadline) {
            Ok(event) => {
                let mut burst = vec![event];
                // One burst of saves is one tick: a `cargo fmt` over a crate must
                // not become one fold per file.
                let until = Instant::now() + options.coalesce;
                while let Ok(more) =
                    inbox.recv_timeout(until.saturating_duration_since(Instant::now()))
                {
                    burst.push(more);
                }
                let mut moved = false;
                for event in burst {
                    match event {
                        WatchEvent::Moved => moved = true,
                        WatchEvent::Closed(glob, failure) => {
                            sources.close(&glob);
                            tracing::warn!(glob, failure, "watch: a file source closed");
                        }
                    }
                }
                if moved {
                    if let Some(rel) = &watch_rel {
                        batch.extend(sources.refresh_arrivals(rel)?);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if periods.is_empty() {
                    break;
                }
            }
        }
        if let Some(rel) = &interval_rel {
            batch.extend(clocks.due_arrivals(rel));
        }
        if batch.is_empty() {
            continue;
        }
        let started = Instant::now();
        let arrivals = batch.len();
        runtime.block_on(live.fold(&seam, batch))?;
        let next = FinalSnapshot::read(program, &seam, &options.run.finals)?;
        for line in snapshot.delta_lines(&next)? {
            println!("{line}");
        }
        snapshot = next;
        if options.run.db.is_some() {
            stamp_meta_tick(&seam, live.tick)?;
        }
        failed |= fail_on_rows(program, &seam, &options.run.fail_on)?;
        tracing::info!(
            tick = live.tick,
            arrivals,
            wall_ms = started.elapsed().as_millis() as u64,
            "watch: tick"
        );
    }
    Ok(failed)
}

fn fail_on_rows(program: &GenProgram, seam: &SqliteSeam, query: &Option<String>) -> Result<bool> {
    match query {
        Some(named) => Ok(count_query(program, seam, named)? > 0),
        None => Ok(false),
    }
}

/// One row, updated in place: a resident fold that inserted per tick would grow
/// `__meta` without bound over a run measured in days.
fn stamp_meta_tick(seam: &SqliteSeam, ticks: usize) -> Result<()> {
    seam.execute(&SqlStatement {
        sql: format!("UPDATE \"{META_TABLE}\" SET \"tick\" = ?, \"finished_at\" = ?"),
        args: vec![
            crate::types::ScalarValue::Integer(ticks as i64),
            crate::types::ScalarValue::Text(unix_seconds().to_string()),
        ],
    })
    .context("stamp the tick into __meta")?;
    Ok(())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

fn bind_rel(binds: &[BindPlanData], executor: &str) -> Option<String> {
    binds
        .iter()
        .find(|plan| plan.execution == executor && !plan.literals.is_empty())
        .map(|plan| plan.name.clone())
}

fn literal_texts(binds: &[BindPlanData], executor: &str) -> Vec<String> {
    binds
        .iter()
        .filter(|plan| plan.execution == executor)
        .flat_map(|plan| plan.literals.iter())
        .filter_map(|literal| literal.as_str().map(str::to_string))
        .collect()
}

fn literal_integers(binds: &[BindPlanData], executor: &str) -> Vec<i64> {
    binds
        .iter()
        .filter(|plan| plan.execution == executor)
        .flat_map(|plan| plan.literals.iter())
        .filter_map(serde_json::Value::as_i64)
        .filter(|period| *period > 0)
        .collect()
}

// ═══ bind watch: soopy's file watcher ════════════════════════════════════════

/// The watcher NOTIFIES and `soopy::enumerate` ANSWERS: a delta carries
/// `ContentId::Blake3`, a demand host reads by the `GitBlob` oid (hosts.rs:839).
struct FileSources {
    root: PathBuf,
    globs: Vec<String>,
    open: BTreeSet<String>,
    /// The rows the rel currently holds, so a re-enumeration is a set difference.
    held: BTreeMap<(String, String), String>,
}

impl FileSources {
    fn new(root: &Path, globs: &[String]) -> FileSources {
        FileSources {
            root: root.to_path_buf(),
            globs: globs.to_vec(),
            open: globs.iter().cloned().collect(),
            held: BTreeMap::new(),
        }
    }

    /// One notifying thread per glob. A one-shot `dl6 run` never calls this: it
    /// takes the enumeration and folds, so it spawns no thread and no watcher.
    fn arm(&mut self, events: mpsc::Sender<WatchEvent>) -> Result<()> {
        for glob in &self.globs {
            let repository = soopy::discover(&self.root)
                .with_context(|| format!("open a repository at {}", self.root.display()))?;
            let mut watcher = soopy::SourceTree::open(repository)
                .watch(soopy::SourceQuery {
                    revision: soopy::Revision::Worktree,
                    patterns: vec![soopy::Pattern(glob.clone())],
                })
                .with_context(|| format!("watch `{glob}`"))?;
            let owned = glob.clone();
            let events = events.clone();
            std::thread::Builder::new()
                .name(format!("dl6-watch-{glob}"))
                .spawn(move || loop {
                    match watcher.recv_timeout(Duration::from_secs(3600)) {
                        Ok(Some(deltas)) if !deltas.is_empty() => {
                            if events.send(WatchEvent::Moved).is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(failure) => {
                            let _ = events
                                .send(WatchEvent::Closed(owned.clone(), failure.to_string()));
                            return;
                        }
                    }
                })
                .with_context(|| format!("spawn the watcher thread for `{glob}`"))?;
        }
        Ok(())
    }

    fn close(&mut self, glob: &str) {
        self.open.remove(glob);
    }

    /// Every tracked file each glob names, with its blob digest. The same
    /// enumeration `SoopyFilesExecutor` runs for the `files` host.
    fn enumerate(&self) -> Result<BTreeMap<(String, String), String>> {
        let repository = soopy::discover(&self.root)
            .with_context(|| format!("open a repository at {}", self.root.display()))?;
        let mut rows = BTreeMap::new();
        for glob in self.globs.iter().filter(|glob| self.open.contains(*glob)) {
            let entries = soopy::enumerate(
                &repository,
                &soopy::GitFilesQuery {
                    revision: soopy::Revision::Worktree,
                    pathspecs: vec![glob.clone()],
                },
            )
            .with_context(|| format!("enumerate `{glob}`"))?;
            for entry in entries {
                let soopy::ContentId::GitBlob(oid) = &entry.content else {
                    bail!(
                        "tracked file {} carries no git blob digest",
                        entry.source.path.0
                    );
                };
                rows.insert(
                    (glob.clone(), entry.source.path.0.to_string()),
                    oid.0.to_string(),
                );
            }
        }
        Ok(rows)
    }

    /// Tick 0's rows.
    fn snapshot_arrivals(&mut self, rel: &str) -> Result<Vec<Arrival>> {
        self.held = self.enumerate()?;
        Ok(self
            .held
            .iter()
            .map(|((glob, path), digest)| {
                bind_arrival(rel, ArrivalSign::Add, [glob, path, digest].map(String::as_str))
            })
            .collect())
    }

    /// A save that changed no bytes re-enumerates to the same digest and is zero
    /// delta at the rel boundary, so nothing downstream re-derives.
    fn refresh_arrivals(&mut self, rel: &str) -> Result<Vec<Arrival>> {
        let next = self.enumerate()?;
        let mut arrivals = Vec::new();
        for (key, digest) in &self.held {
            if next.get(key) != Some(digest) {
                arrivals.push(bind_arrival(
                    rel,
                    ArrivalSign::Del,
                    [key.0.as_str(), key.1.as_str(), digest.as_str()],
                ));
            }
        }
        for (key, digest) in &next {
            if self.held.get(key) != Some(digest) {
                arrivals.push(bind_arrival(
                    rel,
                    ArrivalSign::Add,
                    [key.0.as_str(), key.1.as_str(), digest.as_str()],
                ));
            }
        }
        self.held = next;
        Ok(arrivals)
    }
}

fn bind_arrival(rel: &str, sign: ArrivalSign, cells: [&str; 3]) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign,
        row: cells
            .iter()
            .map(|cell| Value::Text((*cell).to_string()))
            .collect(),
    }
}

// ═══ bind interval: the monotonic clock ══════════════════════════════════════

/// One bucket counter per declared period. The previous bucket is retired in the
/// same batch, so the rel holds one row per cadence rather than growing.
struct IntervalClocks {
    buckets: BTreeMap<i64, i64>,
}

impl IntervalClocks {
    fn open(periods: &[i64]) -> IntervalClocks {
        IntervalClocks {
            buckets: periods.iter().map(|period| (*period, -1)).collect(),
        }
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0)
    }

    /// The first batch: every cadence's current bucket, with nothing to retire.
    fn arrivals(&mut self, rel: &str) -> Vec<Arrival> {
        let now = IntervalClocks::now();
        let mut arrivals = Vec::new();
        for (period, held) in self.buckets.iter_mut() {
            let bucket = now / *period;
            *held = bucket;
            arrivals.push(interval_arrival(rel, ArrivalSign::Add, *period, bucket));
        }
        arrivals
    }

    fn due_arrivals(&mut self, rel: &str) -> Vec<Arrival> {
        let now = IntervalClocks::now();
        let mut arrivals = Vec::new();
        for (period, held) in self.buckets.iter_mut() {
            let bucket = now / *period;
            if bucket == *held {
                continue;
            }
            arrivals.push(interval_arrival(rel, ArrivalSign::Del, *period, *held));
            arrivals.push(interval_arrival(rel, ArrivalSign::Add, *period, bucket));
            *held = bucket;
        }
        arrivals
    }

    /// How long the loop may park before the earliest cadence turns over.
    fn next_fire(&self) -> Option<Duration> {
        let now = IntervalClocks::now();
        self.buckets
            .keys()
            .map(|period| {
                let elapsed = now % *period;
                Duration::from_secs((*period - elapsed).max(1) as u64)
            })
            .min()
    }
}

fn interval_arrival(rel: &str, sign: ArrivalSign, period: i64, bucket: i64) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign,
        row: vec![Value::Integer(period), Value::Integer(bucket)],
    }
}

// ═══ the printed delta ═══════════════════════════════════════════════════════

/// The finals as of one tick, keyed for set difference. Rows are held as rendered
/// cells so the diff is over what the reader sees, never over interned ids.
struct FinalSnapshot {
    rels: Vec<(String, BTreeSet<Vec<String>>)>,
}

impl FinalSnapshot {
    fn read(
        program: &GenProgram,
        seam: &SqliteSeam,
        request: &FinalRequest,
    ) -> Result<FinalSnapshot> {
        let mut rels = Vec::new();
        for rel in final_rels(program, request) {
            let read = read_final(program, seam, &rel)?;
            let rows = read
                .rows
                .iter()
                .map(|row| row.iter().map(cell_text).collect::<Vec<String>>())
                .collect();
            rels.push((rel, rows));
        }
        Ok(FinalSnapshot { rels })
    }

    fn plus_lines(&self) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        for (rel, rows) in &self.rels {
            for row in rows {
                lines.push(delta_line('+', rel, row)?);
            }
        }
        Ok(lines)
    }

    fn delta_lines(&self, next: &FinalSnapshot) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        let before: BTreeMap<&String, &BTreeSet<Vec<String>>> =
            self.rels.iter().map(|(rel, rows)| (rel, rows)).collect();
        for (rel, rows) in &next.rels {
            let empty = BTreeSet::new();
            let held = before.get(rel).copied().unwrap_or(&empty);
            for row in held.difference(rows) {
                lines.push(delta_line('-', rel, row)?);
            }
            for row in rows.difference(held) {
                lines.push(delta_line('+', rel, row)?);
            }
        }
        Ok(lines)
    }
}

fn delta_line(sign: char, rel: &str, row: &[String]) -> Result<String> {
    for cell in row {
        if cell.contains('\t') || cell.contains('\n') {
            bail!("a watch delta cannot carry the tab or newline in a {rel} value");
        }
    }
    Ok(format!("{sign}{rel}\t{}", row.join("\t")))
}
