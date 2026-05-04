// MemQueue smoke tests. Cover the four mechanics:
//   1. enqueue + pull FIFO on Immediate
//   2. SinkGen parking + wake when sink_gens advance
//   3. unmount-by-instance reaps all rows
//   4. depth bookkeeping stays correct across operations

use std::sync::Arc;

use v4::Cursor;
use v4::react::{
    Component, CursorNode, MemQueue, QueueBackend, QueueRow, Wake,
};
use v4::react::queue::DriveTick;
use v4::react::wake::SinkId;

fn cursor(name: &str, val: &str) -> Arc<Cursor> {
    Arc::new(Cursor { terms: vec![(name.into(), val.into())] })
}

fn row(id: u64, instance: u64, wake: Wake, c: Arc<Cursor>) -> QueueRow {
    QueueRow {
        id,
        parent_id:      None,
        batch_idx:      0,
        pipe_hash:      0,
        instance_id:    instance,
        pc:             0,
        cursor:         c,
        wake,
        drive_tick:     0,
        enqueued_at_ns: 0,
    }
}

#[test]
fn fifo_immediate_pull_order() {
    let q = MemQueue::new();
    q.enqueue(row(0, 1, Wake::Immediate, cursor(":n", "a"))).unwrap();
    q.enqueue(row(0, 1, Wake::Immediate, cursor(":n", "b"))).unwrap();
    q.enqueue(row(0, 1, Wake::Immediate, cursor(":n", "c"))).unwrap();

    let r1 = q.pull_runnable(&[], 0).unwrap().unwrap();
    let r2 = q.pull_runnable(&[], 0).unwrap().unwrap();
    let r3 = q.pull_runnable(&[], 0).unwrap().unwrap();
    assert_eq!(r1.cursor.terms[0].1.as_ref(), "a");
    assert_eq!(r2.cursor.terms[0].1.as_ref(), "b");
    assert_eq!(r3.cursor.terms[0].1.as_ref(), "c");
    assert!(q.pull_runnable(&[], 0).unwrap().is_none());
    assert_eq!(q.depth().unwrap(), 0);
}

#[test]
fn sink_gen_parking_and_wake() {
    let q = MemQueue::new();
    let sink: SinkId = 0;
    q.enqueue(row(0, 1, Wake::SinkGen {
        sink, key_pfx: vec![], past_gen: 5,
    }, cursor(":x", "1"))).unwrap();

    // Sink gen still 5: nothing runnable.
    let sink_gens = vec![5];
    assert!(q.pull_runnable(&sink_gens, 0).unwrap().is_none());

    // Sink advances to 6: row becomes runnable.
    let sink_gens = vec![6];
    let pulled = q.pull_runnable(&sink_gens, 0).unwrap().unwrap();
    assert_eq!(pulled.cursor.terms[0].1.as_ref(), "1");
    assert_eq!(q.depth().unwrap(), 0);
}

#[test]
fn unmount_reaps_instance_rows() {
    let q = MemQueue::new();
    q.enqueue(row(0, 100, Wake::Immediate, cursor(":n", "a"))).unwrap();
    q.enqueue(row(0, 100, Wake::Immediate, cursor(":n", "b"))).unwrap();
    q.enqueue(row(0, 200, Wake::Immediate, cursor(":n", "x"))).unwrap();
    assert_eq!(q.depth().unwrap(), 3);

    let n = q.unmount(100).unwrap();
    assert_eq!(n, 2);
    assert_eq!(q.depth().unwrap(), 1);

    // Only the instance-200 row remains.
    let r = q.pull_runnable(&[], 0).unwrap().unwrap();
    assert_eq!(r.instance_id, 200);
    assert_eq!(r.cursor.terms[0].1.as_ref(), "x");
}

#[test]
fn tick_parking() {
    let q = MemQueue::new();
    q.enqueue(row(0, 1, Wake::Tick { past_tick: 10 }, cursor(":n", "a"))).unwrap();

    // global_tick = 10 → still parked
    assert!(q.pull_runnable(&[], 10).unwrap().is_none());
    // global_tick = 11 → runnable
    let r = q.pull_runnable(&[], 11).unwrap().unwrap();
    assert_eq!(r.cursor.terms[0].1.as_ref(), "a");
}

#[test]
fn wake_index_lookup_lists_ready_rows() {
    let q = MemQueue::new();
    let sink: SinkId = 1;
    q.enqueue(row(0, 1, Wake::SinkGen {
        sink, key_pfx: vec![], past_gen: 0,
    }, cursor(":n", "a"))).unwrap();
    q.enqueue(row(0, 1, Wake::SinkGen {
        sink, key_pfx: vec![], past_gen: 5,
    }, cursor(":n", "b"))).unwrap();

    // sink advances to 3: only "a" (past_gen=0) is ready.
    let ready = q.wake_index_lookup(sink, &[], 3).unwrap();
    assert_eq!(ready.len(), 1);
    // sink advances to 10: both ready.
    let ready = q.wake_index_lookup(sink, &[], 10).unwrap();
    assert_eq!(ready.len(), 2);
}

// Component trait spot-check: trait is object-safe and a trivial impl
// works. Real driver tests come in Phase 3.
struct NoopComp;
impl Component for NoopComp {
    fn ident(&self) -> [u8; 32] { [0; 32] }
    fn render(&self, _: &v4::react::RenderCtx, c: &Cursor) -> CursorNode {
        CursorNode::Emit(Arc::new(c.clone()))
    }
}

#[test]
fn component_trait_is_object_safe() {
    let _comp: Arc<dyn Component> = Arc::new(NoopComp);
}
