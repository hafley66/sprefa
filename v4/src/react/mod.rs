// React-shaped runtime for sprefa v4.
//
// This module hosts the new authoring surface (Component / CursorNode)
// and the queue-backed driver that runs it. Lives parallel to the
// legacy Op trait + drive_with for the duration of the migration; once
// every real op has a Component impl, the legacy path is deleted.
//
// See ../../../llm-notes.md "Op = Component. render returns CursorNode"
// for the design write-up. See
// ~/.claude/plans/okay-plan-and-ask-federated-flute.md for the phase
// plan.

pub mod node;
pub mod wake;
pub mod effect;
pub mod component;
pub mod queue;
pub mod mem_queue;
pub mod cursor_codec;
pub mod flatten;
pub mod sink_gen_tracker;
pub mod driver;

pub use node::{CursorNode, PipeHash, PipeRef};
pub use wake::{Wake, SinkId, ExternalToken};
pub use effect::ReactEffect;
pub use component::{Component, RenderCtx};
pub use queue::{QueueBackend, QueueRow, QueueId, InstanceId, DriveTick, SinkGenView};
pub use mem_queue::MemQueue;
pub use sink_gen_tracker::SinkGenTracker;
pub use driver::{drive_react, DriveOpts, DriveStats, PipeInstance};
