// @comment-ok: the shared contract behind `dl6 run`, the built binary's verbs
// and emit_rust_harness; the one doc site for their shape.

// `watch` stays resident on the program's OWN continuing executors: a rel
// routed to `/soopy/watch` or `/clock/tick`, `ExecutorCadence::Continuing`.

// `stays_resident` reads that routing. One external batch is one tick.

// rx: merge(watchSource(glob), timer(period)).pipe(
//       bufferTime(coalesceMs), filter(batch => batch.length > 0),
//       concatMap(batch => engine.submit(batch)), map(diffAgainstLastFinal))

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};

use crate::driver::{drive_tick, format_deltas, run_schedule, run_schedule_live};
use crate::executors::clock::ClockExecutor;
use crate::executors::watch::SoopyWatchExecutor;
use crate::hosts::{self, ExecutorCadence, HostLiveRunner};
use crate::program::{run_boot, GenProgram};
use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, HostAdapterRow, HostPlanData, ProgramJson, RowColumnType, ScalarValue,
    SqlStatement, TickDeltas, Value,
};

// ═══ the program document ════════════════════════════════════════════════════

/// A program as `dl6` carries it after loading the emitted document.
pub struct LoadedProgram {
    pub program: GenProgram,
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
    Ok(LoadedProgram {
        program: GenProgram::try_from_json(document)?,
    })
}

pub fn load_program(path: &Path) -> Result<LoadedProgram> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read the emitted program {}", path.display()))?;
    load_program_text(&text)
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
    mut seeds: Vec<Arrival>,
    options: RunOptions,
) -> Result<RunOutcome> {
    let seam = open_seam(options.db.as_deref())?;
    let mut schedule = options.schedule;
    if options.live_hosts {
        let adapter_rows = crate::types::load_program_host_adapter_rows(&program.name)
            .context("read the process adapter sidecar")?;
        if stays_resident(program, &adapter_rows) {
            seam.size_statement_cache(program.stable_sql_count() + 64);
            seam.run_program_ddl(&program.ddl, &program.queries)
                .context("run the DDL")?;
            run_boot(&seam, &program.boot);
            seeds.extend(continuing_seed_arrivals(
                program,
                &seam,
                &adapter_rows,
                Path::new("."),
            )?);
        }
    }
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
    /// The tree a `/soopy/watch` glob is read against.
    pub root: PathBuf,
    /// How long a burst of filesystem events is gathered into one tick.
    pub coalesce: Duration,
}

impl WatchOptions {
    pub fn new(run: RunOptions, root: PathBuf) -> WatchOptions {
        WatchOptions {
            run,
            root,
            coalesce: Duration::from_millis(120),
        }
    }
}

/// The one host demand rel this crate's resident loop drives on its own clock
/// or its own file watcher, never through the once-per-witness collect seam.
fn continuing_plans<'p>(
    program: &'p GenProgram,
    adapter_rows: &[HostAdapterRow],
) -> Vec<&'p HostPlanData> {
    program
        .host_plans
        .iter()
        .filter(|plan| hosts::cadence_for_plan(plan, adapter_rows) == ExecutorCadence::Continuing)
        .collect()
}

/// The one question `dl6 run` asks the file: a rel routed to a continuing
/// executor is what makes the process stay, so the verb never has to.
pub fn stays_resident(program: &GenProgram, adapter_rows: &[HostAdapterRow]) -> bool {
    !continuing_plans(program, adapter_rows).is_empty()
}

fn relation_table<'p>(program: &'p GenProgram, rel: &str) -> Result<&'p str> {
    program
        .relations
        .iter()
        .find(|relation| relation.rel == rel)
        .map(|relation| relation.table_name.as_str())
        .ok_or_else(|| anyhow!("no incremental relation plan for {rel}"))
}

fn plan_input_column(plan: &HostPlanData) -> Result<&str> {
    plan.inputs
        .first()
        .map(|input| input.name.as_str())
        .ok_or_else(|| anyhow!("{} declares no input column", plan.name))
}

fn distinct_ints(seam: &SqliteSeam, table: &str, column: &str) -> Result<Vec<i64>> {
    let result = seam
        .execute(&SqlStatement {
            sql: format!("SELECT DISTINCT \"{column}\" FROM \"{table}\""),
            args: vec![],
        })
        .with_context(|| format!("read {column} from {table}"))?;
    result
        .rows
        .into_iter()
        .map(|row| match row.into_iter().next() {
            Some(Value::Integer(number)) => Ok(number),
            other => bail!("{column} in {table} is not an integer: {other:?}"),
        })
        .collect()
}

