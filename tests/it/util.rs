//! Shared helpers for it-tests that spawn a real `dl --daemon` child.

use std::ops::{Deref, DerefMut};
use std::process::Child;

/// Kill-on-drop wrapper for a spawned daemon child. If a test panics (or
/// returns) while the child is still running, `Drop` kills and reaps it so a
/// leaked daemon can't keep running in its tempdir sandbox, print `[daemon]`
/// chatter over the suite summary, or hold the write end of a pipe wrapping
/// `cargo test` output. A test that shut the daemon down cleanly has an
/// already-exited child, so `Drop` is a no-op.
pub struct DaemonGuard(pub Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Deref for DaemonGuard {
    type Target = Child;
    fn deref(&self) -> &Child { &self.0 }
}

impl DerefMut for DaemonGuard {
    fn deref_mut(&mut self) -> &mut Child { &mut self.0 }
}
