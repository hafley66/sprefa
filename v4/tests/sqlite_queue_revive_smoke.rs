use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, NextKey, Node, PipeInstance, QueueBackend,
    QueueRow, RenderCtx, SqliteQueue, Wake,
};
use v4::Cursor;

struct Sink(Arc<Mutex<Vec<String>>>);

impl Component for Sink {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, cursor: &Cursor) -> Node<Cursor> {
        self.0.lock().unwrap().push(cursor.value().to_string());
        Node::Done
    }
}

fn cursor(value: &str) -> Arc<Cursor> {
    let mut cursor = Cursor::default();
    cursor.value = Arc::from(value);
    Arc::new(cursor)
}

#[test]
fn sqlite_queue_revives_parked_cursor_after_reopen_and_wake() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("queue.sqlite");
    let wake_key = NextKey([9; 32]);

    {
        let queue = SqliteQueue::<Cursor>::open_file(&db);
        queue.enqueue(QueueRow {
            id: 0,
            parent_id: None,
            batch_idx: 0,
            path: Vec::new(),
            pipe_hash: 42,
            instance_id: 7,
            depth: 0,
            value: cursor("revived"),
            wake: Wake::Key {
                domain: Cow::Borrowed("table_dirty"),
                key: wake_key,
            },
            expand_tick: 1,
            enqueued_at_ns: 0,
        });
        assert_eq!(queue.depth(), 1);
    }

    let queue: Arc<dyn QueueBackend<Cursor>> =
        Arc::new(SqliteQueue::<Cursor>::open_file(&db));
    assert_eq!(queue.dispatch_park("table_dirty", Some(wake_key)), 1);

    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut pipe = PipeInstance::new(vec![
        Arc::new(Sink(sink.clone())) as Arc<dyn Component<Next = Cursor>>,
    ]);
    pipe.pipe_hash = 42;
    pipe.instance_id = 7;

    let stats = expand(&pipe, queue, Vec::new(), ExpandOpts::default());

    assert_eq!(stats.rendered, 1);
    assert_eq!(&*sink.lock().unwrap(), &["revived".to_string()]);
}
