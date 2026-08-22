//! `dl6 run` end to end: a program folds once and prints its ordered `?` rows,
//! `--fail-on` decides the exit code, the compile cache spawns swipl once for
//! two runs of one source, the one db leaves a file a cold `sqlite3` reads, and one
//! touched file under a program whose OWN `bind watch` decl makes it resident
//! produces one extra tick from the same verb.
//!
//! Every test is held to the 10-second law; the resident test caps its own wait.
//!
//! @comment-ok: the sabotage receipts this file's assertions were red against.
//! Sabotage receipts:
//!   - dropping the `if cached.is_file()` early-out in dl6.rs::compiled_program
//!     reds `the_second_run_of_one_source_spawns_no_swipl` with 2 spawns;
//!   - returning `None` for `fail_on_rows` in run.rs::run_once reds
//!     `fail_on_exits_one_when_the_query_answers_rows` with exit 0;
//!   - dropping the `CREATE VIEW` pass in run.rs::install_db_views reds
//!     `a_db_run_leaves_a_cold_reader_its_views` with `no such table: v_score`;
//!   - dropping `refresh_arrivals` from the watch loop's `moved` arm reds
//!     `one_touched_file_produces_exactly_one_extra_tick` with 1 tick.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const TEN_SECOND_LAW: Duration = Duration::from_secs(10);

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    engine_dir().join("tests/fixtures").join(name)
}

struct Scratch {
    home: tempfile::TempDir,
}

impl Scratch {
    /// A private cache root per test, so one test's compile never answers
    /// another's and the spawn count is this test's own.
    fn new() -> Scratch {
        Scratch {
            home: tempfile::tempdir().expect("temporary cache root"),
        }
    }

    fn dl6(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dl6"));
        command.env("XDG_CACHE_HOME", self.home.path());
        // ONE SERVER, ONE DB: there is no per-program flag, so a test moves the
        // one db with `DL6_DB` rather than naming a second file.
        command.env("DL6_DB", self.db());
        command
    }

    fn db(&self) -> std::path::PathBuf {
        self.home.path().join("dl6.db")
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn finish(command: &mut Command, what: &str) -> Run {
    let started = Instant::now();
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    let elapsed = started.elapsed();
    println!("dl6_run: {what} {:.2}s", elapsed.as_secs_f64());
    assert!(
        elapsed < TEN_SECOND_LAW,
        "{what} took {elapsed:?}, over the 10-second law"
    );
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn ok(run: &Run, what: &str) -> String {
    assert_eq!(
        run.code,
        Some(0),
        "{what} exited {:?}: {}{}",
        run.code,
        run.stdout,
        run.stderr
    );
    run.stdout.clone()
}

// Two rows per rel, one of them out of the asked-for order, so a run that lost
// the `?` order tail prints them the other way round.
fn score_seeds(command: &mut Command) {
    command
        .args(["--arrive", "score=1,10"])
        .args(["--arrive", "score=2,30"])
        .args(["--arrive", "module_defs=b.ts,4"])
        .args(["--arrive", "module_defs=a.ts,9"]);
}

#[test]
fn a_one_shot_run_prints_the_ordered_final_tsv() {
    let scratch = Scratch::new();
    let mut command = scratch.dl6();
    command
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .args(["--final-tsv", "--final-only"])
        .args(["--final-rels", "score,module_defs"]);
    score_seeds(&mut command);
    let stdout = ok(&finish(&mut command, "one-shot run"), "one-shot run");
    assert_eq!(
        stdout,
        "score\t2\t30\nscore\t1\t10\nmodule_defs\ta.ts\t9\nmodule_defs\tb.ts\t4\n",
        "the `?` order tail decides the row order, and --final-rels the rel order"
    );
}

#[test]
fn fail_on_exits_one_when_the_query_answers_rows() {
    let scratch = Scratch::new();
    let mut answering = scratch.dl6();
    answering
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .args(["--final-only", "--fail-on", "score"]);
    score_seeds(&mut answering);
    let answered = finish(&mut answering, "fail-on with rows");
    assert_eq!(
        answered.code,
        Some(1),
        "a `?` query with rows exits 1: {}{}",
        answered.stdout,
        answered.stderr
    );

    // A SECOND scratch, so a second db: the one db keeps a table whose shape
    // did not move, and re-using this one would answer the seeded rows again.
    let unseeded = Scratch::new();
    let mut empty = unseeded.dl6();
    empty
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .args(["--final-only", "--fail-on", "score"]);
    let quiet = finish(&mut empty, "fail-on with no rows");
    assert_eq!(
        quiet.code,
        Some(0),
        "a `?` query with no rows exits 0: {}{}",
        quiet.stdout,
        quiet.stderr
    );
}

#[test]
fn an_unknown_fail_on_query_is_named_before_the_fold() {
    let scratch = Scratch::new();
    let mut command = scratch.dl6();
    command
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .args(["--final-only", "--fail-on", "not_a_query"]);
    let run = finish(&mut command, "fail-on an unknown query");
    assert_ne!(run.code, Some(0), "an unknown query is a stop");
    assert!(
        run.stderr.contains("which the program does not answer")
            && run.stderr.contains("score"),
        "the stop names the query and lists the declared ones: {}",
        run.stderr
    );
}

/// A `swipl` shim first on PATH that appends one line per spawn, then execs the
/// real one. The count is the receipt the cache claim rests on.
struct SwiplCounter {
    directory: tempfile::TempDir,
    log: PathBuf,
}

impl SwiplCounter {
    fn install() -> SwiplCounter {
        let directory = tempfile::tempdir().expect("temporary PATH directory");
        let log = directory.path().join("spawns.txt");
        let real = String::from_utf8(
            Command::new("sh")
                .args(["-c", "command -v swipl"])
                .output()
                .expect("look up swipl")
                .stdout,
        )
        .expect("utf-8 swipl path")
        .trim()
        .to_string();
        assert!(!real.is_empty(), "swipl is not on PATH");
        let shim = directory.path().join("swipl");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nprintf 'spawn\\n' >> {}\nexec {} \"$@\"\n",
                log.display(),
                real
            ),
        )
        .expect("write the swipl shim");
        let mut mode = std::fs::metadata(&shim).expect("stat the shim").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o755);
        std::fs::set_permissions(&shim, mode).expect("make the shim executable");
        SwiplCounter { directory, log }
    }

