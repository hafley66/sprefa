# Rust IPC Transports: same-machine data movement (Linux / macOS)

Research notes for: how two Rust processes on one machine move data. Scope:
transports and payload formats only. RPC frameworks and real-world case
studies are owned by the concurrent lane `ipcrpc` and are not covered here.

All crates.io download numbers below were fetched from the crates.io API on
2026-08-08.

## Table of contents

1. [Verdict table](#1-verdict-table)
2. [SQLite WAL as IPC, the finding](#2-sqlite-wal-as-ipc-the-finding)
3. [Decision table](#3-decision-table)
4. [The d2 board](#4-the-d2-board)
5. [Anecdata](#5-anecdata)
6. [UNVERIFIED](#6-unverified)

## 1. Verdict table

One row per candidate. Latency classes are rough orders of magnitude from the
cited sources; exact numbers are in [Anecdata](#5-anecdata) and
[UNVERIFIED](#6-unverified).

### Shared memory

| candidate | version + date | downloads total / recent | what it is | latency class | cross-platform | verdict | one reason |
|---|---|---|---|---|---|---|---|
| iceoryx2 | 0.9.3, 2026-07-08 | 469,016 / 212,999 | lock-free zero-copy pub/sub + events IPC | microsecond, vendor claims flat vs payload size | Linux tier2, macOS tier2, Windows tier2 | consider | real zero-copy shm with wake, but a heavy dependency; use when UDS throughput is the bottleneck |
| shared_memory | 0.12.4, 2022-03-01 | 4,846,162 / 1,029,777 | portable cross-platform shared-memory crate | microsecond | yes (unix + windows) | consider | simple shm allocator; pair with raw_sync |
| raw_sync | 0.1.5, 2020-10-13 | 767,791 / 211,319 | sync primitives (mutex/rwlock/barrier) over shared memory | microsecond | yes | consider | gives locks and fences for a hand-rolled shared buffer |
| memmap2 | 0.9.11, 2026-06-22 | 304,878,805 / 68,017,143 | safe wrapper over mmap | microsecond | yes (unix; windows via its own path) | use | the building block for any roll-your-own shm transport |
| POSIX shm / /dev/shm | OS kernel | n/a | OS-level anonymous shared memory | microsecond | no (no Windows equivalent) | reject for portable code | no Windows path; memmap2 / iceoryx2 cover it with less risk |

### Sockets and pipes

| candidate | version + date | downloads total / recent | what it is | latency class | cross-platform | verdict | one reason |
|---|---|---|---|---|---|---|---|
| interprocess | 2.4.3, 2026-08-01 | 12,537,588 / 3,523,394 | uniform local-socket IPC: UDS on unix, named pipes on windows | tens to hundreds of microseconds | yes (Windows, Linux, macOS explicit) | use | portable bidirectional streaming with a tokio async path |
| std::os::unix::net::UnixStream | std | n/a | raw Unix domain socket | microseconds | unix only | use on unix | zero deps, point-to-point byte stream |
| tokio::net::UnixStream | tokio | n/a | async Unix domain socket | microseconds | unix only | consider | same transport under the async reactor |
| named pipes (Windows) | OS kernel | n/a | Windows bidirectional pipe | microseconds | windows only | reject for linux target | the windows-only half of the seam interprocess abstracts |
| stdio pipes (LSP model) | std | n/a | child stdin/stdout, newline-framed JSON | millisecond | yes | use | natural fit for one parent + one child pair |

### Payload formats, the layer above the transport

| candidate | version + date | downloads total / recent | what it is | latency class | cross-platform | verdict | one reason |
|---|---|---|---|---|---|---|---|
| rkyv | 0.8.18, 2026-08-05 | 139,162,724 / 33,948,312 | zero-copy deserialization framework | near-zero on read | yes | use with shm | zero-copy read straight out of the shared buffer |
| arrow-ipc | 59.2.0, 2026-08-06 | 76,638,645 / 17,183,459 | columnar batch IPC format | low | yes | consider | only worth it if the workload is columnar batches |
| capnp | 0.27.0, 2026-08-02 | 13,463,398 / 2,112,159 | schema + zero-copy wire format | low | yes | consider | zero-copy plus a schema, heavier build tooling |
| flatbuffers | 25.12.19, 2025-12-19 | 90,907,569 / 18,718,456 | schema + zero-copy wire format | low | yes | consider | same trade as capnp, Google toolchain |
| postcard | 1.1.3, 2025-07-24 | 50,618,804 / 18,950,338 | wire-frugal serde format | case by case | yes | consider | compact bytes for constrained wire |
| bincode | 3.0.0, 2025-12-16 | 289,929,031 / 55,022,566 | compact binary serde format | low | yes | use | default compact binary when zero-copy is not needed |
| serde_json | 1.0.151, 2026-07-20 | 1,151,341,314 / 263,470,195 | JSON | highest CPU of the set | yes | use for compat/debug | universal and human readable, pays the most per byte |

### Kernel mechanisms worth naming

| candidate | where | what it is | relevance to same-machine IPC |
|---|---|---|---|
| io_uring | Linux, kernel 5.1+ | async file/network I/O with SQ/CQ rings, syscall avoidance | reject as IPC: it optimizes file and network I/O, it is not a cross-process message channel |
| flock | all unix | advisory file lock on an open file | use: advisory means only cooperating processes honour it |
| eventfd | Linux, 2.6.22+ | file-descriptor event counter for wakeups | use on Linux: the standard buffer + wakeup pattern |
| signalfd | Linux, 2.6.22+ | signal delivery as a readable fd | alternative Linux wake source alongside eventfd |
| kqueue | BSD, macOS | kernel event notification, EVFILT_USER and EVFILT_SIGNAL | use on macOS: the analog; macOS has no eventfd or signalfd |
| memfd_create + SCM_RIGHTS | Linux, 3.17+ | anonymous tmpfs file, fd passed over a unix socket | use on Linux: nameless shared memory plus a clean fd handoff |

### Databases-as-IPC

| candidate | version + date | what it is | latency class | cross-platform | verdict | one reason |
|---|---|---|---|---|---|---|
| SQLite WAL | 3.51.x, 2026 | durable shared SQL store, mmap wal-index | millisecond | yes | use, partial | the assumption is partly right; see the finding section |
| LMDB | ~0.9 | memory-mapped B-tree key/value store | microsecond / mmap | yes | consider | multi-process friendly; the explicit recommendation for read-mostly multi-process use |
| RocksDB | (server engine) | LSM key/value storage engine for a single server process | microsecond | yes | reject for IPC | built for one long-running process, not a read/notify fabric |
| sled | beta | embedded lock-free key/value log | microsecond | yes but single instance per process | reject for IPC | explicitly does not support multiple open instances |

## 2. SQLite WAL as IPC, the finding

The working assumption: one SQLite file in WAL mode is already IPC because the
`-shm` file is an mmap'd shared region coordinating cross-process readers and
writers, and readers do not block the writer. That premise is partly correct,
and the break is not in the shared buffer.

Truth about the -shm premise:

- The wal-index is an mmap'd shared region. "The wal-index is implemented
  using an ordinary file that is mmapped for robustness." https://www.sqlite.org/wal.html#section_7
- Reading and writing run concurrently. "readers do not block writers and a
  writer does not block readers." https://www.sqlite.org/wal.html#overview
- These are real cross-process guarantees, and WAL is same-host only: "all
  processes using a database must be on the same host computer; WAL does not
  work over a network filesystem." https://www.sqlite.org/wal.html#overview

Where the "already IPC" framing breaks:

- Single writer. "since there is only one WAL file, there can only be one
  writer at a time." Concurrent writers surface as SQLITE_BUSY in specific
  cases. https://www.sqlite.org/wal.html#section_2_2 and
  https://www.sqlite.org/wal.html#section_9
- The busy escalation is a retry/poll policy, not a backpressure channel.
  `sqlite3_busy_timeout` sleeps up to a deadline then returns SQLITE_BUSY.
  https://sqlite.org/c3ref/busy_timeout.html
- No push notification across processes. `sqlite3_commit_hook` and
  `sqlite3_update_hook` register callbacks on a single database connection;
  they fire in the process that committed, not in any other process.
  https://sqlite.org/c3ref/commit_hook.html and
  https://sqlite.org/c3ref/update_hook.html
- `sqlite3_wal_hook` is the closest thing to a commit notification, and it is
  also per-connection: "A single database handle may have at most a single
  write-ahead log callback registered at one time," invoked by the connection
  that committed. It does not deliver to reader processes. A reader must
  poll or be woken by an external fd, which SQLite does not provide.
  https://www.sqlite.org/c3ref/wal_hook.html
- Read cost grows with WAL size, so checkpointing matters; a reader that is
  always open starves checkpoints and the WAL grows without bound.
  https://www.sqlite.org/wal.html#section_2_3 and
  https://www.sqlite.org/wal.html#section_6
- A corruption bug, the WAL-reset bug, is tied to multiple connections in
  separate threads or processes writing or checkpointing at the same instant.
  Present from 3.7.0 (2010-07-21) through 3.51.2 (2026-01-09); fixed in 3.51.3
  (2026-03-13), backports in 3.44.6 and 3.50.7. A reader/writer IPC design
  must run a patched SQLite. https://www.sqlite.org/wal.html#section_11
- Locking is POSIX advisory, which is unreliable on some network mounts.
  "POSIX advisory locking is known to be buggy or even unimplemented on many
  NFS implementations (including recent versions of Mac OS X)." If the file
  sits on a network mount, the whole WAL-as-IPC model is unsafe.
  https://www.sqlite.org/lockingv3.html#how_to_corrupt

Conclusion: SQLite WAL is reliable same-machine shared storage with
coordinated multi-reader, single-writer concurrency, and it removes the
durability work of a bespoke transport. It is not a notification transport:
readers poll, one writer owns the file, and per-connection hooks do not cross
processes. For row-oriented records with a millisecond budget it is the right
tool. For microsecond streaming of many small frames it is the wrong tool,
because the reader side has no wakeup and every small commit pays SQLite page
and checkpoint machinery.

## 3. Decision table

| workload shape | transport | why |
|---|---|---|
| row-oriented records, millisecond budget (the boop / agent-session case) | SQLite WAL (single writer) | durable, already the storage layer, no new transport |
| streaming frames, microsecond budget | iceoryx2, or memmap2 + raw_sync + eventfd/kqueue | zero-copy shared buffer with a proper wakeup |
| large blobs | shared memory (iceoryx2 / memfd + memmap2) | avoids copying the payload into a socket |
| many readers, one writer | SQLite WAL or shm pub/sub (iceoryx2) | WAL gives readers without blocking the writer; shm gives the lowest read cost |
| bidirectional request/response | UDS via interprocess (or std UnixStream on unix) | portable, framed, point-to-point |
| one parent, one child (LSP style) | stdio pipes, newline JSON | minimal machinery, natural lifecycle |

Payload rules of thumb: use rkyv (or capnp/flatbuffers if you want a schema)
only when you are on a zero-copy shared buffer and reading repeatedly; use
bincode for compact binary when a plain copy per message is fine; use
serde_json for compat and debugging. Format choice matters in profile only
where serialization is a measured hot path, which for a millisecond-budget
SQLite reader it is not.

## 4. The d2 board

`plans/2026-08-08-rust-ipc-transports.d2`. Layer stack: kernel mechanism,
transport, framing, payload format. Compile gate numbers:

```
viewBox="0 0 1688 864"
21 shape-counted labels
```

Compiled with d2 0.7.1 on 2026-08-08, wider than tall (1688 x 864), under the
24-shape limit (21 by the counted metric).

## 5. Anecdata

Labelled as anecdotal / vendor claims, not measured here.

- iceoryx2 markets itself on "consistently low transmission latency
  regardless of payload size" and publishes a mechanism benchmark plot on its
  README; the plot was not re-measured in this pass. Vendor claim.
  https://github.com/eclipse-iceoryx/iceoryx2 (README, "Performance")
  accessed 2026-08-08.
- interprocess README calls local sockets "a much more appropriate
  alternative to localhost TCP sockets, featuring better performance and
  developer- and user-friendly identifiers." Vendor claim, no numbers.
  https://github.com/kotauskas/interprocess README, accessed 2026-08-08.
- The io_uring article series frames it strictly as file and network I/O: cat,
  a file copier, and a web server that "queues accept(), readv() and writev()"
  operations. No mention of same-machine process-to-process messaging.
  https://unixism.net/2020/04/io-uring-by-example-article-series/ accessed
  2026-08-08.

## 6. UNVERIFIED

Anything below could not be sourced with a methodology in this pass.

- Exact microsecond numbers for iceoryx2 vs UDS vs SQLite WAL on this machine:
  NOT VERIFIED. iceoryx2 ships a benchmark plot but the values were not
  extracted or reproduced.
- macOS kqueue throughput vs Linux eventfd wakeup latency: NOT FOUND with
  numbers.
- LMDB's own wording for multi-process mmap access: only the sled README
  recommendation was captured ("if you have a multi-process workload that
  rarely writes, use LMDB", https://github.com/spacejam/sled README). LMDB's
  own doc wording NOT FOUND in this pass.
- RocksDB cross-process sharing posture: inferred from the project wiki's
  "designed for application servers wanting to store up to a few terabytes"
  phrasing; no explicit "single process only" statement was captured.
  https://github.com/facebook/rocksdb/wiki
