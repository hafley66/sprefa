# Lane brief: span-derived line, column, and slice-text projection

First action: `git merge --ff-only 1b90a018`. Failure = STOP AND REPORT.

## Goal

Issue `issues/span-line-column`. From a byte span owned by a file, project the
containing line, the column, and the slice text. This is a parity slice toward
porting v5 rails to dl6.

## Exact spec

1. Work in `v6/sprefa-engine-rs/src/text_plane.rs` (108 lines, the text-intern
   plane). If your addition crosses ~500 total lines, put it in a new sibling
   `src/text_project.rs` and wire it in `lib.rs`.
2. Add a pure projection API, shape:
   - input: file text (already interned in text_plane) + byte span (start, end)
   - output: start line (1-based), start column (1-based, byte column),
     end line, end column, and the slice text
   - a line-offset index built once per file text (Vec of line-start byte
     offsets, binary search per lookup), never a per-span rescan of the file.
3. Line/column derive from file BYTES, not stored spans. Keep the projection
   read-only and off the id columns; text_plane runs before apply_arrivals.
4. Unit tests in the same file or `tests/`: empty file, span at byte 0, span
   crossing a newline, span at EOF, multi-byte UTF-8 before the span (column
   is byte-based; state that in one comment), CRLF file.
5. Do NOT declare tick-path relations. If you conclude the projection needs
   relation declarations in `source_bind/_0_types.rs` to be useful, WRITE THAT
   as a note in your final commit message body and stop there; another lane
   owns those files tonight.

## Receipts (run each three times)

```bash
cd v6/sprefa-engine-rs && cargo test text
cd v6 && just scale-floor
```

Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`, never pipe a commit,
check `git log` before finishing.

## File ownership

OWNS: `v6/sprefa-engine-rs/src/text_plane.rs`, new `src/text_project.rs`,
`src/lib.rs` (module line only), tests for these.

FORBIDDEN: `src/hosts.rs`, `src/driver.rs`, `src/dep_resolve.rs`,
`src/source_bind/**`, `v6/tsv2/**`, `v6/prolog/**`. Another lane owns
hosts/driver/dep_resolve concurrently.

## Laws

- No `eprintln!`; `tracing` only.
- Comment budget: only constraints the code cannot show.
- No single-letter variable names.
- A permission denial ends the approach; report, never work around.
