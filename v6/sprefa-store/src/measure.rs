//! The ONE uniform measurement path. Recursive-CTE RAM is not guessable from row
//! counts, so every perf run captures the SAME sensor set at the SAME phase
//! boundaries through `run_cell`. No example may read a sensor by hand — that
//! makes its numbers incomparable and disqualifies them from the golden archive.
//!
//! FROZEN CONTRACT: `v6/findings/INSIGHTS.md` §C. Sink: `v6/labs/perf-runs.sqlite`.

/// Independent variables — one OS process per Cell.
#[derive(Clone, Debug)]
pub struct Cell {
    pub engine: &'static str,
    pub workload: &'static str,
    pub nodes: i64,
    pub edges: i64,
    pub cache_size_kib: i64,
    pub memcap_mb: u64,
}

/// Captured identically at each phase boundary ("build" | "insert" | "op").
#[derive(Clone, Debug)]
pub struct PhaseSample {
    pub phase: &'static str,
    pub t_ms: f64,
    pub rss_kb: i64,
    pub sqlite_hw_kb: i64,
    pub disk_read: i64,
    pub disk_write: i64,
    pub cache_hit: i64,
    pub cache_miss: i64,
    pub cache_write: i64,
}

#[derive(Clone, Debug)]
pub struct RunRow {
    pub cell: Cell,
    pub samples: Vec<PhaseSample>,
    pub correct: bool,
    pub out_hash: String,
    pub aborted: bool,
}

// job B ("luna-role") fills:
//   fn peak_rss_kb() / sqlite_hw_kb() / diskio() / db_status_cache() — ONE impl each
//   pub async fn run_cell<S,O>(cell, build, op) -> RunRow
//   fn append_run(&RunRow) -> perf-runs.sqlite  (schema: runs / phase_samples)
// See INSIGHTS §C. Sensors must match the golden-data contract (v6/labs/AGENTS.md).
