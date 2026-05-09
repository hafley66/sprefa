use std::sync::{Arc, Mutex};

use ast_grep_language::SupportLang;
use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, Node,
    PipeInstance, QueueBackend, RenderCtx,
};
use v4::store::SprfStore;
use v4::v2_ops::{AstNmComponent, FsComponent};
use v4::{Cursor, WhereBytesId};

#[derive(Clone)]
struct CaptureSink {
    rows: Arc<Mutex<Vec<Cursor>>>,
}

impl Component for CaptureSink {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.rows.lock().unwrap().push(c.clone());
        Node::Emit(Arc::new(c.clone()))
    }
}

#[test]
fn fs_ast_reads_source_without_read_materializing_full_body() {
    let dir = tempfile::tempdir().unwrap();
    let body = "fn alpha() {}\nconst UNIQUE_TAIL_MARKER: &str = \"large-body-sentinel\";\n";
    std::fs::write(dir.path().join("a.rs"), body).unwrap();

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let sprf = SprfStore::new(facts);
    let rows = Arc::new(Mutex::new(Vec::new()));
    let steps: Vec<Arc<dyn Component<Next = Cursor>>> = vec![
        Arc::new(FsComponent::new(dir.path().to_path_buf(), 64).with_sprf_store(sprf.clone())),
        Arc::new(AstNmComponent::new("fn $NAME".to_string(), SupportLang::Rust).with_sprf_store(sprf.clone())),
        Arc::new(CaptureSink { rows: rows.clone() }),
    ];
    let pipe = PipeInstance::new(steps);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());

    expand(&pipe, queue, vec![Arc::new(Cursor::default())], ExpandOpts::default());

    let rows = rows.lock().unwrap().clone();
    assert_eq!(rows.len(), 1);
    assert!(
        !rows[0].value.contains("UNIQUE_TAIL_MARKER"),
        "match cursor should not carry whole file body in cursor.value",
    );
    let where_bytes = sprf
        .where_bytes_of(WhereBytesId::from(rows[0].at))
        .expect("match cursor has source byte range");
    assert_ne!(where_bytes.file, 0);
    assert!(where_bytes.hi > where_bytes.lo);
}
