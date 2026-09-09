//! The on-disk run trail: one row per extract run plus its phase rows, in the
//! one dl6 store. A run that was slow explains itself after the process is gone.
//! @comment-ok: module header, the shape every module here opens with

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::lang::DL6_DB_RELATIVE_PATH;
use crate::trace::RunSnapshot;

/// `lang` and `phase` stay TEXT: under 50 rows per run and the query surface is
/// `sqlite3 ~/.agent/dl6.db` by hand, so a dictionary table would only cost a join.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS extract_run (
  \"__id\" INTEGER PRIMARY KEY, started_utc TEXT NOT NULL, git_sha TEXT, argv TEXT NOT NULL,
  wall_ms INTEGER NOT NULL, load_start REAL NOT NULL, load_end REAL NOT NULL, pid INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS extract_phase (
  \"__id\" INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES extract_run(\"__id\"),
  lang TEXT NOT NULL, phase TEXT NOT NULL, files INTEGER NOT NULL, calls INTEGER NOT NULL,
  rows INTEGER NOT NULL, bytes INTEGER NOT NULL, micros INTEGER NOT NULL,
  UNIQUE (run_id, lang, phase));
CREATE INDEX IF NOT EXISTS extract_phase_run ON extract_phase(run_id);";

/// Why a trail write did not happen. A trail is instrumentation, so every arm
/// here is reported and swallowed by the caller, never propagated as the run's.
#[derive(Debug)]
pub enum TrailError {
    Open(rusqlite::Error),
    Write(rusqlite::Error),
    Home,
}

impl std::fmt::Display for TrailError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(error) => write!(formatter, "open the trail: {error}"),
            Self::Write(error) => write!(formatter, "write the trail: {error}"),
            Self::Home => write!(
                formatter,
                "HOME is unset, so {DL6_DB_RELATIVE_PATH} has no path"
            ),
        }
    }
}

impl std::error::Error for TrailError {}

/// One run of the trail as `--trail` prints it. The phase tuple is
/// (lang, phase, files, calls, rows, bytes, micros).
pub struct RunReport {
    pub id: u64,
    pub started: String,
    pub argv: String,
    pub wall_ms: u64,
    pub load_start: f64,
    pub load_end: f64,
    pub phases: Vec<(String, String, u64, u64, u64, u64, u64)>,
}

/// A write handle on the one dl6 store's two extract tables.
pub struct Trail {
    conn: Connection,
}

impl Trail {
    /// `$HOME/.agent/dl6.db`, tables created if absent.
    pub fn open() -> Result<Trail, TrailError> {
        let home = std::env::var_os("HOME").ok_or(TrailError::Home)?;
        Trail::open_at(&PathBuf::from(home).join(DL6_DB_RELATIVE_PATH))
    }

