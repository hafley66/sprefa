//! `file_lines(repo, path, rev, line_count)` — the corpus-walk line count
//! (src/rels/filelines.rs) plus the dl-native `.dl/file-size.dl` rail it
//! feeds.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("file_lines_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Hermetic: an empty `$SPREFA_CONFIG` so a developer machine's real
/// `~/.config/sprefa/config.toml` never joins the run (would register extra
/// repos, noisy in stderr and irrelevant here).
fn empty_config(dir: &Path) -> PathBuf {
    let cfg = dir.join("empty.toml");
    fs::write(&cfg, "# no repos\n").unwrap();
    cfg
}

fn run(dir: &Path, prog_path: &str, extra: &[&str]) -> (i32, String, String) {
    let cfg = empty_config(dir);
    let out = Command::new(DL)
        .arg(dir.join(prog_path))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .args(["--no-daemon"])
        .env("SPREFA_CONFIG", &cfg)
        .env("DL_NO_DAEMON", "1")
        .env("DL_TRACE", "debug")
        .args(extra)
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

const SRC_QUERY: &str = r#"
rel src(path: file).
src(path) <- scan("WORK", "src/**/*.rs", path, rev).
? file_lines(repo, path, rev, line_count).
"#;

#[test]
fn file_lines_returns_real_counts() {
    let d = sandbox("counts");
    fs::create_dir_all(d.join("src")).unwrap();
    // 3 lines, trailing newline.
    fs::write(d.join("src/three.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    // 1 line, no trailing newline.
    fs::write(d.join("src/one.rs"), "fn only() {}").unwrap();
    fs::write(d.join("p.dl"), SRC_QUERY).unwrap();

    let (code, out, err) = run(&d, "p.dl", &[]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/three.rs\tWORK\t3"), "expected 3 lines for three.rs: {out}");
    assert!(out.contains("src/one.rs\tWORK\t1"), "expected 1 line for one.rs (no trailing newline): {out}");
}

#[test]
fn file_lines_second_tick_reuses_stored_count_on_unchanged_mtime() {
    let d = sandbox("fastpath");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    fs::write(d.join("p.dl"), SRC_QUERY).unwrap();

    let (code1, out1, _) = run(&d, "p.dl", &[]);
    assert_eq!(code1, 0);
    assert!(out1.contains("src/x.rs\tWORK\t2"), "{out1}");

    // Second run, nothing touched: the mtime+size fast path must reuse the
    // stored hash AND the stored line count without re-reading the file.
    // `[tick] files 0/N parsed` is the existing engine-wide fast-path signal
    // (same pattern extract_cache.rs/resolver tests rely on): 0 files parsed
    // means enumerate_with_hash never fell through to a fresh read+count.
    let (code2, out2, err2) = run(&d, "p.dl", &[]);
    assert_eq!(code2, 0);
    assert!(out2.contains("src/x.rs\tWORK\t2"), "count must survive the fast path: {out2}");
    assert!(err2.contains("files 0/") || err2.contains("0/1 parsed") || err2.contains("parsed"),
        "expected an unchanged-tick signal in stderr: {err2}");
    assert!(!err2.contains("files 1/1 parsed"),
        "an unchanged file must not be re-parsed on the second tick: {err2}");
}

#[test]
fn file_size_rail_flags_over_budget_file_and_reports_aggregate() {
    let d = sandbox("rail");
    fs::create_dir_all(d.join("src")).unwrap();
    // 501 lines: crosses the hard budget, not on the grandfather list.
    let big = "fn marker() {}\n".repeat(501);
    fs::write(d.join("src/big.rs"), big).unwrap();
    fs::write(d.join("src/small.rs"), "fn tiny() {}\n").unwrap();

    let repo_root = std::env::current_dir().unwrap();
    // Walk up from the test binary's cwd to find the repo's `.dl/file-size.dl`
    // (integration tests run with cwd = the crate root already).
    let rail = repo_root.join(".dl/file-size.dl");
    assert!(rail.exists(), "expected .dl/file-size.dl at {}", rail.display());
    let rail_contents = fs::read_to_string(&rail).unwrap();
    fs::write(d.join("file-size.dl"), rail_contents).unwrap();

    let (code, out, err) = run(&d, "file-size.dl", &[]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("file-over-budget") && out.contains("src/big.rs"),
        "expected an over-budget finding for src/big.rs: {out}");
    assert!(!out.contains("src/big.rs\t1\twarning\tfile-over-budget-grandfathered"),
        "src/big.rs is not on the grandfather list, must NOT be silenced: {out}");
    assert!(out.contains("file-size-debt") && out.contains("unacceptable files"),
        "expected the aggregate debt count row: {out}");
    assert!(!out.contains("src/small.rs"), "a small file must not be flagged: {out}");
}

#[test]
fn file_size_rail_check_mode_exits_zero_advisory_only() {
    let d = sandbox("rail_check");
    fs::create_dir_all(d.join("src")).unwrap();
    let big = "fn marker() {}\n".repeat(600);
    fs::write(d.join("src/big.rs"), big).unwrap();

    let repo_root = std::env::current_dir().unwrap();
    let rail = repo_root.join(".dl/file-size.dl");
    let rail_contents = fs::read_to_string(&rail).unwrap();
    fs::write(d.join("file-size.dl"), rail_contents).unwrap();

    let (code, _, err) = run(&d, "file-size.dl", &["--check"]);
    assert_eq!(code, 0, "the .dl rail is advisory-only (warning severity); \
        the bash script scripts/filesize-rail.sh is the exit-2 enforcement prong: {err}");
}
