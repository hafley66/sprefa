//! FAIL-FIRST RECEIPT for `--indexer`.
//!
//! `detect` is any-of over marker files and returns EVERY roster row that
//! matches, and `ensure_index_*` runs all of them. A root carrying both
//! `go.mod` and `package.json` therefore starts scip-go AND scip-typescript.
//! On typescript-go that is what happened: scip-typescript spent the whole
//! 900s budget and the process group was killed, so the go tuning control
//! could not be measured at all.
//!
//! `polyglot_root_runs_both_indexers` is the defect, asserted as it stands.
//! `a_pick_runs_only_the_named_indexer` fails without `detect_picked`:
//! before it, no argument could reach `detect`, so scip-typescript ran on
//! every ask.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sprefa_extract::{detect, detect_picked, ensure_index_picked, pick_cache_dir, IndexBudget};

// PATH is a process global; every test here that moves it holds this lock.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn temp_root(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let root =
        std::env::temp_dir().join(format!("scip-pick-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// A root that matches the go row and the typescript row at once, which is
/// typescript-go's shape.
fn polyglot_root(name: &str) -> PathBuf {
    let root = temp_root(name);
    std::fs::write(root.join("go.mod"), b"module fake\n").expect("go.mod marker");
    std::fs::write(root.join("package.json"), b"{\"name\":\"fake\"}\n").expect("package.json");
    root
}

/// A fake indexer that sleeps past any budget, so a row that RAN is visible as
/// a `timed_out` skip and a row that never ran leaves no trace at all.
fn plant_slow_indexer(bin_dir: &Path, bin: &str) {
    std::fs::create_dir_all(bin_dir).expect("bin dir");
    let fake = bin_dir.join(bin);
    std::fs::write(&fake, "#!/bin/sh\nsleep 120\n").expect("fake indexer");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake indexer executable");
    }
}

fn with_path<T>(bin_dir: &Path, body: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("PATH");
    let mut entries = vec![bin_dir.to_path_buf()];
    if let Some(path) = previous.as_ref() {
        entries.extend(std::env::split_paths(path));
    }
    std::env::set_var("PATH", std::env::join_paths(entries).expect("join PATH"));
    let out = body();
    match previous {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }
    out
}

#[test]
fn polyglot_root_matches_two_roster_rows() {
    let root = polyglot_root("detect");
    let langs: Vec<&str> = detect(&root).iter().map(|indexer| indexer.lang).collect();
    assert!(
        langs.contains(&"go") && langs.contains(&"typescript"),
        "one root, both rows: {langs:?}"
    );
}

#[test]
fn a_pick_narrows_detect_to_one_row() {
    let root = polyglot_root("detect-pick");
    let langs: Vec<&str> = detect_picked(&root, Some("go"))
        .iter()
        .map(|indexer| indexer.lang)
        .collect();
    assert_eq!(langs, vec!["go"]);
}

#[test]
fn a_pick_ignores_markers_entirely() {
    let root = temp_root("no-marker");
    assert!(detect(&root).is_empty());
    let langs: Vec<&str> = detect_picked(&root, Some("rust"))
        .iter()
        .map(|indexer| indexer.lang)
        .collect();
    assert_eq!(langs, vec!["rust"]);
}

#[test]
fn a_picked_index_never_lands_on_the_shared_path() {
    let cache = Path::new("/tmp/cache");
    assert_eq!(pick_cache_dir(cache, None), cache);
    assert_eq!(
        pick_cache_dir(cache, Some("go")),
        cache.join("indexer-go"),
        "a partial index keeps its own directory"
    );
    assert_eq!(
        pick_cache_dir(cache, Some("kotlin/java")),
        cache.join("indexer-kotlin-java"),
        "a roster lang holding a slash is still one directory"
    );
}

#[test]
fn polyglot_root_runs_both_indexers() {
    let _held = ENVIRONMENT.lock().expect("environment lock");
    let root = polyglot_root("both");
    let bin_dir = root.join("bin");
    plant_slow_indexer(&bin_dir, "scip-go");
    plant_slow_indexer(&bin_dir, "scip-typescript");

    let report = with_path(&bin_dir, || {
        ensure_index_picked(
            &root,
            &root.join(".dl").join(".state"),
            IndexBudget { secs: 1 },
            None,
            None,
        )
    });

    let touched: Vec<&str> = report.skips.iter().map(|skip| skip.lang).collect();
    assert!(
        touched.contains(&"go") && touched.contains(&"typescript"),
        "the defect: an unpicked ask spends a budget on every matched row: {touched:?}"
    );
}

#[test]
fn a_pick_runs_only_the_named_indexer() {
    let _held = ENVIRONMENT.lock().expect("environment lock");
    let root = polyglot_root("picked");
    let bin_dir = root.join("bin");
    plant_slow_indexer(&bin_dir, "scip-go");
    plant_slow_indexer(&bin_dir, "scip-typescript");

    let started = std::time::Instant::now();
    let report = with_path(&bin_dir, || {
        ensure_index_picked(
            &root,
            &root.join(".dl").join(".state"),
            IndexBudget { secs: 1 },
            None,
            Some("go"),
        )
    });
    let waited = started.elapsed();

    let touched: Vec<&str> = report
        .skips
        .iter()
        .map(|skip| skip.lang)
        .chain(report.ran.iter().map(|(lang, _)| *lang))
        .collect();
    assert_eq!(
        touched,
        vec!["go"],
        "scip-typescript must not be started at all"
    );
    assert!(
        waited.as_secs() < 10,
        "one budget, not two: {waited:?}"
    );
}
