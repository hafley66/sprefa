//! Minimal salsa 0.28 database with an event log, so the examples can SHOW which
//! queries executed vs were validated from memo.

use std::sync::{Arc, Mutex};

/// A salsa database that records every event salsa emits. `log` is shared so an
/// example can drain it between steps and print exactly what salsa did.
#[salsa::db]
#[derive(Clone)]
pub struct Db {
    storage: salsa::Storage<Self>,
    pub log: Arc<Mutex<Vec<String>>>,
}

impl Default for Db {
    fn default() -> Self {
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        Self {
            storage: salsa::Storage::new(Some(Box::new(move |ev| {
                // We only care about the two events that answer "did it run?":
                // WillExecute = the query body ran; DidValidateMemoizedValue = it
                // was reused from memo without running.
                let line = match ev.kind {
                    salsa::EventKind::WillExecute { .. } => Some(format!("EXECUTE  {:?}", ev.kind)),
                    salsa::EventKind::DidValidateMemoizedValue { .. } => {
                        Some(format!("VALIDATE {:?}", ev.kind))
                    }
                    _ => None,
                };
                if let Some(line) = line {
                    log2.lock().unwrap().push(line);
                }
            }))),
            log,
        }
    }
}

#[salsa::db]
impl salsa::Database for Db {}

impl Db {
    /// Drain the event log and return the (execute, validate) counts + the lines.
    pub fn drain(&self) -> (usize, usize, Vec<String>) {
        let mut g = self.log.lock().unwrap();
        let lines: Vec<String> = std::mem::take(&mut *g);
        let exec = lines.iter().filter(|l| l.starts_with("EXECUTE")).count();
        let val = lines.iter().filter(|l| l.starts_with("VALIDATE")).count();
        (exec, val, lines)
    }
}

pub fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    ru.ru_maxrss as f64 / (1024.0 * 1024.0) // darwin: bytes
}
