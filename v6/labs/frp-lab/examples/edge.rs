//! EDGE, stream form — the pipeline that WORKS and is where FRP earns its keep.
//!
//!   cargo run --example edge
//!
//! A scripted event log (watcher saves + duplicate saves + a git move + tick
//! boundaries) runs through the real buffer/groupBy/distinct/emit trigger. Every
//! event is owned, so the whole `futures::Stream` graph is `'static` — no borrow
//! rides along. This is the crude-rx upgrade the session asked for.

use frp_lab::{run_trigger, DeriveJob, Event, Family};

fn main() {
    // A tick's worth of watcher noise: a.rs changed twice (2nd is a NO-OP dupe by
    // digest), b.rs and a .ts changed, git HEAD moved. Then Tick flushes. Then a
    // second window where a.rs changes for real (new digest) + Tick.
    let log = vec![
        Event::FileChanged { path: "a.rs".into(), digest: 0x11 },
        Event::FileChanged { path: "b.rs".into(), digest: 0x22 },
        Event::FileChanged { path: "a.rs".into(), digest: 0x11 }, // dupe digest -> dropped
        Event::FileChanged { path: "ui.ts".into(), digest: 0x33 },
        Event::GitHead("refs/heads/v11".into()),
        Event::Tick,
        Event::FileChanged { path: "a.rs".into(), digest: 0x99 }, // real change -> kept
        Event::Tick,
    ];

    let jobs: Vec<DeriveJob> = futures::executor::block_on(run_trigger(futures::stream::iter(log)));

    println!("EDGE trigger — {} coalesced jobs from the event log:", jobs.len());
    for j in &jobs {
        println!("  {:?}  paths={:?}  head={:?}", j.family, j.paths, j.head);
    }

    // Window 1: a.rs (once, dupe dropped) + b.rs under Rust; ui.ts under Ts.
    // Window 2: a.rs again (new digest passed the distinct gate).
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].family, Family::Rust);
    assert_eq!(jobs[0].paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
    assert_eq!(jobs[1].family, Family::Ts);
    assert_eq!(jobs[2].paths, vec!["a.rs".to_string()]);
    println!("\nWhole graph is 'static + Send. No lifetime, no clone-to-appease, no rayon shattered.");
}
