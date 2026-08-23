// The single root driver (the v6 one-subscribe law). Opens the connection,
// runs DDL, runs boot, then folds the schedule one batch per tick, driving
// each tick's Stream to completion and printing the ticklog line. stdout
// carries the tick log and nothing else.

use futures::StreamExt;
use std::time::Duration;

use crate::program::{run_boot, GenProgram};
use crate::sql::SqliteSeam;
use crate::ticklog::tick_line;
use crate::types::{Arrival, BoundaryError, BoundaryResult, TickDeltas};

pub struct TickFold {
    pub lines: Vec<String>,
}

// The two ways a live-host fold stops: the runtime boundary, or the host.
#[derive(Debug, Clone, PartialEq)]
pub enum RunError {
    Boundary(BoundaryError),
    Host(crate::hosts::HostError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Boundary(error) => write!(f, "{error}"),
            RunError::Host(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<BoundaryError> for RunError {
    fn from(error: BoundaryError) -> RunError {
        RunError::Boundary(error)
    }
}

impl From<crate::hosts::HostError> for RunError {
    fn from(error: crate::hosts::HostError) -> RunError {
        RunError::Host(error)
    }
}

/// An enum-typed column holds a reference, so it keeps the `int` type the
/// compiler already declares for it; the enum plane moves no boundary type.
pub fn format_deltas(program: &GenProgram, tick: usize, deltas: &TickDeltas) -> String {
    tick_line(
        tick,
        deltas,
        &program.rel_column_types,
        &program.rel_column_types,
    )
}

pub async fn run_schedule(
    program: &GenProgram,
    seam: &SqliteSeam,
    schedule: &[Vec<Arrival>],
    drain_cap: usize,
) -> BoundaryResult<TickFold> {
    crate::trace::arm();
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(seam, &program.boot);
    let mut lines = Vec::new();
    let mut tick_number = 0usize;
    let mut carry_pending = false;
    let mut drains_used = 0usize;
    loop {
        let drains = tick_number >= schedule.len();
        let arrivals = match schedule.get(tick_number) {
            Some(batch) => batch.clone(),
            None if carry_pending => Vec::new(),
            None => break,
        };
        if drains && drains_used >= drain_cap {
            panic!(
                "drain overflow: {} exceeded {} drain ticks",
                program.name, drain_cap
            );
        }
        let deltas = {
            let span =
                tracing::info_span!("tick", tick = tick_number, arrivals = arrivals.len());
            let _entered = span.enter();
            drive_tick_transacted(program, seam, arrivals).await?
        };
        tick_number += 1;
        if drains {
            drains_used += 1;
        }
        carry_pending = deltas.carry_pending;
        {
            let _scope = crate::trace::Scope::phase("ticklog");
            lines.push(format_deltas(program, tick_number, &deltas));
        }
    }
    crate::sql::report_seam_tally();
    crate::trace::report();
    Ok(TickFold { lines })
}

// Live-host variant: each tick's demand +deltas execute and the projected
// response rows become the NEXT tick's batch, where a fixture scripts them.
pub async fn run_schedule_live(
    program: &GenProgram,
    seam: &SqliteSeam,
    schedule: &[Vec<Arrival>],
    drain_cap: usize,
) -> Result<TickFold, RunError> {
    crate::trace::arm();
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(seam, &program.boot);
    let adapter_rows =
        crate::types::load_program_host_adapter_rows(&program.name).map_err(|error| {
            crate::hosts::HostError {
                host: program.name.clone(),
                message: format!("read process adapter sidecar: {error}"),
            }
        })?;
    let mut runner = crate::hosts::HostLiveRunner::with_adapter_rows(
        &program.host_plans,
        &program.rel_columns,
        &adapter_rows,
    )?;
    let mut pending: std::collections::VecDeque<Vec<Arrival>> = std::collections::VecDeque::new();
    let mut lines = Vec::new();
    let mut tick_number = 0usize;
    let mut schedule_index = 0usize;
    let mut carry_pending = false;
    let mut off_schedule_ticks = 0usize;
    loop {
        let (arrivals, scheduled) = match pending.pop_front() {
            Some(batch) => (batch, false),
            None => match schedule.get(schedule_index) {
                Some(batch) => {
                    schedule_index += 1;
                    (batch.clone(), true)
                }
                None if carry_pending => (Vec::new(), false),
                None => break,
            },
        };
        if !scheduled {
            off_schedule_ticks += 1;
            if off_schedule_ticks > drain_cap {
                panic!(
                    "drain overflow: {} exceeded {} host/drain ticks",
                    program.name, drain_cap
                );
            }
        }
        let arrival_rows = arrivals.len();
        let deltas = {
            let span = tracing::info_span!("tick", tick = tick_number, arrivals = arrival_rows);
            let _entered = span.enter();
            drive_tick_transacted(program, seam, arrivals).await?
        };
        tick_number += 1;
        carry_pending = deltas.carry_pending;
        {
            let _scope = crate::trace::Scope::phase("ticklog");
            lines.push(format_deltas(program, tick_number, &deltas));
        }
        let responses = {
            let _scope = crate::trace::Scope::phase("host_collect");
            let span = tracing::info_span!("host_collect", tick = tick_number);
            let _entered = span.enter();
            runner.collect(&deltas)?
        };
        if !responses.is_empty() {
            pending.push_back(responses);
        }
    }
    crate::sql::report_seam_tally();
    crate::trace::report();
    Ok(TickFold { lines })
}

// Drive the tick's Stream to its single item. A stream constructed and never
// driven silently produces nothing; this is the drain every tick must get.
pub async fn drive_tick(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: Vec<Arrival>,
) -> BoundaryResult<TickDeltas> {
    let mut stream = program.tick(seam, arrivals);
    stream.next().await.expect("tick stream produced no item")
}

/// `drive_tick` wrapped in one SQLite transaction: a tick killed partway
/// through never leaves a base table half-promoted (docs/failure-modes.md).
pub async fn drive_tick_transacted(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: Vec<Arrival>,
) -> BoundaryResult<TickDeltas> {
    seam.begin_tick()
        .unwrap_or_else(|error| panic!("tick transaction: {error}"));
    match drive_tick(program, seam, arrivals).await {
        Ok(deltas) => {
            seam.commit_tick().expect("COMMIT failed");
            Ok(deltas)
        }
        Err(error) => {
            seam.rollback_tick().expect("ROLLBACK failed");
            Err(error)
        }
    }
}

/// Feed one debounced Soopy worktree batch through the same SourceBind tick
/// path used by explicit source requests. The optional result lets a driver
/// keep waiting through identical re-saves and backend notifications that
/// produce no logical source delta.
pub fn drive_watch_tick(
    bind: &mut crate::source_bind::SourceBind,
    program: &GenProgram,
    seam: &SqliteSeam,
    worktree: &soopy::WorktreeId,
    clock: u64,
    timeout: Duration,
) -> anyhow::Result<Option<crate::source_bind::SourceBindTickFrame>> {
    bind.run_watch_tick(program, seam, worktree, clock, timeout)
}
