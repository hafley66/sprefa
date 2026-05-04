// Suspense + sink-gen wakeup. Phase 4 mechanic:
//   - AwaitSink returns Suspense{wake: SinkGen{...}}
//   - drive_react exits with parked rows
//   - tracker.advance(sink) bumps the gen
//   - re-entering drive_react picks up the parked row at pc+1

use std::sync::{Arc, Mutex};

use v4::Cursor;
use v4::react::{
    Component, CursorNode, DriveOpts, MemQueue, PipeInstance, RenderCtx,
    SinkGenTracker, Wake, drive_react,
};

fn cursor(name: &str, val: &str) -> Arc<Cursor> {
    Arc::new(Cursor { terms: vec![(name.into(), val.into())] })
}

struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
impl Component for Collector {
    fn ident(&self) -> [u8; 32] { [9; 32] }
    fn render(&self, _: &RenderCtx, c: &Cursor) -> CursorNode {
        self.sink.lock().unwrap().push(c.clone());
        CursorNode::Done
    }
}

/// Suspends every cursor that flows in, parking it on `sink` past_gen.
/// The cursor wakes once the tracker advances `sink` past `past_gen`.
struct AwaitSink { sink: u32, past_gen: u64 }
impl Component for AwaitSink {
    fn ident(&self) -> [u8; 32] { [3; 32] }
    fn render(&self, _: &RenderCtx, c: &Cursor) -> CursorNode {
        CursorNode::Suspense {
            cursor: Arc::new(c.clone()),
            wake: Wake::SinkGen {
                sink:    self.sink,
                key_pfx: Vec::new(),
                past_gen: self.past_gen,
            },
        }
    }
}

#[test]
fn suspense_parks_until_sink_gen_advances() {
    let queue: Arc<dyn v4::react::QueueBackend> = Arc::new(MemQueue::new());
    let tracker = Arc::new(SinkGenTracker::new());
    let collected = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(AwaitSink { sink: 0, past_gen: 0 }) as Arc<dyn Component>,
        Arc::new(Collector { sink: collected.clone() }),
    ]);

    let opts = DriveOpts::default().with_sink_gens(tracker.clone());

    let stats = drive_react(
        &pipe, queue.clone(), vec![cursor(":raw", "alpha")], opts.clone(),
    ).unwrap();
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.parked,   1, "row should be parked on sink 0");
    assert_eq!(collected.lock().unwrap().len(), 0);
    assert_eq!(queue.depth().unwrap(), 1);

    let new_gen = tracker.advance(0);
    assert_eq!(new_gen, 1);

    let stats = drive_react(&pipe, queue.clone(), Vec::new(), opts).unwrap();
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.parked,   0);
    let got = collected.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":raw").unwrap(), "alpha");
    assert_eq!(queue.depth().unwrap(), 0);
}

#[test]
fn many_cursors_park_on_same_sink_and_all_wake() {
    let queue: Arc<dyn v4::react::QueueBackend> = Arc::new(MemQueue::new());
    let tracker = Arc::new(SinkGenTracker::new());
    let collected = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(AwaitSink { sink: 0, past_gen: 0 }) as Arc<dyn Component>,
        Arc::new(Collector { sink: collected.clone() }),
    ]);

    let opts = DriveOpts::default().with_sink_gens(tracker.clone());

    let seed = vec![cursor(":i", "a"), cursor(":i", "b"), cursor(":i", "c")];

    let stats = drive_react(&pipe, queue.clone(), seed, opts.clone()).unwrap();
    assert_eq!(stats.parked, 3);

    tracker.advance(0);

    drive_react(&pipe, queue.clone(), Vec::new(), opts).unwrap();

    assert_eq!(collected.lock().unwrap().len(), 3);
    assert_eq!(queue.depth().unwrap(), 0);
}
