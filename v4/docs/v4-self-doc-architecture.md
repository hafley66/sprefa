# V4 Self-Documenting Architecture

This file is intentionally written through sprf. The generated section is
bounded by HTML comments so normal prose can live around it.

<!-- sprf:self-doc:start -->
## Generated Formula

    source file cursor
      > read
      > match comment range and bind BODY as an addressable byte span
      > render markdown into cursor.value
      > write_cursor(:replace, :BODY)

## Current Cursor Contract

| Step | Cursor value | Address used for write |
| --- | --- | --- |
| read | whole file bytes as text | whole-file coord |
| re with (?P<BODY>...) | matched comment block | BODY term coord points at inside bytes |
| render(:markdown) | generated markdown | preserves BODY term coord |
| write_cursor(:replace, :BODY) | generated markdown | replaces only BODY byte range |

## Why This Is Less Blunt

- The markers stay in the file.
- Only the inner generated region is replaced.
- Surrounding handwritten markdown remains untouched.
- Drift detection can skip the write when the source changed after it was indexed.

## Target Shape

    docs/ARCHITECTURE.md
      > term_bind(:FS)
      > read
      > re (?s)<!-- sprf:start -->\n(?P<BODY>.*?)\n<!-- sprf:end -->
      > render(:markdown) ...generated architecture rows...
      > write_cursor(:replace, :BODY)

<!-- sprf:self-doc:end -->
