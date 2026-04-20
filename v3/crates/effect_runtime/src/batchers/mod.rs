//! Reusable batcher topologies. Op authors don't touch these — they just
//! implement a `Work<E>` function and pick a batcher to wrap it at
//! registration time. Swapping topology is a one-line change in the
//! wiring, not in the op.
//!
//! Three shapes, matching what the throughput probe measured on the
//! linux kernel:
//!
//! - `Passthrough`    — direct compute in the submitter's task. Zero
//!                       concurrency, zero queue. Baseline shape.
//! - `WorkSteal`      — rayon thread pool. Per-request spawn; reply
//!                       via tokio oneshot. Pull-driven, no queue, no
//!                       batching. Best for homogeneous CPU work
//!                       (parse, match, fixed-string scan).
//! - `BoundedBatched` — crossbeam bounded channel + N worker threads
//!                       coalescing up to `max_batch` items per drain.
//!                       Push-driven with backpressure. Best for
//!                       effects that amortize (sqlite tx, git ODB
//!                       writes, network batch RPC).
//!
//! All three satisfy `Batcher<E>`, so op code is identical across them.

pub mod bounded_batched;
pub mod bounded_work_steal;
pub mod passthrough;
pub mod work_steal;

pub use bounded_batched::BoundedBatched;
pub use bounded_work_steal::BoundedWorkSteal;
pub use passthrough::Passthrough;
pub use work_steal::WorkSteal;
