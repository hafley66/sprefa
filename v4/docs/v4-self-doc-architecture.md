# V4 Self-Documenting Architecture

This file is intentionally written through sprf. The generated section is
bounded by HTML comments so normal prose can live around it.

<!-- sprf:self-doc:start -->
## Generated Formula

    source file cursor
      > read
      > comment(open, close) narrows cursor.value to the generated region
      > render markdown into cursor.value
      > write_cursor(:replace)

## Current Cursor Contract

| Step | Cursor value | Address used for write |
| --- | --- | --- |
| read | whole file bytes as text | whole-file coord |
| comment(open, close) | bytes inside marker pair | focal LO/HI point at inside bytes |
| render.markdown | generated markdown | preserves FS/LO/HI |
| write_cursor(:replace) | generated markdown | replaces only focal byte range |

## Why This Is Less Blunt

- The markers stay in the file.
- Only the inner generated region is replaced.
- Surrounding handwritten markdown remains untouched.
- Drift detection can skip the write when the source changed after it was indexed.

## Nested Pipe Output

- **Runtime batching**: [generation and collection boundaries](v4/docs/v4-runtime-batching.md)
- **SQL rule queries**: [SQLite-shaped rule querying](v4/docs/v4-sql-rule-query-plan.md)
- **V3 parity gaps**: [remaining parity surface](v4/docs/v4-v3-parity-gaps.md)


## Target Shape

    docs/ARCHITECTURE.md
      > read
      > comment(open_marker_regex, close_marker_regex)
      > render.markdown ...generated architecture rows...
      > write_cursor(:replace)

<!-- sprf:self-doc:end -->
