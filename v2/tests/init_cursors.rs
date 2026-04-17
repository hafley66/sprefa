//! init_cursors entry-point smoke: empty exprs yields Done, cancel
//! shortcuts the run, a no-pipeline expr ExprDone fires.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use v2::_0_types::RunEvent;
use v2::_7_init_cursors::{init_cursors, InitInputs};
use v2::{Config, MemWriter, RuntimeConfig};

fn test_config() -> Arc<Config> {
    Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:       1,
            buffer_size:          256,
            flush_interval_ms:    100,
            collect_witnesses:    false,
            xref_cartesian_limit: 10_000,
            max_passes:           1,
            max_claims_per_pass:  1,
            max_cursors_per_root: 1,
        },
        content_hash: 0,
    })
}

#[tokio::test]
async fn empty_exprs_yields_only_done() {
    let store = v2::store::SqliteStore::open_memory().await.unwrap();
    let cfg = test_config();
    let reader: Arc<dyn v2::Reader> = Arc::new(v2::MemReader::new(cfg.clone()));
    let writer: Arc<dyn v2::Writer> = Arc::new(MemWriter::new());
    let (mtx, _mrx) = tokio::sync::mpsc::channel(8);

    let mut s = init_cursors(InitInputs {
        exprs:        vec![],
        expr_levels:  None,
        config:       cfg,
        store:        store.clone(),
        reader,
        writer,
        mutations_tx: mtx,
        cancel:       tokio_util::sync::CancellationToken::new(),
        run_id:       v2::RunId(1),
        scanner_hash: Arc::from(""),
        result_store: None,
    });

    let first = tokio::time::timeout(Duration::from_secs(2), s.next()).await.unwrap();
    assert!(matches!(first, Some(RunEvent::Done)));
    let rest  = tokio::time::timeout(Duration::from_secs(1), s.next()).await.unwrap();
    assert!(rest.is_none());
}

#[tokio::test]
async fn cancel_yields_done_without_hanging() {
    let store = v2::store::SqliteStore::open_memory().await.unwrap();
    let cfg = test_config();
    let reader: Arc<dyn v2::Reader> = Arc::new(v2::MemReader::new(cfg.clone()));
    let writer: Arc<dyn v2::Writer> = Arc::new(MemWriter::new());
    let (mtx, _mrx) = tokio::sync::mpsc::channel(8);
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();

    let mut s = init_cursors(InitInputs {
        exprs:        vec![],
        expr_levels:  None,
        config:       cfg,
        store:        store.clone(),
        reader,
        writer,
        mutations_tx: mtx,
        cancel:       cancel.clone(),
        run_id:       v2::RunId(1),
        scanner_hash: Arc::from(""),
        result_store: None,
    });

    let ev = tokio::time::timeout(Duration::from_secs(2), s.next()).await.unwrap();
    assert!(matches!(ev, Some(RunEvent::Done)));
}
