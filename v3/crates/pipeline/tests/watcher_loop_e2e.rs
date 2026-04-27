//! sprefa-upb Phase F end-to-end:
//! Closes the entire loop in one test — fs watcher + invalidator + cache.
//!
//! Setup: pre-populate an OpCache entry indexed at (repo, "wt", path).
//! Spawn the fs watcher pointed at the dir, spawn the invalidator drain.
//! Write to the file. Wait for quiesce. Assert the cache shrank.
//!
//! macOS caveat: FSEvents reports parent-dir granularity, so the test
//! pre-populates BOTH the file path and the empty (root) path. Either
//! event shape evicts at least one entry; the assertion accepts both.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pipeline::cache_key::OpCache;
use pipeline::invalidate::{spawn_invalidator, ChangeSubject};
use pipeline::Cursor;

fn tempdir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("sprefa_watcher_loop_{name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&p).unwrap();
    std::fs::canonicalize(&p).unwrap_or(p)
}

fn cursor_at(repo: &str, rev: &str, path: &str) -> Cursor {
    let mut c = Cursor::default();
    c.repo = Arc::from(repo);
    c.rev = Arc::from(rev);
    c.fs = Some(Arc::from(Path::new(path)));
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_to_watched_file_evicts_cache_entry() {
    let dir = tempdir("write_evicts");
    let target = dir.join("a.rs");
    std::fs::write(&target, b"old").unwrap();

    let cache = Arc::new(OpCache::new(true));

    // Pre-populate: one entry rooted at the file (relative path, as
    // fs() sets it on real cursors), one at the empty (root) path
    // for macOS dir-granularity FSEvents, one unrelated (must survive).
    cache.insert([1u8; 32], Arc::from(vec![cursor_at("r", "wt", "a.rs")]));
    cache.insert([2u8; 32], Arc::from(vec![cursor_at("r", "wt", "")]));
    cache.insert([3u8; 32], Arc::from(vec![cursor_at("r", "wt", "unrelated.rs")]));
    let initial_len = cache.len();
    assert_eq!(initial_len, 3);

    let subject = ChangeSubject::new();
    let invalidator = spawn_invalidator(cache.clone(), &subject);
    let _watcher = pipeline::spawn_fs_watcher(
        Arc::from("r"),
        dir.clone(),
        subject.sender(),
        Duration::from_millis(50),
    )
    .expect("watcher spawned");

    // Let the watcher register before mutating the file.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // The actual edit. Write multiple times to make absolutely sure
    // the FS event fires through fsevents + the debouncer window.
    for chunk in [b"new1".as_slice(), b"new2".as_slice(), b"new3".as_slice()] {
        std::fs::write(&target, chunk).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Wait for quiesce + drain. Cache should drop at least one entry.
    let mut shrank = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if cache.len() < initial_len {
            shrank = true;
            break;
        }
    }

    assert!(
        shrank,
        "cache did not shrink within 2s of file write; initial={initial_len} now={}",
        cache.len()
    );
    // The unrelated entry under /tmp/unrelated.rs must still be there.
    assert!(
        cache.get(&[3u8; 32]).is_some(),
        "unrelated entry must survive file-scoped eviction"
    );

    // Drop the watcher first (it holds a Sender clone keeping the
    // broadcast channel open), then the subject. Only then will the
    // invalidator's recv() return RecvError::Closed and the task exit.
    drop(_watcher);
    drop(subject);
    invalidator.abort();
    let _ = std::fs::remove_dir_all(&dir);
}
