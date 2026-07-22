//! Break everything. Ramp each engine until it fails, each scale in an isolated child
//! process so a crash (the 5 GB gun firing = SIGABRT) is observed, not fatal.
//! Hypothesis: resident dd dies on RAM (gun), on-disk DRed dies on TIME (RAM bounded).
//!
//!   cargo run --release --example breaking_point --features with-dd

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::reach_dred::SqliteReachDRed;
use labkit::{gun, sqlmem};
use std::collections::HashSet;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

// ---- child: dd all-pairs reach at V nodes (resident) --------------------------
#[cfg(feature = "with-dd")]
fn child_dd(v: usize) {
    gun::install(5120);
    use labkit::reach::reach_stream;
    let mut e = labkit::DdReach::default();
    use labkit::Experiment;
    e.setup(v);
    let s = reach_stream(v, 0xBEEF, 5, 2);
    for (adds, rems) in &s.ticks {
        e.tick(adds, rems);
    }
    println!("OK v={v} allocMB={:.0} live={}", gun::peak_mb(), e.live());
}

// ---- child: DRed on a dense CYCLIC graph at V nodes (on disk) ------------------
fn child_dred(v: usize) {
    gun::install(5120);
    sqlmem::reset_peak();
    let n = v as i64;
    let mut rng = Lcg(0xD5ED ^ v as u64);
    let mut edge = |r: &mut Lcg| {
        let u = (r.next() % n as u64) as i64;
        let mut w = (r.next() % n as u64) as i64;
        if w == u {
            w = (w + 1) % n;
        }
        (u, w)
    };
    let mut live: HashSet<(i64, i64)> = HashSet::new();
    while live.len() < v * 3 {
        live.insert(edge(&mut rng));
    }
    let init: Vec<(i64, i64)> = live.iter().copied().collect();
    let mut d = SqliteReachDRed::new();
    d.setup(&[0], &init);

    let mut ms = 0.0;
    let rounds = 3;
    for _ in 0..rounds {
        let dels: Vec<(i64, i64)> = live.iter().copied().take(100).collect();
        let mut adds = Vec::new();
        while adds.len() < 100 {
            let e = edge(&mut rng);
            if !live.contains(&e) {
                adds.push(e);
            }
        }
        let t = std::time::Instant::now();
        d.del_batch(&dels);
        for e in &dels {
            live.remove(e);
        }
        d.add_batch(&adds);
        for &e in &adds {
            live.insert(e);
        }
        ms += t.elapsed().as_secs_f64() * 1000.0;
    }
    println!(
        "OK v={v} ms_round={:.0} sqliteMB={:.0} rssMB={:.0}",
        ms / rounds as f64,
        sqlmem::peak_mb(),
        gun::peak_rss_mb()
    );
}

fn run_child(exe: &std::path::Path, kind: &str, v: usize) -> (bool, String) {
    let out = std::process::Command::new(exe).arg(kind).arg(v.to_string()).output().unwrap();
    let ok = out.status.success();
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().find(|l| l.starts_with("OK")).unwrap_or("").to_string();
    (ok, if line.is_empty() { format!("exit={:?}", out.status.code()) } else { line })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        let v: usize = args[2].parse().unwrap();
        match args[1].as_str() {
            #[cfg(feature = "with-dd")]
            "dd" => return child_dd(v),
            "dred" => return child_dred(v),
            _ => {}
        }
        return;
    }

    let exe = std::env::current_exe().unwrap();
    println!("BREAKING POINT — ramp to failure (gun cap 5 GB)\n");

    println!("== dd all-pairs reach (RESIDENT) — expect RAM death (gun SIGABRT) ==");
    for &v in &[500usize, 1000, 2000, 3000, 4000, 6000, 8000] {
        let (ok, line) = run_child(&exe, "dd", v);
        if ok {
            println!("  {line}");
        } else {
            println!("  v={v}  *** BROKE — gun fired (resident RAM > 5 GB), {line} ***");
            break;
        }
    }

    println!("\n== DRed dense cyclic (ON DISK) — expect TIME blowup, RAM bounded ==");
    for &v in &[50_000usize, 200_000, 800_000, 2_000_000] {
        let (ok, line) = run_child(&exe, "dred", v);
        if ok {
            println!("  {line}");
        } else {
            println!("  v={v}  *** BROKE — {line} ***");
            break;
        }
    }
    println!("\n  read it: dd's number is allocMB climbing to the gun; DRed's is ms_round climbing while sqliteMB stays bounded.");
}
