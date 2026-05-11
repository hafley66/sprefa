//! `Next` and `NextQ` Components — sprf's imperative-event surface
//! ported onto the v2 runtime's park/wake primitives.
//!
//!   `next(:chan)`   publish: fires `queue.dispatch_park("next", H(chan))`,
//!                   passes its input cursor through unchanged.
//!
//!   `next?(:chan)`  subscribe-park: on first render, parks on
//!                   `Wake::Key { domain: "next", key: H(chan) }`. On
//!                   re-render (after wake), emits the cursor and
//!                   clears the park-mark. Carries a magic
//!                   `:nextq:<chan>` flag so resume survives restart.
//!
//! Park/wake via queue.dispatch_park; payload semantics arrives via
//! FactStore on a side table when needed.

use std::sync::Arc;

use effect_runtime::v2::{Component, NextKey, Node, QueueBackend, QueueRow, RenderCtx, Wake};

use crate::Cursor;

const DOMAIN: &str = "next";

fn channel_key(chan: &str) -> NextKey {
    NextKey(*blake3::hash(chan.as_bytes()).as_bytes())
}

fn mark_key(chan: &str) -> String {
    format!(":nextq:{}", chan)
}

/// `next(:chan)`. Side-effect publish. Wakes every parker on `chan`.
pub struct Next {
    pub chan: Arc<str>,
}

impl Next {
    pub fn new(chan: impl Into<Arc<str>>) -> Self {
        Self { chan: chan.into() }
    }
}

impl Component for Next {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let key = channel_key(&self.chan);
        queue.dispatch_park(DOMAIN, Some(key));

        // Pass-through: emit each input cursor downstream. Use the
        // default render path manually since we overrode dispatch.
        use effect_runtime::v2::splice_into;
        for row in rows {
            let node = Node::Emit(row.value.clone());
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
        }
    }
}

/// `next?(:chan)`. Subscribe-park. First render parks on the channel's
/// wake-key. Wake re-renders the same cursor; the second pass emits.
pub struct NextQ {
    pub chan: Arc<str>,
}

impl NextQ {
    pub fn new(chan: impl Into<Arc<str>>) -> Self {
        Self { chan: chan.into() }
    }
}

impl Component for NextQ {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let mark = mark_key(&self.chan);
        if c.get(&mark).is_some() {
            // Second pass: clean the mark and emit.
            let mut next = c.clone();
            next.unset(&mark);
            return Node::Emit(Arc::new(next));
        }

        // First pass: stamp the mark, park.
        let key = channel_key(&self.chan);
        let mut parked = c.clone();
        parked.set(&mark, "1");
        Node::Yield {
            value: Arc::new(parked),
            wake: Wake::Key {
                domain: DOMAIN.into(),
                key,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::{expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend};
    use std::sync::Mutex;

    struct Collector {
        sink: Arc<Mutex<Vec<Cursor>>>,
    }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor_with_value(v: &str) -> Arc<Cursor> {
        let mut c = Cursor::default();
        c.set_value(v);
        Arc::new(c)
    }

    #[test]
    fn nextq_parks_until_dispatch_park_promotes() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(NextQ::new("alpha")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        let stats = expand(
            &pipe,
            queue.clone(),
            vec![cursor_with_value("seed")],
            ExpandOpts::default(),
        );
        assert_eq!(stats.parked, 1);
        assert_eq!(sink.lock().unwrap().len(), 0);

        let promoted = queue.dispatch_park(DOMAIN, Some(channel_key("alpha")));
        assert_eq!(promoted, 1);

        expand(&pipe, queue.clone(), Vec::new(), ExpandOpts::default());
        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(&*got[0].value, "seed");
        assert!(got[0].get(&mark_key("alpha")).is_none()); // mark cleaned
    }

    #[test]
    fn nextq_dispatch_only_wakes_matching_channel() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(NextQ::new("alpha")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue.clone(),
            vec![cursor_with_value("a")],
            ExpandOpts::default(),
        );

        let promoted = queue.dispatch_park(DOMAIN, Some(channel_key("beta")));
        assert_eq!(promoted, 0);
        assert_eq!(sink.lock().unwrap().len(), 0);
    }

    #[test]
    fn next_passes_cursor_through_unchanged() {
        // Next is a side-effect publisher that fires dispatch_park and
        // emits its input. With no parker in the queue, dispatch_park
        // is a no-op; the cursor still reaches downstream Collector.
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(Next::new("delta")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue,
            vec![cursor_with_value("payload")],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(&*got[0].value, "payload");
    }
}
