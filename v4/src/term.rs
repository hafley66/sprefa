//! `Term` — the v2 runtime Component that backs sprf's ALL-CAPS bare
//! identifier references.
//!
//! Two modes, both string-scoped over the current cursor:
//!
//!   `TERM`   (Read)  → cursor.value = cursor.get(name)   (mapBy / focus)
//!   `TERM?`  (Bind)  → cursor.set(name, cursor.value)    (capture)
//!
//! No braces. No carveouts. The pipe step is the term reference. The
//! lower-pass turns every bare ALL-CAPS ident at pipe position into a
//! `Term` Component; whether the trailing `?` is present picks the mode.

use std::sync::Arc;

use effect_runtime::v2::{Component, Diag, Node, RenderCtx};

use crate::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermMode {
    /// `TERM`. Pull the named capture into `&.value`. Diag if unbound.
    Read,
    /// `TERM?`. Push `&.value` into the named capture. Shadowing fine.
    Bind,
}

#[derive(Debug, Clone)]
pub struct Term {
    pub name: Arc<str>,
    pub mode: TermMode,
}

impl Term {
    pub fn read(name: impl Into<Arc<str>>) -> Self {
        Self { name: name.into(), mode: TermMode::Read }
    }
    pub fn bind(name: impl Into<Arc<str>>) -> Self {
        Self { name: name.into(), mode: TermMode::Bind }
    }
}

impl Component for Term {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        match self.mode {
            TermMode::Bind => {
                let mut next = c.clone();
                next.set_arc(&self.name, c.value.clone());
                Node::Emit(Arc::new(next))
            }
            TermMode::Read => {
                let v = match c.get(&self.name) {
                    Some(v) => v,
                    None    => {
                        ctx.diag.emit(
                            Diag::error("term/unbound",
                                format!("{} not bound", &self.name))
                        );
                        return Node::Done;
                    }
                };
                let mut next = c.clone();
                next.value = Arc::from(v);
                Node::Emit(Arc::new(next))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, ExpandOpts, MemQueue, Node, PipeInstance, QueueBackend,
        DiagSink, RenderCtx,
    };

    struct CollectorSink { sink: Arc<Mutex<Vec<Diag>>> }
    impl DiagSink for CollectorSink {
        fn emit(&self, d: Diag) { self.sink.lock().unwrap().push(d); }
    }

    struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor_with_value(v: &str) -> Arc<Cursor> {
        Arc::new(Cursor { value: v.into(), terms: Vec::new() })
    }

    #[test]
    fn bind_writes_value_into_named_term() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(Term::bind("X")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(&pipe, queue, vec![cursor_with_value("hello")], ExpandOpts::default());

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].get("X"), Some("hello"));
        assert_eq!(&*got[0].value, "hello");   // value unchanged on Bind
    }

    #[test]
    fn read_pulls_term_into_value() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(Term::bind("X")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Term::read("X")),    // identity-ish: X already bound
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(&pipe, queue, vec![cursor_with_value("hello")], ExpandOpts::default());

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(&*got[0].value, "hello");
        assert_eq!(got[0].get("X"), Some("hello"));
    }

    #[test]
    fn read_unbound_emits_diag_and_drops_row() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let diags = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(Term::read("Y")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);
        let opts = ExpandOpts::default()
            .with_diag(Arc::new(CollectorSink { sink: diags.clone() }));

        expand(&pipe, queue, vec![cursor_with_value("hi")], opts);

        assert_eq!(sink.lock().unwrap().len(), 0);
        let ds = diags.lock().unwrap();
        assert_eq!(ds.len(), 1);
        assert_eq!(&*ds[0].code, "term/unbound");
    }

    #[test]
    fn bind_then_mutate_then_read_focuses_back() {
        // X? then overwrite value via a synthetic mutator, then X
        // should restore X's bound value into &.value.
        struct OverwriteValue { v: &'static str }
        impl Component for OverwriteValue {
            type Next = Cursor;
            fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
                let mut next = c.clone();
                next.value = Arc::from(self.v);
                Node::Emit(Arc::new(next))
            }
        }

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(Term::bind("X")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(OverwriteValue { v: "DIFFERENT" }),
            Arc::new(Term::read("X")),
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(&pipe, queue, vec![cursor_with_value("first")], ExpandOpts::default());

        let got = sink.lock().unwrap();
        assert_eq!(&*got[0].value, "first");          // Read("X") restored
        assert_eq!(got[0].get("X"), Some("first"));   // X still bound
    }
}
