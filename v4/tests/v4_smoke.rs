//! End-to-end v2-pipeline smoke. Builds Fs > Read > AstNm > Count over
//! a tmpdir of .rs files containing a known pattern. One assertion:
//! the count matches what we planted.
//!
//! Substrate purification (2026-05-07): matchers consume cursor.value
//! bytes. `read` is the gate that materializes them; the explicit
//! `Read` step replaces the implicit FS-load that used to live inside
//! AstNmComponent.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ast_grep_language::SupportLang;
use effect_runtime::v2::{expand, Component, ExpandOpts, MemQueue, PipeInstance, QueueBackend};
use v4::v2_ops::{AstNmComponent, CountComponent, FsComponent, ReadComponent};

#[test]
fn fs_astnm_count_v2_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    // Three .rs files; planted pattern is `fn $NAME` (ast-grep meta).
    // a.rs: 2 fns, b.rs: 1 fn, c.rs: 0 fns. Total = 3 matches.
    std::fs::write(dir.path().join("a.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "fn gamma() {}\n").unwrap();
    std::fs::write(
        dir.path().join("c.rs"),
        "// no fn here, just a comment\nstruct S;\n",
    )
    .unwrap();

    let counter = Arc::new(AtomicU64::new(0));
    let steps: Vec<Arc<dyn Component<Next = v4::Cursor>>> = vec![
        Arc::new(FsComponent::new(dir.path().to_path_buf(), 64)),
        Arc::new(ReadComponent::new()),
        Arc::new(AstNmComponent::new(
            "fn $NAME".to_string(),
            SupportLang::Rust,
        )),
        Arc::new(CountComponent {
            count: counter.clone(),
        }),
    ];
    let pipe = PipeInstance::new(steps);
    let queue: Arc<dyn QueueBackend<v4::Cursor>> = Arc::new(MemQueue::new());
    let opts = ExpandOpts::default().with_batch_cap(4096);
    let _stats = expand(&pipe, queue, vec![Arc::new(v4::Cursor::default())], opts);
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}
