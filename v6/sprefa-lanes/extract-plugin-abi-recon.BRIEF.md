# BRIEF: recon, where the sprefa-extract plugin ABI / shared-memory line stopped

## Base
- Read-only recon on `91c5ea6e` (main). Verify with `git log --oneline -1`.
- You WRITE exactly two files, both new. You EDIT nothing.

## The question, in the user's words (2026-08-11)
"that will stress sprefa-extract abi or shared mem model for plugins outside of
rust so likely wasm or dense packed array of shared mem or sqlite as shared mem
or some shit, get sonnet to figure out where we left off on that"

Context that raised it: the tree-sitter grammar for dl6 is now 68% machine-
generated from the Prolog parser (merged `91c5ea6e`, `v6/labs/tree-sitter-door/`).
For `sprefa-extract` to USE a generated grammar, a non-Rust artifact has to
cross into a Rust process. That is the boundary this recon is about.

**You answer where the work stopped. You do not design the boundary and you do
not pick a transport.** Findings come back as cited forks; the user decides.

## What to establish, in this order

### 1. The current boundary
How does `v6/sprefa-extract` take work in and hand results out TODAY? Name the
process shape, the serialization, and the file:line of each seam. Start at
`v6/sprefa-extract/{AGENTS.md,Cargo.toml,proto/,src/,tools/}`. The `proto/`
directory suggests a wire format is already chosen; say which, and whether it
is actually used or vestigial.

### 2. What is already written down
These plans exist. Read each, date it, and say whether it landed, was
abandoned, or was never decided:

| plan | why it matters |
|---|---|
| `plans/2026-08-08-rust-ipc-transports.md` + `.d2` | the transport comparison, likely the direct ancestor of the user's question |
| `plans/2026-08-08-rust-ipc-rpc-frameworks.md` | the framework half of the same comparison |
| `plans/2026-07-30-sprefa-extract-spelunk.md` | the extract internals survey |
| `plans/2026-08-06-rust-emitter-modes.md` | rust emitter modes, adjacent |
| `plans/2026-07-29-sqlite-udf-graft-verdict.md` | "sqlite as shared memory" has a prior verdict here; find out what it said |
| `plans/2026-08-08-single-db-design-b.md` or `2026-07-20-single-db-design-b.md` | single-db shape |

Cross-check each against `v6/prolog/ARCH.pl`. Relevant rows to look up by name:
`extract_spelunk` (unbuilt), `doc_format_extraction` (unbuilt),
`file_span_kernel_host_boundary_lab` (labbed), `extraction_host_batching_lab`
(labbed), `extraction_host_batching` (done), `scip_families` (done).
An ARCH row's comment carries the landed detail; quote it, do not paraphrase.

### 3. The four candidate carriers the user named
For each, report ONLY what the repo already says, with citations. If the repo
says nothing about one, say "no prior work found" and stop. Do NOT research
libraries or invent a comparison; that is a later arc with its own brief.

| carrier | what to look for |
|---|---|
| wasm | any prior probe, any dependency, any plan mention |
| dense packed array over shared memory | same |
| SQLite as the shared medium | `2026-07-29-sqlite-udf-graft-verdict.md` first |
| the transport already in `proto/` | is it live |

### 4. The SCIP question the user raised and dropped
"how does scip work on that" — SCIP indexing is the ONE named exception to the
repo's 10-second law, and `scip_families` is done. Establish: does the current
extraction path depend on SCIP for the languages it handles, and would a
tree-sitter-generated grammar for dl6 need SCIP at all, or is SCIP orthogonal?
Cite the code. A one-paragraph answer with citations beats a survey.

### 5. The gap statement
One table: what a non-Rust plugin can do today, what it cannot, and the
file:line where the "cannot" is enforced. That table is the deliverable's
spine.

## What you must NOT do
- Do not edit any source file.
- Do not design a transport, pick a serialization, or rank the four carriers.
- Do not research external libraries. The repo's build-vs-buy law requires a
  written candidate-by-candidate analysis before any such call, and that is a
  separate arc with a separate brief.
- Do not spawn subagents.
- Do not report a limit you have not traced to a line of code. Per the repo's
  standing law, a comment is not the language and a stated limit needs its
  throw site cited.

## Deliverable
Two new files, nothing else:
1. `plans/2026-08-11-extract-plugin-abi-recon.md` — citations, file:line
   everywhere, for the auditor. Opens with a table of contents.
2. `plans/2026-08-11-extract-plugin-abi-recon.visual.human.unga.md` — plain
   words, diagrams, ZERO citations, written for a reader with no context. A
   plan without this second doc is undelivered.

Both open with the one-sentence answer to "where did we leave off".

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Tables, lists, and mermaid over prose. Prose is a caption under a diagram.
- Numbers come from tool output only. No vague quantity claims.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- Never announce location in text ("here is", "below is", "the following").
  Point with file:line or a node name.
