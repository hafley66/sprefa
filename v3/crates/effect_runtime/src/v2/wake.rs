//! Wake — what makes a parked queue row runnable.
//!
//! Three flavors:
//!   - `Immediate`         — driver picks up on next sweep.
//!   - `Tick { past_tick }` — global drive_tick must advance past it.
//!   - `Key { domain, key }` — park inside the queue under a (domain,
//!     key) tag. Producers call `queue.dispatch_park(domain, Some(key))`
//!     to flip every matching parked row to `Wake::Immediate` in one
//!     indexed update. Domain-only dispatch (`dispatch_park(d, None)`)
//!     promotes every parked row in that domain.
//!
//! The park subscription lives in the queue itself, not a separate
//! listener map. At 10k parked rows this avoids 10k closure
//! registrations on the bus.

use std::borrow::Cow;

use super::next_key::NextKey;

#[derive(Debug, Clone)]
pub enum Wake {
    Immediate,
    Tick { past_tick: u64 },
    Key { domain: Cow<'static, str>, key: NextKey },
}