    fn path_with_shim(&self) -> String {
        let existing = std::env::var("PATH").unwrap_or_default();
        format!("{}:{existing}", self.directory.path().display())
    }

    fn spawns(&self) -> usize {
        std::fs::read_to_string(&self.log)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}

#[test]
fn the_second_run_of_one_source_spawns_no_swipl() {
    let scratch = Scratch::new();
    let counter = SwiplCounter::install();
    let mut walls = Vec::new();
    for pass in 0..2 {
        let mut command = scratch.dl6();
        command
            .env("PATH", counter.path_with_shim())
            .arg("run")
            .arg(fixture("query_order_tail.dl6"))
            .args(["--final-tsv", "--final-only", "--final-rels", "score"]);
        score_seeds(&mut command);
        let started = Instant::now();
        let run = finish(&mut command, &format!("cache pass {pass}"));
        walls.push(started.elapsed());
        let stdout = ok(&run, &format!("cache pass {pass}"));
        assert_eq!(
            stdout, "score\t2\t30\nscore\t1\t10\n",
            "both passes fold the same program"
        );
        assert!(
            run.stderr.contains(if pass == 0 {
                "compile swipl"
            } else {
                "compile cached"
            }),
            "pass {pass} reports which door it took: {}",
            run.stderr
        );
    }
    assert_eq!(
        counter.spawns(),
        1,
        "two runs of one source compile once; walls {walls:?}"
    );
}

#[test]
fn a_db_run_leaves_a_cold_reader_its_views() {
    let scratch = Scratch::new();
    let db = scratch.db();
    let mut command = scratch.dl6();
    command
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .arg("--final-only");
    score_seeds(&mut command);
    ok(&finish(&mut command, "run"), "run");
    assert!(db.is_file(), "{} is a file", db.display());

    // The process is gone: a TEMP view would have gone with it.
    let rows = sqlite(&db, "SELECT player, points FROM v_score");
    assert_eq!(
        rows, "2|30\n1|10\n",
        "v_score carries the `?` order tail and outlives the process"
    );
    let meta = sqlite(&db, "SELECT program, tick FROM __meta");
    assert!(
        meta.starts_with("query_order_tail|"),
        "__meta names the program that wrote the file: {meta}"
    );
    let decoded = sqlite(
        &db,
        "SELECT count(*) FROM sqlite_master WHERE type='view' AND name LIKE '__txt_%'",
    );
    assert_ne!(
        decoded.trim(),
        "0",
        "the decoded-text views are persistent in the one db"
    );
}

fn sqlite(db: &Path, sql: &str) -> String {
    let output = Command::new("sqlite3")
        .arg(db)
        .arg(sql)
        .output()
        .expect("run sqlite3");
    assert!(
        output.status.success(),
        "sqlite3 {sql}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 sqlite3 output")
}

fn git(worktree: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["-c", "user.email=dl6@test", "-c", "user.name=dl6"])
        .args(arguments)
        .status()
        .expect("run git");
    assert!(status.success(), "git {arguments:?} in {}", worktree.display());
}

