// React driver smoke test. Demonstrates Phase 3 mechanics:
//   - seed → enqueue → render → flatten → enqueue → ... → drain
//   - blanket SimpleOp → Component bridge works for existing ops
//   - terminal cursors past the pipe are counted, not re-rendered

use std::sync::{Arc, Mutex};

use v4::{Cursor, Trim};
use v4::react::{
    Component, CursorNode, DriveOpts, MemQueue, PipeInstance, RenderCtx, drive_react,
};

fn cursor(name: &str, val: &str) -> Arc<Cursor> {
    Arc::new(Cursor { terms: vec![(name.into(), val.into())] })
}

/// Test sink: clones each rendered cursor into a shared Vec.
struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
impl Component for Collector {
    fn ident(&self) -> [u8; 32] { [9; 32] }
    fn render(&self, _: &RenderCtx, c: &Cursor) -> CursorNode {
        self.sink.lock().unwrap().push(c.clone());
        CursorNode::Done
    }
}

/// Test fan-out: emits N copies of the input cursor.
struct FanOut { n: usize }
impl Component for FanOut {
    fn ident(&self) -> [u8; 32] { [7; 32] }
    fn render(&self, _: &RenderCtx, c: &Cursor) -> CursorNode {
        let nodes: Vec<CursorNode> = (0..self.n)
            .map(|i| {
                let mut copy = c.clone();
                copy.set(":fan_idx", i.to_string());
                CursorNode::Emit(Arc::new(copy))
            })
            .collect();
        CursorNode::Many(nodes)
    }
}

#[test]
fn drives_a_single_op_pipe() {
    let queue: Arc<dyn v4::react::QueueBackend> = Arc::new(MemQueue::new());
    let sink = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(Trim::new(":raw", ":clean")) as Arc<dyn Component>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let seed = vec![
        cursor(":raw", "  hello  "),
        cursor(":raw", "world\n"),
    ];

    let stats = drive_react(&pipe, queue.clone(), seed, DriveOpts::default()).unwrap();

    let collected = sink.lock().unwrap();
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].get(":clean").unwrap(), "hello");
    assert_eq!(collected[1].get(":clean").unwrap(), "world");
    assert_eq!(stats.rendered, 4);  // 2 trims + 2 collects
    assert_eq!(queue.depth().unwrap(), 0);
}

#[test]
fn many_fan_out_creates_three_rows_per_input() {
    let queue: Arc<dyn v4::react::QueueBackend> = Arc::new(MemQueue::new());
    let sink = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(FanOut { n: 3 }) as Arc<dyn Component>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let seed = vec![cursor(":raw", "x"), cursor(":raw", "y")];
    drive_react(&pipe, queue.clone(), seed, DriveOpts::default()).unwrap();

    let collected = sink.lock().unwrap();
    assert_eq!(collected.len(), 6); // 2 inputs × 3 fan
    let xs: Vec<_> = collected.iter().filter(|c| c.get(":raw") == Some("x")).collect();
    assert_eq!(xs.len(), 3);
    let fan_idxs: Vec<_> = xs.iter().map(|c| c.get(":fan_idx").unwrap()).collect();
    let mut sorted = fan_idxs.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["0", "1", "2"]);
}

#[test]
fn done_node_emits_nothing() {
    struct AlwaysDone;
    impl Component for AlwaysDone {
        fn ident(&self) -> [u8; 32] { [1; 32] }
        fn render(&self, _: &RenderCtx, _: &Cursor) -> CursorNode {
            CursorNode::Done
        }
    }
    let queue: Arc<dyn v4::react::QueueBackend> = Arc::new(MemQueue::new());
    let sink = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(AlwaysDone) as Arc<dyn Component>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    drive_react(&pipe, queue.clone(), vec![cursor(":x", "1")], DriveOpts::default()).unwrap();
    assert_eq!(sink.lock().unwrap().len(), 0);
    assert_eq!(queue.depth().unwrap(), 0);
}