    /// The same door on an explicit path, so a test names its own store.
    pub fn open_at(path: &Path) -> Result<Trail, TrailError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                TrailError::Open(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
        }
        let conn = Connection::open(path).map_err(TrailError::Open)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(TrailError::Open)?;
        conn.execute_batch(SCHEMA).map_err(TrailError::Open)?;
        Ok(Trail { conn })
    }

    /// One run row and ONE multi-row insert for its phases, in one transaction.
    pub fn write(
        &self,
        run: &RunSnapshot,
        argv: &[String],
        git_sha: Option<&str>,
    ) -> Result<u64, TrailError> {
        let started = format_utc(run.started);
        let wall_ms = run.wall.as_millis() as u64;
        self.conn
            .execute_batch("BEGIN")
            .map_err(TrailError::Write)?;
        let outcome = self.write_rows(&started, wall_ms, run, argv, git_sha);
        match outcome {
            Ok(id) => {
                self.conn
                    .execute_batch("COMMIT")
                    .map_err(TrailError::Write)?;
                Ok(id)
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn write_rows(
        &self,
        started: &str,
        wall_ms: u64,
        run: &RunSnapshot,
        argv: &[String],
        git_sha: Option<&str>,
    ) -> Result<u64, TrailError> {
        self.conn
            .execute(
                "INSERT INTO extract_run (started_utc, git_sha, argv, wall_ms, load_start, load_end, pid) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    started,
                    git_sha,
                    argv.join(" "),
                    wall_ms as i64,
                    run.load_start,
                    run.load_end,
                    i64::from(std::process::id()),
                ],
            )
            .map_err(TrailError::Write)?;
        let id = self.conn.last_insert_rowid();
        if run.phases.is_empty() {
            return Ok(id as u64);
        }
        // ONE statement for every phase row: a statement per row is the N+1 this
        // repo bans, and the row set is already in hand.
        let placeholders = (0..run.phases.len())
            .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?)")
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            "INSERT INTO extract_phase (run_id, lang, phase, files, calls, rows, bytes, micros) VALUES {placeholders}"
        );
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(run.phases.len() * 8);
        for phase in &run.phases {
            values.push(Box::new(id));
            values.push(Box::new(phase.lang.clone()));
            values.push(Box::new(phase.phase.as_str()));
            values.push(Box::new(phase.files as i64));
            values.push(Box::new(phase.calls as i64));
            values.push(Box::new(phase.rows as i64));
            values.push(Box::new(phase.bytes as i64));
            values.push(Box::new(phase.micros as i64));
        }
        let bound: Vec<&dyn rusqlite::ToSql> = values.iter().map(AsRef::as_ref).collect();
        self.conn
            .execute(&statement, rusqlite::params_from_iter(bound))
            .map_err(TrailError::Write)?;
        Ok(id as u64)
    }

    /// The last `n` runs, newest first, each with its phase rows.
    pub fn recent(&self, n: usize) -> Result<Vec<RunReport>, TrailError> {
        let mut runs = self
            .conn
            .prepare(
                "SELECT \"__id\", started_utc, argv, wall_ms, load_start, load_end \
                 FROM extract_run ORDER BY \"__id\" DESC LIMIT ?1",
            )
            .map_err(TrailError::Open)?;
        let mut reports: Vec<RunReport> = runs
            .query_map([n as i64], |row| {
                Ok(RunReport {
                    id: row.get::<_, i64>(0)? as u64,
                    started: row.get(1)?,
                    argv: row.get(2)?,
                    wall_ms: row.get::<_, i64>(3)? as u64,
                    load_start: row.get(4)?,
                    load_end: row.get(5)?,
                    phases: Vec::new(),
                })
            })
            .map_err(TrailError::Open)?
            .collect::<Result<_, _>>()
            .map_err(TrailError::Open)?;
        if reports.is_empty() {
            return Ok(reports);
        }
        // One query for every phase row of the whole page, grouped in Rust: a
        // query per run is the N+1 on the read side.
        let ids: Vec<i64> = reports.iter().map(|report| report.id as i64).collect();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let mut phases = self
            .conn
            .prepare(&format!(
                "SELECT run_id, lang, phase, files, calls, rows, bytes, micros \
                 FROM extract_phase WHERE run_id IN ({placeholders}) ORDER BY micros DESC"
            ))
            .map_err(TrailError::Open)?;
        let found = phases
            .query_map(rusqlite::params_from_iter(ids), |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    (
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)? as u64,
                        row.get::<_, i64>(4)? as u64,
                        row.get::<_, i64>(5)? as u64,
                        row.get::<_, i64>(6)? as u64,
                        row.get::<_, i64>(7)? as u64,
                    ),
                ))
            })
            .map_err(TrailError::Open)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(TrailError::Open)?;
        for (run_id, phase) in found {
            if let Some(report) = reports.iter_mut().find(|report| report.id == run_id) {
                report.phases.push(phase);
            }
        }
        Ok(reports)
    }
}

/// RFC 3339 to the second, UTC, with no chrono in the graph for one timestamp.
fn format_utc(at: std::time::SystemTime) -> String {
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's days-from-civil, inverted. Exact for every day this runs on.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (year + i64::from(month <= 2), month, day)
}
