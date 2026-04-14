//! Integration tests for BufferOverlay reader.

use std::sync::Arc;

use bytes::Bytes;
use futures::executor::block_on;
use futures_util::StreamExt;

use v2::readers::{BufferKey, BufferOverlay, MemReader};
use v2::_0_types::FilePath;
use v2::_2_config::{Config, RuntimeConfig};
use v2::Reader;

fn cfg() -> Arc<Config> {
    Arc::new(Config {
        repos: vec![],
        revs: vec![],
        fs_exclude: vec![],
        sprf_files: vec![],
        shell_allow: vec![],
        runtime: RuntimeConfig {
            worker_threads: 1,
            buffer_size: 256,
            flush_interval_ms: 100,
            collect_witnesses: false,
        },
        content_hash: 0,
    })
}

fn fp(s: &str) -> FilePath {
    FilePath(Arc::from(std::path::Path::new(s)))
}

fn key(repo: &str, rev: &str, path: &str) -> BufferKey {
    BufferKey {
        repo: Arc::from(repo),
        rev:  Arc::from(rev),
        path: Arc::from(path),
    }
}

// 1. A buffered path shadows the inner reader's bytes.
#[test]
fn buffer_overlay_serves_override() {
    let inner = Arc::new(
        MemReader::new(cfg())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["a.ts"])
            .with_content("r", "main", "a.ts", b"inner"),
    );
    let overlay = BufferOverlay::new(inner);
    overlay.set_buffer(
        key("r", "main", "a.ts"),
        Arc::new(Bytes::from_static(b"buffer")),
    );

    let data = block_on(overlay.bytes("r", "main", &fp("a.ts")).next()).unwrap();
    assert_eq!(&*data, b"buffer");
}

// 2. Without a buffer entry, bytes delegate to inner.
#[test]
fn buffer_overlay_delegates_when_no_buffer() {
    let inner = Arc::new(
        MemReader::new(cfg())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["b.ts"])
            .with_content("r", "main", "b.ts", b"from_inner"),
    );
    let overlay = BufferOverlay::new(inner);

    let data = block_on(overlay.bytes("r", "main", &fp("b.ts")).next()).unwrap();
    assert_eq!(&*data, b"from_inner");
}

// 3. files() returns inner list UNION buffer-only paths that match the glob.
#[test]
fn buffer_overlay_unions_files_list() {
    let inner = Arc::new(
        MemReader::new(cfg())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["existing.ts"]),
    );
    let overlay = BufferOverlay::new(inner);
    overlay.set_buffer(
        key("r", "main", "unsaved.ts"),
        Arc::new(Bytes::from_static(b"draft")),
    );

    let mut files = block_on(overlay.files("r", "main", "**/*.ts").next()).unwrap();
    files.sort_by(|a, b| a.0.to_string_lossy().cmp(&b.0.to_string_lossy()));

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].0.to_string_lossy(), "existing.ts");
    assert_eq!(files[1].0.to_string_lossy(), "unsaved.ts");
}

// 4. clear_buffer restores delegation to inner.
#[test]
fn buffer_overlay_clear_restores_inner() {
    let inner = Arc::new(
        MemReader::new(cfg())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["c.ts"])
            .with_content("r", "main", "c.ts", b"original"),
    );
    let overlay = BufferOverlay::new(inner);
    let k = key("r", "main", "c.ts");
    overlay.set_buffer(k.clone(), Arc::new(Bytes::from_static(b"modified")));

    // Confirm override is active.
    let before = block_on(overlay.bytes("r", "main", &fp("c.ts")).next()).unwrap();
    assert_eq!(&*before, b"modified");

    overlay.clear_buffer(&k);

    let after = block_on(overlay.bytes("r", "main", &fp("c.ts")).next()).unwrap();
    assert_eq!(&*after, b"original");
}

// 5. Buffer keys are scoped to (repo, rev); same path in a different rev is independent.
#[test]
fn buffer_overlay_respects_repo_rev_keying() {
    let inner = Arc::new(
        MemReader::new(cfg())
            .with_repo("r", &["main", "feat"])
            .with_files("r", "main", &["d.ts"])
            .with_files("r", "feat", &["d.ts"])
            .with_content("r", "main", "d.ts", b"on_main")
            .with_content("r", "feat", "d.ts", b"on_feat"),
    );
    let overlay = BufferOverlay::new(inner);
    // Only shadow main.
    overlay.set_buffer(
        key("r", "main", "d.ts"),
        Arc::new(Bytes::from_static(b"main_buffer")),
    );

    let main_data = block_on(overlay.bytes("r", "main", &fp("d.ts")).next()).unwrap();
    let feat_data = block_on(overlay.bytes("r", "feat", &fp("d.ts")).next()).unwrap();

    assert_eq!(&*main_data, b"main_buffer");
    assert_eq!(&*feat_data, b"on_feat");
}
