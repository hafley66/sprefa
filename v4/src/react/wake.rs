// Wake — what makes a parked queue row runnable.
//
// The discriminator stored on every queue row. `Immediate` = the driver
// picks it up next sweep. Everything else is some predicate the driver
// re-evaluates on the relevant signal (sink commit, tick, external).

use crate::Gen;

pub type SinkId = u32;

#[derive(Debug, Clone)]
pub enum Wake {
    /// Driver picks up the row on next sweep, FIFO by drive_tick.
    Immediate,

    /// Park until the global tick counter advances past `past_tick`.
    /// Cheap "yield until next round of work."
    Tick { past_tick: Gen },

    /// Park until `sink` commits past `past_gen` AND the committed key
    /// has prefix `key_pfx`. `key_pfx` may be empty to wake on any
    /// commit to the named sink.
    SinkGen {
        sink:        SinkId,
        key_pfx:     Vec<u8>,
        past_gen:    Gen,
    },

    /// Park until an opaque external token is signalled. Wake source is
    /// outside the relational substrate (LSP request, fs notify, user
    /// hotkey). Held in a side-table; not commonly used.
    External(ExternalToken),
}

/// Opaque handle to an external wakeup channel. Held in a side-table on
/// the runtime; not part of the queue row's index keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalToken(pub u64);
