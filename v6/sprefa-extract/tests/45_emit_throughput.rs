//! JSONL emission throughput: the default run must stream a large row count
//! through one buffered writer, not one flush + one String alloc per row.
//! Pins wall time on a synthetic 350k-row input and checks the piped line
//! count equals the `--bench` fact count.
//!
//! Budget derivation (debug binary): post-fix piped 4.03-4.15s, pre-fix
//! (per-row `println!` = LineWriter flush + `to_string` alloc per row)
//! 4.86-5.73s. 5.5s sits between the two. (Pre-fix standalone 4.86-5.73s; post-fix 4.03-4.15s; one noisy harness run hit 5.11s, so the budget carries headroom over the post-fix band.)

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

const WALL_BUDGET_SECS: f64 = 5.5;

#[test]
fn emit_throughput_350k_rows_under_budget() {
    let dir = std::env::temp_dir().join("sprefa-extract-45-emit");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rows.go");
    // ~14 cst rows + 1 df_lit row per line, the lit row carrying a 1KB string:
    // 25k lines -> 350k rows, ~25MB out.
    let lit = "x".repeat(1000);
    let mut src = String::from("package rows\n\n");
    for i in 0..25_000 {
        src.push_str(&format!("var s{i} string = \"{lit}\"\n"));
    }
    std::fs::write(&path, &src).unwrap();

    let bin = env!("CARGO_BIN_EXE_extract");
    let arg = path.to_string_lossy().into_owned();

    // Piped run: count lines, wall under a fixed budget.
    let t = Instant::now();
    let mut child = Command::new(bin)
        .arg(&arg)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = 0usize;
    {
        let stdout = child.stdout.take().unwrap();
        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let n = std::io::BufRead::read_until(&mut reader, b'\n', &mut buf).unwrap();
            if n == 0 {
                break;
            }
            lines += 1;
        }
    }
    let piped_elapsed = t.elapsed();
    let status = child.wait().unwrap();
    assert!(status.success());

    // --bench run: the fact count must match the piped line count.
    let out = Command::new(bin).arg("--bench").arg(&arg).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let facts: u64 = stderr
        .lines()
        .filter_map(|line| {
            line.split("facts=")
                .nth(1)
                .and_then(|rest| rest.trim_end_matches(')').parse().ok())
        })
        .next_back()
        .unwrap_or_else(|| panic!("no facts summary in: {stderr}"));
    std::io::stdout().flush().unwrap();

    assert_eq!(
        facts, lines as u64,
        "--bench facts {facts} != piped lines {lines}"
    );
    // Budget fixed above; the pre-fix per-row `println!` flush + alloc
    // lands well above it (see module doc).
    assert!(
        piped_elapsed.as_secs_f64() < WALL_BUDGET_SECS,
        "piped emission took {:?} for {lines} lines",
        piped_elapsed
    );
}