/// A declared `text` column may store an `__str` surrogate id, per the rel's
/// own text-intern plan, so a raw `SELECT` on it reads an integer, not text.
fn column_is_interned(program: &GenProgram, rel: &str, column: &str) -> bool {
    let Some(plan) = &program.text_intern_plan else {
        return false;
    };
    let Some(index) = program
        .rel_columns
        .get(rel)
        .and_then(|columns| columns.iter().position(|name| name == column))
    else {
        return false;
    };
    plan.rel_columns
        .get(rel)
        .and_then(|flags| flags.get(index))
        .copied()
        .unwrap_or(false)
}

fn distinct_texts(
    program: &GenProgram,
    seam: &SqliteSeam,
    table: &str,
    rel: &str,
    column: &str,
) -> Result<Vec<String>> {
    let sql = if column_is_interned(program, rel, column) {
        format!(
            "SELECT DISTINCT s.\"content\" FROM \"{table}\" t \
             JOIN \"__str\" s ON s.\"__id\" = t.\"{column}\""
        )
    } else {
        format!("SELECT DISTINCT \"{column}\" FROM \"{table}\"")
    };
    let result = seam
        .execute(&SqlStatement { sql, args: vec![] })
        .with_context(|| format!("read {column} from {table}"))?;
    result
        .rows
        .into_iter()
        .map(|row| match row.into_iter().next() {
            Some(Value::Text(text)) => Ok(text),
            other => bail!("{column} in {table} is not text: {other:?}"),
        })
        .collect()
}

/// `/clock/tick`'s answer at one `every`/`bucket` pair, called directly since
/// the resident loop never re-claims a demand delta.
fn clock_answer(
    plan: &HostPlanData,
    rel_columns: &HashMap<String, Vec<String>>,
    every: i64,
    bucket: i64,
    sign: ArrivalSign,
) -> Result<Vec<Arrival>> {
    let mut inputs = BTreeMap::new();
    inputs.insert("every".to_string(), ScalarValue::Integer(every));
    let answered = vec![crate::hosts::host_row([(
        "bucket",
        serde_json::json!(bucket),
    )])];
    hosts::project_host_answer(plan, inputs, &answered, rel_columns, sign)
        .map_err(|failure| anyhow!("{failure}"))
}

/// `/soopy/watch`'s answer for every `(path, digest)` entry one glob currently
/// carries.
fn watch_answer_rows(
    plan: &HostPlanData,
    rel_columns: &HashMap<String, Vec<String>>,
    glob: &str,
    entries: &[(String, String)],
    sign: ArrivalSign,
) -> Result<Vec<Arrival>> {
    let mut inputs = BTreeMap::new();
    inputs.insert("glob".to_string(), ScalarValue::Text(glob.to_string()));
    let answered: Vec<_> = entries
        .iter()
        .map(|(path, digest)| {
            crate::hosts::host_row([
                ("path", serde_json::json!(path)),
                ("digest", serde_json::json!(digest)),
            ])
        })
        .collect();
    hosts::project_host_answer(plan, inputs, &answered, rel_columns, sign)
        .map_err(|failure| anyhow!("{failure}"))
}

/// Tick 0's rows for every continuing plan, read off the demand table the
/// boot facts already cascaded into.
pub fn continuing_seed_arrivals(
    program: &GenProgram,
    seam: &SqliteSeam,
    adapter_rows: &[HostAdapterRow],
    root: &Path,
) -> Result<Vec<Arrival>> {
    let mut seeds = Vec::new();
    for plan in continuing_plans(program, adapter_rows) {
        let table = relation_table(program, &plan.demand_rel)?;
        let column = plan_input_column(plan)?;
        match plan.name.as_str() {
            "clock__tick" => {
                for every in distinct_ints(seam, table, column)? {
                    let bucket = ClockExecutor::bucket_of(every);
                    seeds.extend(clock_answer(
                        plan,
                        &program.rel_columns,
                        every,
                        bucket,
                        ArrivalSign::Add,
                    )?);
                }
            }
            "soopy__watch" => {
                for glob in distinct_texts(program, seam, table, &plan.demand_rel, column)? {
                    let entries = SoopyWatchExecutor::enumerate_glob(root, &glob)
                        .map_err(|failure| anyhow!("{failure}"))?;
                    seeds.extend(watch_answer_rows(
                        plan,
                        &program.rel_columns,
                        &glob,
                        &entries,
                        ArrivalSign::Add,
                    )?);
                }
            }
            other => bail!("{other} declares a continuing cadence with no resident-loop driver"),
        }
    }
    Ok(seeds)
}

