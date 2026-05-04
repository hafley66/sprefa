//! `EventBus` — unifies wake and cache-invalidation under one mechanism.
//!
//! Wake = a `NextKey` becomes "ready" and the driver pulls its row.
//! Cache invalidation = a `NextKey` becomes "dirty" and its memo entry
//! drops. Same dispatch surface, two consumer kinds.
//!
//! Three event flavors:
//!   - `KeyDirty(NextKey)`        — direct, used for ad-hoc wakes.
//!   - `PathDirty(prefix)`        — every subscriber whose registered
//!                                  path is a descendant of `prefix`
//!                                  is signaled. Powers cascade-delete
//!                                  / scoped invalidation.
//!   - `DomainDirty(&'static)`    — every subscriber tagged with the
//!                                  same domain string is signaled.
//!                                  Powers `invalidateQueries('fs')`
//!                                  shape from react-query.
//!
//! Sync. No tokio. Multiple subscribers can register against the same
//! key/path/domain — all of them get signaled on dispatch. The driver
//! consults `snapshot_ready()` per loop iteration.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::next_key::NextKey;

#[derive(Debug, Clone)]
pub enum Event {
    KeyDirty(NextKey),
    PathDirty(Vec<u32>),
    DomainDirty(&'static str),
}

/// Listener for general bus events (cache invalidation, telemetry).
/// Distinct from wake-by-key — a listener doesn't park, it reacts.
pub trait BusListener: Send + Sync + 'static {
    fn on_event(&self, ev: &Event);
}

pub struct EventBus {
    counter: AtomicU64,
    inner:   Mutex<Inner>,
}

struct Inner {
    ready:       HashSet<NextKey>,
    path_subs:   Vec<(Vec<u32>, NextKey)>,
    domain_subs: Vec<(&'static str, NextKey)>,
    listeners:   Vec<std::sync::Arc<dyn BusListener>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
            inner: Mutex::new(Inner {
                ready:       HashSet::new(),
                path_subs:   Vec::new(),
                domain_subs: Vec::new(),
                listeners:   Vec::new(),
            }),
        }
    }

    /// Mint an opaque, content-shaped key for a one-off pause point.
    /// Useful when a Component does not have a structural NextKey yet
    /// (e.g. wrapping a thread-spawned effect). Real keys for in-flight
    /// rows come from `next_key::compute_key`.
    pub fn fresh_key(&self) -> NextKey {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&n.to_le_bytes());
        NextKey(bytes)
    }

    pub fn subscribe_path(&self, path: Vec<u32>, key: NextKey) {
        self.inner.lock().unwrap().path_subs.push((path, key));
    }

    pub fn subscribe_domain(&self, domain: &'static str, key: NextKey) {
        self.inner.lock().unwrap().domain_subs.push((domain, key));
    }

    pub fn add_listener(&self, l: std::sync::Arc<dyn BusListener>) {
        self.inner.lock().unwrap().listeners.push(l);
    }

    pub fn dispatch(&self, ev: Event) {
        let listeners: Vec<std::sync::Arc<dyn BusListener>> = {
            let mut inner = self.inner.lock().unwrap();
            match &ev {
                Event::KeyDirty(k) => { inner.ready.insert(*k); }
                Event::PathDirty(prefix) => {
                    let to_wake: Vec<NextKey> = inner.path_subs.iter()
                        .filter(|(p, _)| has_prefix(p, prefix))
                        .map(|(_, k)| *k)
                        .collect();
                    for k in to_wake { inner.ready.insert(k); }
                }
                Event::DomainDirty(d) => {
                    let to_wake: Vec<NextKey> = inner.domain_subs.iter()
                        .filter(|(dd, _)| *dd == *d)
                        .map(|(_, k)| *k)
                        .collect();
                    for k in to_wake { inner.ready.insert(k); }
                }
            }
            inner.listeners.clone()
        };
        // Listeners fire outside the lock so they can re-enter the bus
        // (e.g. cache drops triggering further dispatches).
        for l in listeners { l.on_event(&ev); }
    }

    pub fn snapshot_ready(&self) -> Vec<NextKey> {
        self.inner.lock().unwrap().ready.iter().copied().collect()
    }

    pub fn forget(&self, key: NextKey) {
        let mut inner = self.inner.lock().unwrap();
        inner.ready.remove(&key);
        inner.path_subs.retain(|(_, k)| *k != key);
        inner.domain_subs.retain(|(_, k)| *k != key);
    }

    pub fn is_ready(&self, key: NextKey) -> bool {
        self.inner.lock().unwrap().ready.contains(&key)
    }

    pub fn ready_count(&self) -> usize {
        self.inner.lock().unwrap().ready.len()
    }
}

impl Default for EventBus {
    fn default() -> Self { Self::new() }
}

/// True if `path` starts with `prefix`. Empty prefix matches every path.
fn has_prefix(path: &[u32], prefix: &[u32]) -> bool {
    path.len() >= prefix.len() && path.iter().zip(prefix).all(|(a, b)| a == b)
}
