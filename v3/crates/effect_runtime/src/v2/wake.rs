//! Wake — what makes a parked queue row runnable.
//!
//! Three flavors:
//!   - `Immediate`    — driver picks up on next sweep.
//!   - `Tick`         — global drive_tick must advance past `past_tick`.
//!   - `Key(NextKey)` — park against a content-derived key. Whoever
//!                      drives the wake calls `bus.dispatch(KeyDirty)`,
//!                      `PathDirty`, or `DomainDirty` on the
//!                      `EventBus`. The driver consults
//!                      `bus.snapshot_ready()` per iteration to find
//!                      runnable keys.

use super::next_key::NextKey;

#[derive(Debug, Clone)]
pub enum Wake {
    Immediate,
    Tick { past_tick: u64 },
    Key(NextKey),
}