/// How often the loop wakes to notice a stop request while nothing else moves.
const STOP_POLL: Duration = Duration::from_millis(250);

// The glob a wakeup came from does not narrow the answer, because one
// enumeration reads every open glob, so `Moved` carries no payload.
enum WatchEvent {
    Moved,
    Closed(String, String),
}

/// One bucket counter per declared `every`, keyed for its plan's own re-answer.
struct ClockSource<'p> {
    plan: &'p HostPlanData,
    buckets: BTreeMap<i64, i64>,
}

impl<'p> ClockSource<'p> {
    fn open(plan: &'p HostPlanData, periods: &[i64]) -> ClockSource<'p> {
        ClockSource {
            plan,
            buckets: periods.iter().map(|period| (*period, -1)).collect(),
        }
    }

    fn seed_arrivals(&mut self, rel_columns: &HashMap<String, Vec<String>>) -> Result<Vec<Arrival>> {
        let mut arrivals = Vec::new();
        let periods: Vec<i64> = self.buckets.keys().copied().collect();
        for period in periods {
            let bucket = ClockExecutor::bucket_of(period);
            self.buckets.insert(period, bucket);
            arrivals.extend(clock_answer(
                self.plan,
                rel_columns,
                period,
                bucket,
                ArrivalSign::Add,
            )?);
        }
        Ok(arrivals)
    }

    fn due_arrivals(&mut self, rel_columns: &HashMap<String, Vec<String>>) -> Result<Vec<Arrival>> {
        let mut arrivals = Vec::new();
        let periods: Vec<i64> = self.buckets.keys().copied().collect();
        for period in periods {
            let bucket = ClockExecutor::bucket_of(period);
            let held = self.buckets[&period];
            if bucket == held {
                continue;
            }
            arrivals.extend(clock_answer(
                self.plan,
                rel_columns,
                period,
                held,
                ArrivalSign::Del,
            )?);
            arrivals.extend(clock_answer(
                self.plan,
                rel_columns,
                period,
                bucket,
                ArrivalSign::Add,
            )?);
            self.buckets.insert(period, bucket);
        }
        Ok(arrivals)
    }

    /// How long the loop may park before the earliest cadence turns over.
    fn next_fire(&self) -> Option<Duration> {
        let now = ClockExecutor::bucket_of(1);
        self.buckets
            .keys()
            .map(|period| {
                let elapsed = now % *period;
                Duration::from_secs((*period - elapsed).max(1) as u64)
            })
            .min()
    }
}

/// The watcher NOTIFIES and `SoopyWatchExecutor::enumerate_glob` ANSWERS, keyed
/// for its plan's own re-answer.
struct WatchSource<'p> {
    plan: &'p HostPlanData,
    root: PathBuf,
    globs: Vec<String>,
    open: BTreeSet<String>,
    /// The rows the rel currently holds, so a re-enumeration is a set difference.
    held: BTreeMap<(String, String), String>,
}

