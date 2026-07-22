use labkit::{gun, live_set_workload, reach_workload, CascadeZset, Experiment, RamReach, RamZset, SqliteReach, SqliteTemporal};
#[cfg(feature = "with-dd")]
use labkit::DdReach;
#[cfg(feature = "with-salsa")]
use labkit::SalsaRows;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[global_allocator]
static GLOBAL: gun::Gun = gun::Gun;

fn engine(name: &str) -> Box<dyn Experiment> {
    match name {
        "ram-zset" => Box::new(RamZset::default()),
        "cascade-zset" => Box::new(CascadeZset::default()),
        "ram-reach" => Box::new(RamReach::default()),
        "sqlite-reach" => Box::new(SqliteReach::default()),
        "sqlite-temporal" => Box::new(SqliteTemporal::default()),
        #[cfg(feature = "with-dd")]
        "dd-reach" => Box::new(DdReach::default()),
        #[cfg(feature = "with-salsa")]
        "salsa-rows" => Box::new(SalsaRows::default()),
        _ => panic!("unknown engine {name}"),
    }
}

fn child(name: &str, workload: &str, scale: usize) {
    let work = if workload == "reach" { reach_workload() } else { live_set_workload() };
    let stream = (work.make)(scale, 7, 1, 8);
    let mut experiment = engine(name);
    experiment.setup(scale);
    drop(stream.ticks.clone());
    gun::reset_peak(); labkit::sqlmem::reset_peak();
    let started = Instant::now();
    for (adds, removes) in &stream.ticks { experiment.tick(adds, removes); }
    println!("RESULT engine={name} workload={workload} scale={scale} correct={} digest={} oracle={} ms={:.3} rust_peak={:.3} sqlite_hw={:.3} rss={:.3}", experiment.digest() == stream.expected_digest, experiment.digest(), stream.expected_digest, started.elapsed().as_secs_f64()*1000.0, gun::peak_mb(), labkit::sqlmem::peak_mb(), gun::peak_rss_mb());
}

fn run_child(executable: &Path, engine: &str, workload: &str, scale: usize, cap: u64) -> String {
    String::from_utf8_lossy(&Command::new(executable).args(["--child", engine, workload, &scale.to_string()]).env("DL_MEMCAP_MB", cap.to_string()).output().unwrap().stdout).into_owned()
}

fn main() {
    let cap = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(2048);
    gun::install(cap);
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") { child(&args[2], &args[3], args[4].parse().unwrap()); return; }
    let exe = std::env::current_exe().unwrap();
    let mut report = String::from("# Unified G4v2 report\n\n| engine | workload | scale | result |\n|---|---|---:|---|\n");
    #[allow(unused_mut)]
    let mut live_engines = vec!["ram-zset", "cascade-zset", "sqlite-temporal"];
    #[cfg(feature = "with-salsa")]
    live_engines.push("salsa-rows");
    #[allow(unused_mut)]
    let mut reach_engines = vec!["ram-reach", "sqlite-reach"];
    #[cfg(feature = "with-dd")]
    reach_engines.push("dd-reach");
    for (workload, engines) in [("live", live_engines), ("reach", reach_engines)] {
        for scale in [100usize, 1000] { for name in engines.iter() { let line=run_child(&exe,name,workload,scale,cap); report.push_str(&format!("| {name} | {workload} | {scale} | {} |\n", line.lines().find(|l|l.starts_with("RESULT")).unwrap_or("child failed"))); } }
    }
    report.push_str("\n| engine | workload | scale | result |\n|---|---|---:|---|\n");
    report.push_str("| Reconciler | excluded | - | trait interface; no concrete experiment state |\n");
    report.push_str("| sqlite-reconciler | excluded | - | reconciliation-DAG digest, not live-set or all-pairs reach |\n");
    report.push_str("| salsa-reconciler | excluded | - | reconciliation-DAG digest, not live-set or all-pairs reach; requires with-salsa |\n");
    report.push_str("| dd-bfs | excluded | - | single-source reachable-node digest, not all-pairs reach; requires with-dd |\n");
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/UNIFIED-REPORT.md"), report).unwrap();
}
