# Lane: a machine-readable diagnostic channel for dl6

## What this is for

dl6 needs an LSP. The LSP itself is NOT built in Prolog and is not your job. Your
job is the seam it reads: the dl6 compiler must publish its diagnostics as
structured records that an external process can consume, alongside the human
messages it already prints.

The user's framing, verbatim: a diagnostic state somewhere that someone reads,
from a file or a stream. Not everything wired in Prolog. But the information and
the clock facts must flow out of how the compiler already works during compiling.
And the Prolog side of this must be beautiful.

## Base

First action, from the worktree root:

    git merge --ff-only 3d8c34e3

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not work
around it.

## What already exists, read all of it before writing anything

- `v6/prolog/0_refusal_messages.pl` (214 lines). ONE umbrella `prolog:message//1`
  over 77 dynamically inventoried refusal signatures, with a coverage test. This
  is the existing human renderer and it is the shape to preserve. Read its header
  comment: it already anticipates this work and reserves an `at(File, Line, Reason)`
  arm for it.
- `v6/prolog/compile/parse_dl.pl` (1655 lines). Positions appear ONLY in error
  throws, at lines 105, 134, and 191. Successful parses discard them. This is the
  gap.
- `v6/prolog/3_clock_check.pl` (563 lines), exporting `clock_refusal_reason/1`.
  Clock findings are diagnostics too and belong in the channel.
- `v6/prolog/compile/registry.pl`, `surface/5`, which is where refusals are
  declared, including `value(refuse(...))` rows.
- Roughly 108 `throw(...)` sites across the compiler: `lower.pl` 40,
  `1_host_expand.pl` 19, `analyze.pl` 15, `compile.pl` 11, and the rest scattered.

## The hazard that decides the design

Emitted TypeScript is graded on BYTE IDENTITY across a 135-module corpus. If you
change the shape of parsed terms, you risk changing emitted bytes, and that fails
the gate for reasons having nothing to do with diagnostics.

So: positions do NOT go inside existing terms. They go in a SIDE TABLE, keyed by
an identifier the parser assigns, leaving every existing term shape untouched.
Something of this shape, name it as you see fit:

    dl6_span(SpanId, File, StartLine, StartCol, EndLine, EndCol)

and whatever minimal key lets a refusal find its SpanId (rule index at worst,
term identity where the parser can afford it). If you find a design that beats a
side table without touching term shapes, take it and say why in REPORT.md.

## Deliverable

1. `v6/prolog/labs/diag_channel/diag.pl`: the structured emitter. One diagnostic
   record per line, JSON, on a stream selected by the caller. Default stderr.
   `DL6_DIAG_JSONL=<path>` redirects to that file instead. Use SWI's own
   `library(http/json)` for encoding; do not hand-roll JSON.

2. The record shape is LSP-compatible so a client needs no translation layer:

       {"uri": "...", "range": {"start": {"line": L, "character": C},
                                "end":   {"line": L, "character": C}},
        "severity": 1, "code": "<refusal signature functor>",
        "source": "dl6", "message": "<the same text the human renderer prints>"}

   LSP line and character are ZERO-based. Prolog's are 1-based. Convert once, in
   one predicate, and test that predicate.

3. Position retention in `compile/parse_dl.pl`, feeding the side table.

4. Clock findings from `3_clock_check.pl` routed through the same channel.

5. `v6/prolog/labs/diag_channel/diag.test.pl`, plunit.

## The rule that makes this beautiful rather than bolted on

There is ONE source of truth per diagnostic: the refusal term. It gets TWO
renderers. The existing `prolog:message//1` DCG renders it for humans, unchanged.
Your new renderer emits the same term as JSON. Neither renderer may know anything
the other does not, and neither may construct text the other cannot.

Concretely, this is the rail: if a diagnostic's JSON `message` field and the
umbrella renderer's human line ever disagree in content, that is a defect. Test
it across the inventory, do not assume it.

Do not add a second message table. Do not duplicate the 77 signatures. Do not
write a renderer that special-cases functors the umbrella already handles.

## Grading

Non-negotiable, run all of it and paste the output:

    cd v6/prolog && swipl -g go -t halt ARCH.pl
    just green-all

The existing battery is: conformance 281/0, plunit 276, TEXT_DOOR 196/196/0,
tsv2 128/1skip, store 74/74, dl 96/96. Any number that moves is a finding you
report, not a thing you fix by adjusting the test.

Plus, specific to this lane:

- Human message output must be BYTE IDENTICAL before and after your change.
  Prove it: capture the messages for a set of failing programs on the base
  commit, capture again after, diff. Paste the diff command and its empty result.
- Emit valid JSON. Prove it by piping every emitted line through a JSON parser
  and reporting the count.

## The honest number this lane exists to produce

Of the 77 refusal signatures in the inventory, how many can now report a REAL
source position, and how many still fall back to rule-index granularity?

Report it as a fraction with the list of which ones still fall back and why. A
lane that retrofits positions for 12 of 77 and says so plainly is a success. A
lane that claims 77 of 77 without a per-signature table is not.

## Scope discipline

You will not finish the whole span retrofit. That is expected and fine. Order of
work, and stop wherever you run out:

1. The channel, end to end, with rule-index positions only. This alone is useful.
2. Real positions for parse-time diagnostics, which is where they are cheapest.
3. Real positions for as many post-parse refusals as you can reach.

Do NOT edit `lower.pl` or `emit_ts.pl`. If a diagnostic can only get its position
by changing those, name it in REPORT.md and leave it alone.

## Style laws (repo-wide, enforced)

- The Prolog must be idiomatic and declarative. DCG for message rendering,
  terms for data, no imperative string building. The user asked for beautiful
  Prolog in this file specifically and will read it.
- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Comments state only constraints the code cannot show. No change-log narrative,
  no dates, no restating the next line.
- Descriptive names, never single-letter.
- N+1 is banned: never a per-row write, collect the set and write once.

## REPORT.md format

    # dl6 diag channel: REPORT
    ## Base proof
    ## What I built
    ## The one-source-two-renderers proof
    <how you tested that human and JSON never disagree>
    ## Position coverage
    <N of 77, with the per-signature table of what still falls back>
    ## Byte-identity proof
    <the diff command and its empty output>
    ## Gate output
    <every number from just green-all, verbatim>
    ## What I could not do