impl<'p> WatchSource<'p> {
    fn new(plan: &'p HostPlanData, root: &Path, globs: &[String]) -> WatchSource<'p> {
        WatchSource {
            plan,
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

    fn enumerate(&self) -> Result<BTreeMap<(String, String), String>> {
        let mut rows = BTreeMap::new();
        for glob in self.globs.iter().filter(|glob| self.open.contains(*glob)) {
            let entries = SoopyWatchExecutor::enumerate_glob(&self.root, glob)
                .map_err(|failure| anyhow!("{failure}"))?;
            for (path, digest) in entries {
                rows.insert((glob.clone(), path), digest);
            }
        }
        Ok(rows)
    }

    /// Tick 0's rows.
    fn snapshot_arrivals(
        &mut self,
        rel_columns: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Arrival>> {
        self.held = self.enumerate()?;
        let mut arrivals = Vec::new();
        for glob in &self.globs {
            let entries: Vec<(String, String)> = self
                .held
                .iter()
                .filter(|((held_glob, _), _)| held_glob == glob)
                .map(|((_, path), digest)| (path.clone(), digest.clone()))
                .collect();
            if !entries.is_empty() {
                arrivals.extend(watch_answer_rows(
                    self.plan,
                    rel_columns,
                    glob,
                    &entries,
                    ArrivalSign::Add,
                )?);
            }
        }
        Ok(arrivals)
    }

    /// A save that changed no bytes re-enumerates to the same digest and is zero
    /// delta at the rel boundary, so nothing downstream re-derives.
    fn refresh_arrivals(
        &mut self,
        rel_columns: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Arrival>> {
        let next = self.enumerate()?;
        let mut arrivals = Vec::new();
        for glob in &self.globs {
            let dels: Vec<(String, String)> = self
                .held
                .iter()
                .filter(|(key, _)| &key.0 == glob)
                .filter(|(key, digest)| next.get(*key) != Some(*digest))
                .map(|(key, digest)| (key.1.clone(), digest.clone()))
                .collect();
            let adds: Vec<(String, String)> = next
                .iter()
                .filter(|(key, _)| &key.0 == glob)
                .filter(|(key, digest)| self.held.get(*key) != Some(*digest))
                .map(|(key, digest)| (key.1.clone(), digest.clone()))
                .collect();
            if !dels.is_empty() {
                arrivals.extend(watch_answer_rows(
                    self.plan,
                    rel_columns,
                    glob,
                    &dels,
                    ArrivalSign::Del,
                )?);
            }
            if !adds.is_empty() {
                arrivals.extend(watch_answer_rows(
                    self.plan,
                    rel_columns,
                    glob,
                    &adds,
                    ArrivalSign::Add,
                )?);
            }
        }
        self.held = next;
        Ok(arrivals)
    }
}

/// Fold, print the finals, then stay up: one continuing re-answer is one tick.
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

    let adapter_rows = crate::types::load_program_host_adapter_rows(&program.name)
        .context("read the process adapter sidecar")?;
    let plans = continuing_plans(program, &adapter_rows);
    if plans.is_empty() {
        bail!(
            "{} declares no continuing executor, so a watch would never tick; \
             run it once with `dl6 run`",
            program.name
        );
    }
    let clock_plan = plans.iter().find(|plan| plan.name == "clock__tick").copied();
    let watch_plan = plans.iter().find(|plan| plan.name == "soopy__watch").copied();
    if let Some(other) = plans
        .iter()
        .find(|plan| plan.name != "clock__tick" && plan.name != "soopy__watch")
    {
        bail!("{} declares a continuing cadence with no resident-loop driver", other.name);
    }

    let mut clocks = match clock_plan {
        Some(plan) => {
            let table = relation_table(program, &plan.demand_rel)?;
            let column = plan_input_column(plan)?;
            let periods = distinct_ints(&seam, table, column)?;
            Some(ClockSource::open(plan, &periods))
        }
        None => None,
    };
    let mut sources = match watch_plan {
        Some(plan) => {
            let table = relation_table(program, &plan.demand_rel)?;
            let column = plan_input_column(plan)?;
            let globs = distinct_texts(program, &seam, table, &plan.demand_rel, column)?;
            Some(WatchSource::new(plan, &options.root, &globs))
        }
        None => None,
    };

    let (events, inbox) = mpsc::channel::<WatchEvent>();
    if let Some(sources) = sources.as_mut() {
        sources.arm(events)?;
    }

    let mut first = seeds;
    if let Some(clocks) = clocks.as_mut() {
        first.extend(clocks.seed_arrivals(&program.rel_columns)?);
    }
    if let Some(sources) = sources.as_mut() {
        first.extend(sources.snapshot_arrivals(&program.rel_columns)?);
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

    loop {
        if *stop.borrow_and_update() {
            break;
        }
        let deadline = clocks
            .as_ref()
            .and_then(ClockSource::next_fire)
            .unwrap_or(STOP_POLL)
            .min(STOP_POLL);
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
                            if let Some(sources) = sources.as_mut() {
                                sources.close(&glob);
                            }
                            tracing::warn!(glob, failure, "watch: a file source closed");
                        }
                    }
                }
                if moved {
                    if let Some(sources) = sources.as_mut() {
                        batch.extend(sources.refresh_arrivals(&program.rel_columns)?);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if clocks.is_none() {
                    break;
                }
            }
        }
        if let Some(clocks) = clocks.as_mut() {
            batch.extend(clocks.due_arrivals(&program.rel_columns)?);
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
