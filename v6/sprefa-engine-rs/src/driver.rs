// The single root driver (the v6 one-subscribe law). Opens the connection,
// runs DDL, runs boot, then folds the schedule one batch per tick, driving
// each tick's Stream to completion and printing the ticklog line. stdout
// carries the tick log and nothing else.

use futures::StreamExt;

use crate::program::{run_boot, GenProgram};
use crate::sql::SqliteSeam;
use crate::ticklog::tick_line;
use crate::types::{Arrival, TickDeltas};

pub struct TickFold {
    pub lines: Vec<String>,
}

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
) -> TickFold {
    seam.run_ddl(&program.ddl).expect("DDL execution failed");
    run_boot(seam, &program.boot);
    let mut lines = Vec::new();
    for (tick_index, batch) in schedule.iter().enumerate() {
        let tick_number = tick_index + 1;
        let deltas = drive_tick(program, seam, batch.clone()).await;
        lines.push(format_deltas(program, tick_number, &deltas));
        let mut drain_tick = tick_number + 1;
        let mut carry = deltas.carry_pending;
        let mut drained = 0;
        while carry && drained < drain_cap {
            let empty = Vec::new();
            let deltas = drive_tick(program, seam, empty).await;
            lines.push(format_deltas(program, drain_tick, &deltas));
            carry = deltas.carry_pending;
            drain_tick += 1;
            drained += 1;
        }
    }
    TickFold { lines }
}

// Drive the tick's Stream to its single item. A stream constructed and never
// driven silently produces nothing; this is the drain every tick must get.
pub async fn drive_tick(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: Vec<Arrival>,
) -> TickDeltas {
    let mut stream = program.tick(seam, arrivals);
    stream.next().await.expect("tick stream produced no item")
}
