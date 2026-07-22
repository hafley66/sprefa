//! CORE, stream form — THE ATTEMPT THAT DOES NOT COMPILE.  (gated so default
//! `cargo build` stays green; reproduce the wall with:)
//!
//!   cargo build --example core_frp_attempt --features frp-core-attempt
//!
//! Goal: express `derive_family_batch` as a stream graph — a Stream of source
//! files, flat_mapped to a Stream of `Hit<'r>`, driven concurrently on a real
//! scheduler (the thing you'd reach for to replace rayon). Two independent walls
//! appear, both rooted in the SAME fact: `Hit<'r>` borrows the file buffer, and
//! every stream tool that gives you concurrency demands `'static`.

use frp_lab::{extract, File, Hit};
use futures::executor::ThreadPool;
use futures::stream::{self, StreamExt};
use futures::task::SpawnExt;

// ---- WALL 1: the scheduler demands 'static, the hit borrows the buffer --------
//
// To parallelize per-file extraction (rayon's job) on a stream scheduler you spawn
// the work onto a pool. `ThreadPool::spawn` requires `Future: Send + 'static`. The
// future captures `&'r File`, which is NOT 'static -> "borrowed data escapes".
// This is the DataLoader trap the CLAUDE.md warns of, made mechanical: to stream
// it, you must first sever the borrow (own everything), which is the batch you
// were trying to avoid.
fn wall_1_spawn_extract(pool: &ThreadPool, files: &[File]) {
    for file in files {
        // ↓ the future borrows `file`; the pool wants 'static. Does not compile.
        pool.spawn(async move {
            let hits: Vec<Hit<'_>> = extract(file);
            let _ = hits.len();
        })
        .unwrap();
    }
}

// ---- WALL 2: a lifetime-parametric stream infects the whole runtime -----------
//
// The session's model is "everything is a stream held by id in the store". Hold a
// stream of borrowed hits and the `'r` leaks into the runtime type itself: the
// store can no longer outlive ANY source buffer it ever read. Contrast the batch
// runtime, `struct Runtime { edges: BTreeSet<Edge> }`, which owns and has no `'r`.
struct StreamRuntime<'r> {
    // BoxStream carries 'r; the whole store is now lifetime-welded to the corpus.
    hits: futures::stream::BoxStream<'r, Hit<'r>>,
}

fn wall_2_runtime<'r>(files: &'r [File]) -> StreamRuntime<'r> {
    let hits = stream::iter(files)
        .flat_map(|file| stream::iter(extract(file))) // Stream<Item = Hit<'r>>
        .boxed();
    // To actually JOIN across hits (the fixpoint) you must `.collect().await` the
    // whole set anyway — i.e. re-batch inside the stream. The stream bought nothing
    // and cost a lifetime parameter on every type that touches the store.
    StreamRuntime { hits }
}

fn main() {
    let files = vec![
        File { path: "a.rs".into(), text: "main parse\nmain lower".into() },
        File { path: "b.rs".into(), text: "parse lex".into() },
    ];
    let pool = ThreadPool::new().unwrap();
    wall_1_spawn_extract(&pool, &files);
    let _rt = wall_2_runtime(&files);
}