#[test]
fn one_touched_file_produces_exactly_one_extra_tick() {
    let worktree = tempfile::tempdir().expect("temporary worktree");
    let root = worktree.path();
    git(root, &["init", "-q", "."]);
    std::fs::create_dir_all(root.join("src")).expect("create src");
    std::fs::write(root.join("src/one.ts"), "export const one = 1;\n").expect("write one.ts");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "seed"]);

    let scratch = Scratch::new();
    let mut child = scratch
        .dl6()
        .env("RUST_LOG", "sprefa_engine_rs=info")
        .arg("run")
        .arg(engine_dir().join("../dl/fixtures/served-watch-rail.dl6"))
        .arg("--root")
        .arg(root)
        .args(["--final-tsv", "--final-only", "--final-rels", "seen"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the resident file watch");

    // The first fold has to land before the touch, or the enumeration already
    // carries the new bytes and there is no second tick to count.
    let started = Instant::now();
    std::thread::sleep(Duration::from_millis(1500));
    std::fs::write(root.join("src/one.ts"), "export const one = 11;\n").expect("touch one.ts");
    std::thread::sleep(Duration::from_millis(3500));
    let _ = child.kill();
    let output = child.wait_with_output().expect("collect the resident file watch");
    let elapsed = started.elapsed();
    println!("dl6_run: watch one touch {:.2}s", elapsed.as_secs_f64());
    assert!(
        elapsed < TEN_SECOND_LAW,
        "the watch test took {elapsed:?}, over its 10-second cap"
    );

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let ticks = stderr.matches("watch: tick").count();
    assert_eq!(
        ticks, 1,
        "one touched file is one tick past the first fold: {stderr}"
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "the first fold adds one row and the touch swaps it: {stdout}"
    );
    assert!(
        lines[0].starts_with("+seen\tsrc/one.ts\t"),
        "the first fold prints the row it holds: {stdout}"
    );
    assert!(
        lines[1].starts_with("-seen\tsrc/one.ts\t") && lines[2].starts_with("+seen\tsrc/one.ts\t"),
        "a content change is a del of the old digest and an add of the new: {stdout}"
    );
    assert_eq!(
        lines[1].trim_start_matches('-'),
        lines[0].trim_start_matches('+'),
        "the del retires exactly the row the first fold held"
    );
    assert_ne!(
        lines[2], lines[0],
        "the add carries the new digest, so the change is a real freshness delta"
    );
}

/// @comment-ok: TEST header carrying the fail-first receipts.
/// Sabotage receipts:
///   - returning `stat` rather than `subtract(&stat, &previous)` in cost.rs
///     reds the flat-memory assertion, because `wall_ms` and `calls` climb
///     without bound and the series stops describing one tick;
///   - dropping the `stays_resident` arm from dl6.rs::run makes this exit at
///     tick 0 and reds the tick-count assertion with 1 bucket.
#[test]
fn a_resident_run_measures_itself_and_a_storeless_program_stays_flat() {
    let scratch = Scratch::new();
    let mut child = scratch
        .dl6()
        .env("RUST_LOG", "sprefa_engine_rs=info")
        .arg("run")
        .arg(fixture("tick_cost_beat.dl6"))
        .args(["--final-tsv", "--final-only", "--final-rels", "tick_cost"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the resident run");

    let started = Instant::now();
    std::thread::sleep(Duration::from_millis(8_000));
    let _ = child.kill();
    let output = child.wait_with_output().expect("collect the resident run");
    let elapsed = started.elapsed();
    println!("dl6_run: resident self-measurement {:.2}s", elapsed.as_secs_f64());
    assert!(
        elapsed < TEN_SECOND_LAW,
        "the resident test took {elapsed:?}, over its 10-second cap"
    );

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let rows: Vec<Vec<&str>> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("+tick_cost\t"))
        .map(|line| line.split('\t').collect())
        .collect();
    assert!(
        rows.len() >= 4,
        "one tick a second for eight seconds is several cost rows, got {}: {stdout}{stderr}",
        rows.len()
    );

    let buckets: std::collections::BTreeSet<&str> =
        rows.iter().map(|row| row[0]).collect();
    assert!(
        buckets.len() >= 4,
        "the cadence turned {} times in eight seconds: {stderr}",
        buckets.len()
    );

    let resident: Vec<i64> = rows
        .iter()
        .filter(|row| row[1] == "wall")
        .filter_map(|row| row[3].parse::<i64>().ok())
        .collect();
    assert!(
        resident.len() >= 4,
        "every tick answers one `wall` row: {stdout}"
    );
    let low = *resident.iter().min().expect("a low reading");
    let high = *resident.iter().max().expect("a high reading");
    assert!(low > 0, "the resident set is a real reading, got {resident:?}");
    // SCOPE: this program stores five rows a tick, so a climb here is the LOOP
    // leaking, never stored history. A program with a real `keep(all)` log grows
    // by design and is not pinned by this number (v6/dl/prwatch measured
    // 46 MB to 100 MB over six ticks; PR #407 body carries the series).
    assert!(
        high * 4 <= low * 5,
        "rss climbed from {low} kB to {high} kB across {} ticks: {resident:?}",
        buckets.len()
    );
}

/// TEST: ONE SERVER, ONE DB (CLAUDE.md 2026-08-21). Two programs folded into
/// one file keep their own tables, and the second run does not replace the file
/// under the first. Sabotage receipt: restoring `open_seam`'s `remove_file`
/// turns the `v_score` read after the second program red.
#[test]
fn two_programs_share_one_db_without_clobbering_each_other() {
    let scratch = Scratch::new();
    let db = scratch.db();

    let mut first = scratch.dl6();
    first
        .arg("run")
        .arg(fixture("query_order_tail.dl6"))
        .arg("--final-only");
    score_seeds(&mut first);
    ok(&finish(&mut first, "first program"), "first program");
    let scores = sqlite(&db, "SELECT player, points FROM v_score");
    assert_eq!(scores, "2|30\n1|10\n");

    let mut second = scratch.dl6();
    second
        .arg("run")
        .arg(fixture("one_shot_pair.dl6"))
        .arg("--final-only")
        .args(["--arrive", "note=heavy,4"])
        .args(["--arrive", "note=light,1"]);
    ok(&finish(&mut second, "second program"), "second program");

    // The first program's view and rows are still there.
    assert_eq!(
        sqlite(&db, "SELECT player, points FROM v_score"),
        "2|30\n1|10\n",
        "the second program must not replace the file under the first"
    );
    // Both programs have a __meta row, and __str was never dropped.
    let programs = sqlite(&db, "SELECT DISTINCT program FROM __meta ORDER BY program");
    assert!(
        programs.contains("query_order_tail") && programs.contains("one_shot_pair"),
        "__meta names both programs: {programs}"
    );
    let interned = sqlite(&db, "SELECT count(*) > 0 FROM __str");
    assert_eq!(interned.trim(), "1", "the shared intern table survives");
}

/// TEST: a re-run of ONE program replaces its own tables rather than failing on
/// `table already exists`, which is what the shared file made possible.
#[test]
fn re_running_one_program_replaces_only_its_own_tables() {
    let scratch = Scratch::new();
    let db = scratch.db();
    for pass in 1..=2 {
        let mut command = scratch.dl6();
        command
            .arg("run")
            .arg(fixture("query_order_tail.dl6"))
            .arg("--final-only");
        score_seeds(&mut command);
        ok(&finish(&mut command, "pass"), &format!("pass {pass}"));
    }
    assert_eq!(
        sqlite(&db, "SELECT player, points FROM v_score"),
        "2|30\n1|10\n",
        "the second pass re-created the view rather than doubling its rows"
    );
    assert_eq!(
        sqlite(&db, "SELECT count(*) FROM __meta WHERE program='query_order_tail'").trim(),
        "2",
        "__meta appends one row per run"
    );
}
