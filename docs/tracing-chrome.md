# DL_TRACE_CHROME: Perfetto timeline export

`DL_TRACE_CHROME=<path>` attaches a [`tracing-chrome`](https://crates.io/crates/tracing-chrome)
layer (exact-pinned `=0.7.2` in `Cargo.toml`; see the comment there) to the
process's tracing subscriber. Every span the process emits becomes a
chrome-trace-format JSON event at `<path>`; load the file at
[ui.perfetto.dev](https://ui.perfetto.dev) for a zoomable timeline. Unset, the
layer is never constructed — the only cost is one `env::var_os` check at
subscriber init (`src/trace.rs::chrome_layer`).

Works for both process shapes:

- one-shot CLI: layer installed in `trace::init`
- daemon: layer installed in `daemon::init_daemon_tracing`

## Repro

```sh
DL_TRACE_CHROME=/tmp/dl-trace.json dl examples/imports.dl --db /tmp/dl-trace.db
jq '.[0]' /tmp/dl-trace.json     # strict JSON once the process exits cleanly
```

Then open ui.perfetto.dev and drag `/tmp/dl-trace.json` in.

## Span vocabulary

| span | opened / closed | fields (`args` in the trace) |
|---|---|---|
| `tick` | `activity::begin_tick` / `activity::end_tick` | `tick`, `root`, `trigger` (`"full"`, `"paths"`, `"cold-extract"`) |
| `phase` | `activity::set` (closed by the next `set` or `end_tick`) | `tick`, `name` (declare/parse-extract/reconcile/derived/...), `detail` |
| `job` | `daemon_shell::jobs::worker_loop`, around the whole `JobRunner::run` | `kind`, `key`, `root`, `req_id` |

Nesting is by tracing's thread-local current-span stack: a daemon job runs its
tick synchronously on one blocking thread, so the timeline shows
`job > tick > phase` slices. `include_args(true)` is set, so the field values
identify WHICH tick/phase/job each slice is.

Pre-existing spans elsewhere in the crate (anything created with
`info_span!`/`debug_span!` under the subscriber's filter) also land in the
export; the three above are the deliberate timeline skeleton.

## Kill-safety (what a crash costs, measured)

`tracing-chrome`'s writer thread streams each span/event to a `BufWriter`
(8KB default capacity) around the output file as it happens, and writes the
closing `]` only when its `FlushGuard`'s `Drop` runs (signal writer thread ->
join -> write `]` -> flush). Two separate hazards:

- **`std::process::exit` skips `Drop`.** Several one-shot CLI exit paths call
  it directly (see `cli::run`). The guard therefore lives in a process-global
  slot (`trace.rs::CHROME_GUARD`, not a local returned up the call chain), and
  every such exit site calls `trace::finish_chrome_trace()` explicitly first
  (same "explicit, not RAII" pattern `invlog::record_end` already uses for the
  identical hazard). The daemon calls it once, in `shutdown_cleanup`, which
  both its graceful-shutdown paths (SIGINT/SIGTERM/RPC, and the tray's
  process::exit) reach.
- **SIGKILL loses whatever never reached disk.** `BufWriter` auto-flushes to
  the OS whenever its internal buffer fills, independent of any explicit call
  — this is WHY most of a run survives a kill even with no help from this
  arc. `activity::end_tick` also calls `trace::flush_chrome_trace()` (flush
  without close) at every completed tick boundary; this narrows the loss
  window for a DAEMON serving many short ticks ("since the last completed
  tick", not "since process start"), but buys nothing for a single long
  one-shot tick, since there is no earlier tick boundary to flush at — the
  measurement below is exactly that worst case.

**Measured** (`.dl/rails.dl --check` against this repo, 176 files, one ~13.8s
tick, `kill -9` 3s in): the clean run wrote 2107 chrome-trace events. The
killed run's raw file was cut off MID-EVENT (an unterminated JSON string, not
just a missing `]`) — `jq` reports `Unfinished string at EOF`. Manually
truncating back to the last complete `},` and appending `]` recovers 1996
events (95% of the clean run); the in-flight `tick` span in that recovered
file has only its `"B"` (begin) entry, no matching `"E"` — the trace itself
shows the run was killed mid-tick, which doubles as a diagnostic. `jq`
rejects the raw killed file (`jq -e 'type'` exits 5); ui.perfetto.dev's
loader is derived from Chromium's trace-viewer, which is documented to
recover a truncated trace by dropping the incomplete tail object and
re-closing the array — verify against the ui.perfetto.dev build in use before
depending on this for an actual incident, since that recovery behavior is
perfetto's, not this crate's or this repo's.

One path = one export: a second process writing to the same `DL_TRACE_CHROME`
path truncates the first export (`ChromeLayerBuilder::file` opens for write,
not append).
