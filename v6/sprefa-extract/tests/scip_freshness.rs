//! FAIL-FIRST RECEIPTS for index freshness and for the indexer budget.
//!
//! Before `index_path_for_set` existed, `index_path` returned whichever index
//! had the newest mtime and nothing compared it to the file set the caller was
//! asking about: `stale_set_rebuilds_and_the_original_set_still_hits` asserted
//! `None` against a function that could only ever answer `Some`, so it failed on
//! its second assertion. `a_nested_checkout_is_never_staged` failed too: the
//! staging walk skipped only build outputs, so a lane worktree under
//! `.boop-worktrees/**` was copied whole (2320 `.rs` on hafley-rs, 129 after).
//!
//! `slow_indexer_is_a_named_skip` is the budget's receipt: a fake `scip-go` that
//! sleeps far past a one-second budget must come back as `SkipReason::TimedOut`,
//! never as a wait.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sprefa_extract::{
    ensure_index_for_set, index_path, index_path_for_set, record_index_set, IndexBudget, IndexSet,
    SkipReason,
};

// PATH and SPREFA_SCIP_INDEX are process globals, so the tests that move them
// hold this lock; every other test here touches only its own temp dirs.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn temp_root(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let root =
        std::env::temp_dir().join(format!("scip-fresh-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

fn place_fake_index(cache: &Path) -> PathBuf {
    std::fs::create_dir_all(cache).expect("cache dir");
    let index = cache.join("index.scip");
    std::fs::write(&index, b"not a real index, only a path to find").expect("write index");
    index
}

fn set_of(pairs: &[(&str, &str)]) -> IndexSet {
    IndexSet::new(
        pairs
            .iter()
            .map(|(path, digest)| (path.to_string(), digest.to_string())),
    )
}

#[test]
fn stale_set_rebuilds_and_the_original_set_still_hits() {
    let root = temp_root("stale");
    let cache = root.join(".dl").join(".state");
    let index = place_fake_index(&cache);

    let built_from = set_of(&[("a.rs", "digest-a"), ("b.rs", "digest-b")]);
    record_index_set(&index, &built_from);

    assert_eq!(
        index_path_for_set(&root, &cache, Some(built_from.digest())),
        Some(index.clone()),
        "the set the index was built from is a hit"
    );

    let one_file_changed = set_of(&[("a.rs", "digest-a"), ("b.rs", "digest-b-MOVED")]);
    assert_ne!(built_from.digest(), one_file_changed.digest());
    assert_eq!(
        index_path_for_set(&root, &cache, Some(one_file_changed.digest())),
        None,
        "one changed digest is a different set, so the index is not current"
    );

    assert_eq!(
        index_path_for_set(&root, &cache, Some(built_from.digest())),
        Some(index.clone()),
        "asking the original set again hits without rebuilding"
    );
    assert_eq!(
        index_path(&root, &cache),
        Some(index),
        "the v5 form with no set asked is unchanged"
    );
}

#[test]
fn set_digest_is_order_insensitive_and_content_sensitive() {
    let one_order = set_of(&[("a.rs", "one"), ("b.rs", "two")]);
    let other_order = set_of(&[("b.rs", "two"), ("a.rs", "one")]);
    assert_eq!(one_order.digest(), other_order.digest());

    let extra_file = set_of(&[("a.rs", "one"), ("b.rs", "two"), ("c.rs", "three")]);
    assert_ne!(one_order.digest(), extra_file.digest());
    assert_eq!(extra_file.len(), 3);
}

#[test]
fn a_stale_index_makes_ensure_rebuild_rather_than_reuse() {
    let root = temp_root("ensure");
    let cache = root.join(".dl").join(".state");
    let index = place_fake_index(&cache);
    let built_from = set_of(&[("a.rs", "digest-a")]);
    record_index_set(&index, &built_from);

    let reused = ensure_index_for_set(&root, &cache, IndexBudget { secs: 1 }, Some(&built_from));
    assert!(reused.reused, "the matching set reuses");

    let moved = set_of(&[("a.rs", "digest-a-MOVED")]);
    let rebuilt = ensure_index_for_set(&root, &cache, IndexBudget { secs: 1 }, Some(&moved));
    assert!(!rebuilt.reused, "a different set never reuses");
    // The temp root carries no marker file, so the rebuild it attempted names
    // the one reason it could not run rather than answering out of the stale index.
    assert_eq!(
        rebuilt.skips.first().map(|skip| skip.reason.slug()),
        Some("no_markers")
    );
    assert!(rebuilt.index.is_none());
}

#[test]
fn explicit_index_override_ignores_the_set() {
    let _held = ENVIRONMENT.lock().expect("environment lock");
    let root = temp_root("override");
    let cache = root.join(".dl").join(".state");
    let explicit = root.join("elsewhere.scip");
    std::fs::write(&explicit, b"explicit").expect("write explicit index");

    let previous = std::env::var_os("SPREFA_SCIP_INDEX");
    std::env::set_var("SPREFA_SCIP_INDEX", &explicit);
    let never_built_from = set_of(&[("nothing.rs", "nothing")]);
    let found = index_path_for_set(&root, &cache, Some(never_built_from.digest()));
    match previous {
        Some(value) => std::env::set_var("SPREFA_SCIP_INDEX", value),
        None => std::env::remove_var("SPREFA_SCIP_INDEX"),
    }
    assert_eq!(found, Some(explicit));
}

#[test]
fn slow_indexer_is_a_named_skip_not_a_wait() {
    let _held = ENVIRONMENT.lock().expect("environment lock");
    let root = temp_root("budget");
    std::fs::write(root.join("go.mod"), b"module fake\n").expect("go.mod marker");
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("bin dir");
    let fake = bin_dir.join("scip-go");
    std::fs::write(&fake, "#!/bin/sh\nsleep 120\n").expect("fake indexer");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake indexer executable");
    }

    let previous_path = std::env::var_os("PATH");
    let mut entries = vec![bin_dir.clone()];
    if let Some(path) = previous_path.as_ref() {
        entries.extend(std::env::split_paths(path));
    }
    let joined = std::env::join_paths(entries).expect("join PATH");
    std::env::set_var("PATH", &joined);

    let started = std::time::Instant::now();
    let report = ensure_index_for_set(
        &root,
        &root.join(".dl").join(".state"),
        IndexBudget { secs: 1 },
        None,
    );
    let waited = started.elapsed();

    match previous_path {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }

    assert!(report.index.is_none());
    let timed_out = report
        .skips
        .iter()
        .any(|skip| matches!(skip.reason, SkipReason::TimedOut { secs: 1 }));
    assert!(
        timed_out,
        "expected a named TimedOut skip, got {:?}",
        report.skips
    );
    assert!(
        waited.as_secs() < 30,
        "the budget bounded the wait: {waited:?}"
    );
}

#[test]
fn a_nested_checkout_is_never_staged() {
    let root = temp_root("staging");
    let own = root.join("crates").join("mine").join("src");
    std::fs::create_dir_all(&own).expect("own sources");
    std::fs::write(own.join("lib.rs"), b"pub fn mine() {}\n").expect("own file");
    std::fs::write(root.join("Cargo.toml"), b"[package]\nname='mine'\n").expect("manifest");

    // A git worktree carries a `.git` FILE and a submodule a `.git` dir; both
    // are a different checkout and neither is part of this workspace.
    let worktree = root.join(".boop-worktrees").join("fix").join("lane");
    std::fs::create_dir_all(worktree.join("src")).expect("nested checkout");
    std::fs::write(worktree.join(".git"), b"gitdir: /elsewhere\n").expect("worktree marker");
    std::fs::write(worktree.join("src").join("lib.rs"), b"pub fn theirs() {}\n")
        .expect("nested file");
    let submodule = root.join("vendor").join("dep");
    std::fs::create_dir_all(submodule.join(".git")).expect("submodule marker");
    std::fs::write(submodule.join("lib.rs"), b"pub fn vendored() {}\n").expect("vendored file");

    let stage = temp_root("staging-out");
    sprefa_extract::copy_sources(&root, &stage, &["rs"], &["Cargo.toml"]).expect("stage");

    assert!(stage.join("crates/mine/src/lib.rs").is_file());
    assert!(stage.join("Cargo.toml").is_file());
    assert!(
        !stage.join(".boop-worktrees/fix/lane/src/lib.rs").exists(),
        "a nested worktree is a different checkout"
    );
    assert!(
        !stage.join("vendor/dep/lib.rs").exists(),
        "a submodule is a different checkout"
    );
}

#[test]
fn a_persistent_stage_drops_a_source_the_corpus_deleted() {
    let root = temp_root("prune");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("src/a.rs"), b"pub fn a() {}\n").expect("a");
    std::fs::write(root.join("src/b.rs"), b"pub fn b() {}\n").expect("b");
    let stage = temp_root("prune-out");
    sprefa_extract::copy_sources(&root, &stage, &["rs"], &[]).expect("first stage");
    assert!(stage.join("src/b.rs").is_file());

    std::fs::remove_file(root.join("src/b.rs")).expect("delete b");
    std::fs::create_dir_all(stage.join("target/debug")).expect("warm target");
    std::fs::write(stage.join("target/debug/marker.rs"), b"kept\n").expect("target marker");
    sprefa_extract::copy_sources(&root, &stage, &["rs"], &[]).expect("second stage");

    assert!(stage.join("src/a.rs").is_file());
    assert!(
        !stage.join("src/b.rs").exists(),
        "a deleted source is pruned"
    );
    assert!(
        stage.join("target/debug/marker.rs").is_file(),
        "the warm target is what the persistent stage exists to keep"
    );
}

#[test]
fn the_informed_default_adopts_a_fresh_index_and_a_stale_one_stays_plain() {
    let root = temp_root("informed-default");
    let cache = root.join(".dl").join(".state");
    let index = place_fake_index(&cache);

    let built_from = set_of(&[("a.ts", "digest-a"), ("b.ts", "digest-b")]);
    record_index_set(&index, &built_from);

    assert_eq!(
        sprefa_extract::fresh_index_for_set(&root, built_from.digest()),
        Some(index.clone()),
        "the plain resolve leg adopts the index whose set matches the file set"
    );

    let other_set = set_of(&[("a.ts", "digest-a"), ("b.ts", "digest-b-MOVED")]);
    assert_eq!(
        sprefa_extract::fresh_index_for_set(&root, other_set.digest()),
        None,
        "a set the index was not built from stays on the plain name-match leg"
    );
    assert_eq!(
        sprefa_extract::fresh_index_for_set(&root, "no-sidecar-anywhere"),
        None,
        "an index without a recorded set is never adopted"
    );
}
