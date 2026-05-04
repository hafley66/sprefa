// Effect runner — synchronous Phase 5 dispatch.
//
// Walks a `CursorNode` tree returned from render, dispatches each
// `Effect{eff, then}` it encounters, and substitutes `then` in place.
// The resulting tree has no Effect variants left and is fed straight
// into `flatten`.
//
// Phase 5 effect set: Print (logs via hook), CommitChunk (advances
// sink gen, no Store yet), AwaitSink/Tick/External (rejected — these
// belong as Suspense or in a later async phase).

use std::sync::Arc;

use anyhow::{Result, anyhow};

use super::effect::ReactEffect;
use super::node::CursorNode;
use super::sink_gen_tracker::SinkGenTracker;
use super::wake::SinkId;

/// Hook called once per `Print` effect. Default writes to stderr; tests
/// override to capture output.
pub type PrintHook = Arc<dyn Fn(&str) + Send + Sync>;

pub fn default_print_hook() -> PrintHook {
    Arc::new(|s: &str| eprintln!("[react.print] {}", s))
}

/// One-shot side-effect dispatch. Pure on input; mutates `sink_gens`
/// and calls `print_hook` for Print. Returns Err for unsupported
/// variants in Phase 5.
pub fn dispatch(
    eff: &ReactEffect,
    sink_gens: &SinkGenTracker,
    print_hook: &PrintHook,
) -> Result<()> {
    match eff {
        ReactEffect::Print(s) => {
            print_hook(s);
            Ok(())
        }
        ReactEffect::CommitChunk { sink, rows: _ } => {
            // Phase 5 stub: advance the sink gen so SinkGen-parked rows
            // wake. Phase 6 will also write rows via Store::insert_many.
            sink_gens.advance(*sink);
            Ok(())
        }
        ReactEffect::AwaitSink { .. } => Err(anyhow!(
            "AwaitSink as Effect not supported; use CursorNode::Suspense \
            with Wake::SinkGen instead"
        )),
        ReactEffect::Tick(_) => Err(anyhow!(
            "Tick effect requires async runtime; deferred"
        )),
        ReactEffect::External(_) => Err(anyhow!(
            "External effect deferred"
        )),
    }
}

/// Walk `node`, dispatch every Effect found, return an Effect-free tree.
/// Effect{eff, then} → resolve_effects(then) after dispatching eff.
pub fn resolve_effects(
    node: CursorNode,
    sink_gens: &SinkGenTracker,
    print_hook: &PrintHook,
) -> Result<CursorNode> {
    match node {
        CursorNode::Done       => Ok(CursorNode::Done),
        CursorNode::Emit(c)    => Ok(CursorNode::Emit(c)),
        CursorNode::Suspense { cursor, wake } => Ok(CursorNode::Suspense { cursor, wake }),
        CursorNode::Mount { .. } => Err(anyhow!(
            "Mount not implemented in Phase 5"
        )),
        CursorNode::Many(children) => {
            let mut out = Vec::with_capacity(children.len());
            for c in children {
                out.push(resolve_effects(c, sink_gens, print_hook)?);
            }
            Ok(CursorNode::Many(out))
        }
        CursorNode::Effect { eff, then } => {
            dispatch(&eff, sink_gens, print_hook)?;
            resolve_effects(*then, sink_gens, print_hook)
        }
    }
}

/// Convenience for tests: capture Print output into a shared Vec.
pub fn capturing_print_hook(buf: Arc<std::sync::Mutex<Vec<String>>>) -> PrintHook {
    Arc::new(move |s: &str| buf.lock().unwrap().push(s.to_string()))
}

#[allow(dead_code)]
pub fn sink_id(s: SinkId) -> SinkId { s }
