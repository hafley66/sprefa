//! JSONL emission throughput: the default run must stream a large row count
//! through one buffered writer, not one flush + one String alloc per row.
//! Pins wall time on a synthetic 350k-row input, checks the piped line count
//! equals the `--bench` fact count, and counts the go door's source hashes.
//!
//! Budget derivation (debug binary): 5.5 s dates from #533, whose post-fix band
//! was 4.03-4.15 s against a pre-fix 4.86-5.73 s. That band does not reproduce:
//! `4a0b91362`, the commit that set it, measures 4.902 / 4.996 / 4.874 s on the
//! machine this comment was written on, so ~0.8 s of the gap below is the
//! machine, not the code (failure-modes 107).
//!
//! Re-measured at `5b9063bef` plus the one-hash fix, eleven runs: 5.403 to
//! 5.503 s, ten under the budget and one over, the one over at load 3.40. The
//! margin is 40 to 95 ms and the budget is NOT durably met. It sits no lower
//! because two full blake3 passes over the 25 MB input survive at 325 ms each
//! (`dispatch.rs` for the extract cache key, `go_parse_shared_keyed` for the
//! parse cache key); threading the dispatch id to the go door removes the
//! second and is the open arc in failure-modes 107. The load skip below is set
//! for a saturated machine and does NOT cover this band.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

const WALL_BUDGET_SECS: f64 = 5.5;

/// The binary under test is single-threaded, so the budget only reports the
/// code while a performance core is free for it. This machine reports 12
/// logical CPUs, 8 of them performance (`sysctl hw.perflevel0.logicalcpu`); at
/// a 1-minute average of 8 the runnable set already equals the performance-core
/// count and the wall number stops being about the code. The control runs that
/// bisected this budget sat at 4.1 to 5.5 and were faithful, so the threshold
/// sits above that band and cuts only a saturated machine.
const LOAD_SKIP_THRESHOLD: f64 = 8.0;

/// The `files` column of one `DL_TRACE_SUMMARY` row, which counts span entries
/// for that family. Columns are `lang family us files facts`.
fn summary_files(stderr: &str, family: &str) -> Option<u64> {
    stderr.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        (columns.next() == Some("go") && columns.next() == Some(family))
            .then(|| columns.nth(1)?.parse().ok())
            .flatten()
    })
}

/// The 1-minute load average, or None where the platform will not report it.
fn load_avg_1min() -> Option<f64> {
    let mut avg = [0f64; 3];
    // SAFETY: getloadavg fills at most `nelem` entries of the caller's array.
    let filled = unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) };
    (filled >= 1).then_some(avg[0])
}

#[test]
fn emit_throughput_350k_rows_under_budget() {
    // Read before the run, not after: the wall assert below is the only part
    // a saturated machine invalidates, and the COUNT assert is not.
    let load = load_avg_1min();

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

    // 25,563,904 B is over the default --max-bytes ceiling, and being over it
    // is the point: this test measures emission on a large row count, so it
    // opts out rather than measuring the skip. Budget and asserts unchanged.
    let no_ceiling = ["--max-bytes", "0"];

    // Piped run: count lines, wall under a fixed budget.
    let t = Instant::now();
    let mut child = Command::new(bin)
        .args(no_ceiling)
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

    // --bench run: the fact count must match the piped line count, and the
    // trace summary carries the COUNT receipt beside the wall one.
    let out = Command::new(bin)
        .arg("--bench")
        .args(no_ceiling)
        .arg(&arg)
        .env("DL_TRACE_SUMMARY", "1")
        .output()
        .unwrap();
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

    // COUNT receipt: hashing the source is linear in file size, so a second
    // full hash per file is a whole extra pass over 25 MB that no wall number
    // distinguishes from a busier machine. The go door takes exactly one.
    let hashes = summary_files(&stderr, "content-id")
        .unwrap_or_else(|| panic!("no `go content-id` summary row in: {stderr}"));
    assert_eq!(hashes, 1, "go hashed the source {hashes} times, want 1");
    // The measurement is the receipt, so a PASS reports its number too
    // (`-- --nocapture`), not only a FAIL.
    println!("piped emission {piped_elapsed:?} for {lines} lines, budget {WALL_BUDGET_SECS}s");

    // A wall budget measured on a saturated machine names the machine, never
    // the code (failure-modes 104, 107). Skipping BY NAME keeps that outcome
    // distinguishable from a pass.
    if let Some(load) = load {
        if load > LOAD_SKIP_THRESHOLD {
            println!("skipped: load {load:.2} > threshold {LOAD_SKIP_THRESHOLD:.1}");
            return;
        }
    }
    assert!(
        piped_elapsed.as_secs_f64() < WALL_BUDGET_SECS,
        "piped emission took {:?} for {lines} lines",
        piped_elapsed
    );
}
