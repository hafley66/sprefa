# Slice 1: reader, CST, and printer

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/compile/parse_dl_dcg.pl`
- `v6/prolog/print_dl.pl`
- `v6/prolog/0_cst_query.pl`
- `v6/prolog/compile/scripts/text_door_receipt.pl`
- parser-facing tests under `v6/prolog/compile/test/`

Trace the exact first semantic term produced after parsing and the exact term
accepted by the printer. Separate reusable literal/comment/location machinery
from DL6 punctuation, declaration, braces, arrows, and application grammar.

Write `v7/1_AUDIT/results/1_READER.md`.
