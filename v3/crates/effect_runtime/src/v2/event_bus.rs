//! `EventBus` — cache-invalidation fan-out only.
//!
//! After the park-as-row redesign, wake subscriptions live in the
//! queue (`QueueBackend::dispatch_park`), not the bus. The bus is now
//! a one-call fan-out for cache listeners (`MemoCache`, `QueryCache`,
//! telemetry). One event flavor:
//!
//!   - `Dirty { domain, key }` — `domain` names a cache scope (e.g.
//!     "fs"), `key = None` invalidates the whole scope, `key =
//!     Some(k)` invalidates a single entry.

use std::borrow::Cow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::next_key::NextKey;

#[derive(Debug, Clone)]
pub enum Event {
    Dirty { domain: Cow<'static, str>, key: Option<NextKey> },
}

/// Listener for cache-invalidation / telemetry events.
pub trait BusListener: Send + Sync + 'static {
    fn on_event(&self, ev: &Event);
}

pub struct EventBus {
    counter:   AtomicU64,
    listeners: Mutex<Vec<std::sync::Arc<dyn BusListener>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            counter:   AtomicU64::new(1),
            listeners: Mutex::new(Vec::new()),
        }
    }

    /// Mint an opaque, content-shaped key for a one-off pause point.
    /// Useful when a Component does not have a structural NextKey yet.
    pub fn fresh_key(&self) -> NextKey {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&n.to_le_bytes());
        NextKey(bytes)
    }

    pub fn add_listener(&self, l: std::sync::Arc<dyn BusListener>) {
        self.listeners.lock().unwrap().push(l);
    }

    pub fn dispatch(&self, ev: Event) {
        let listeners = self.listeners.lock().unwrap().clone();
        // Listeners fire outside the lock so they can re-enter the bus
        // (e.g. cache drops triggering further dispatches).
        for l in listeners { l.on_event(&ev); }
    }

    /// Convenience: dispatch a `Dirty` event with the given domain and key.
    pub fn dispatch_dirty(&self, domain: impl Into<Cow<'static, str>>, key: Option<NextKey>) {
        self.dispatch(Event::Dirty { domain: domain.into(), key });
    }
}

impl Default for EventBus {
    fn default() -> Self { Self::new() }
}
