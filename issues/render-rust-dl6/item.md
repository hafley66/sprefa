---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: done
priority: normal
epic: dl6-first-typegen
labels:
- size:med
- area:typegen
- pkg:dl6
- pkg:prolog
closed: 2026-08-15
commits:
- hash: 294f4506
  summary: render_rust.dl6 twin of render_ts.dl6 + 12 rust goldens + golden-driver rust leg
---

# render_rust.dl6: the dl6 door renders Rust types

## Description

render_ts.dl6 exists and holds 12 goldens; the Rust type render still lives only in 8_emit_rust_types.pl. Port the renderer shape (type_row/7 JSONL in, rendered_type out) and judge it against the prolog door in typegen_golden.sh exactly as the TS pair is judged. Unblocked: PR #266 fixed the list_type<->element_type mutual closure this program needs.
