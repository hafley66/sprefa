# file_span design (coordinator, user-directed 2026-07-29 evening; supersedes the naked span pair)

USER CORRECTION, verbatim intent: a span is a FILE_SPAN. It references its
parent file, has a range of start and end, and text. The struct arc's
`type span(start: int, end: int)` free-floating pair is WRONG and is the
single hole behind four shipped warts:
1. `path` rides as a sibling column beside every span (df_node(path, at)).
2. flagship-flow.dl6 hand-builds identity: `concat([path,':',start,':',end])`
   -- that concat IS the missing file reference.
3. Comment rails shell out to grep for text (text lives nowhere).
4. Line/col live in a python referee translator + LSP line numbers are 0.

## The model
- `file` = (path, digest). digest = content identity; path = location.
- `file_span` = (file, start, end) -- a NESTED struct ref (value plane
  already supports child refs; types-r2 parent-hash-consumes-child receipt).
- TEXT IS NEVER STORED, always derived: (digest, start, end) IS the text
  identity. `text(span)` = content-addressed derivation (world reads bytes
  once per key, cached forever, digest kills staleness). Zero amplification.
- `line(span)` / `col(span)` derive through a per-digest newline index --
  the SAME absent derivation as the comment-lab "byte-span flattener", the
  LSP zeros, and the flow-referee python translator. Belongs in-language.
- `slice(span, rel_start, rel_end)` -> child file_span, same file,
  bounds-checked against parent. THIS is the user's match/scan/SLICE third
  leg (slice = sub-range projection, NOT destructuring -- coordinator
  misread it earlier; the running assign lab was told the wrong reading).
  Marker capture = slicing the comment span; grep hosts demote to
  optimization.

## What stays untouched
The extractor. Wire keeps bare byte ranges; the HOST BOUNDARY constructs
file_span values by pairing each record span with the demand's file
(host expansion already binds the file). Programs shed path columns.

## Storage
Dictionary rows: one file row per file, three ints per span. Replaces
per-row raw path text (measured 163KB-for-56-paths duplication in the
comment db). The interning-when-it-matters answer arrives structurally.

## User decision cards (open)
1. text() through a world host reading bytes (coordinator lean: git holds
   the bytes, digest pins staleness) vs a stored file-content plane.
2. `file` as both rel (existence fact) and type (reference in spans), or
   unified.
3. rev on the file value now (enumerate_at already speaks rev; the OG
   repo/tag checkout loop, archive-20260428 File/FileSpan precedent) or
   later.

## Migration sketch (unpriced, next lane's job)
type file_span in 0_type_plane; host expansion pairs demand file + record
span; flagship-flow/comment programs drop path columns + concat ids;
referee coordinate translation deleted in favor of line/col rules; span
fixtures regraded. Owner-shape: opus design execution or sol with this doc
as the contract.
