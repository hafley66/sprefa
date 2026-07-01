# Beyond Manual Paging

Chapter 5 told a story: the data is big, so you keep a bounded resident set and
page the rest to disk. That story is true, and it is also a *frame* — and frames
decide which questions you are allowed to ask. This short series is about the
questions the manual-paging frame hides, and the bodies of algorithm theory that
only become visible once you stop assuming the frame.

Read [01 — the six bodies](01-the-six-bodies.md) for the per-field detail with
citations and bounds. This page is the frame and why it constrains you.

## The frame, written out

"Manually page memory" sounds like one decision. It is actually three assumptions
stacked on top of each other, and each one is a wall:

1. **The closure is a relation you store.** It is big, therefore it must live
   somewhere, therefore you page it.
2. **You decide what is resident.** Residency is your job, so you spend design
   effort on what to keep and when to evict.
3. **You optimise operation counts (the RAM model).** Uniform O(1) memory access
   is the cost model; paging is a patch bolted on after the fact.

Every assumption is reasonable. Every assumption also hides a field that only
appears when you drop it.

## What each wall hides

| Drop this assumption | The field you can suddenly see | The one-line idea |
|---|---|---|
| "the closure is a stored relation" | **Reachability / distance labeling** | The closure is a *sub-quadratic index*, not a materialised relation. Answer `reaches(a,b)` without ever building the O(n²) table. |
| "the representation is bulky" | **Succinct / compressed structures** | Make the data small enough to be resident and *query it without decompressing*. A 133 MB graph can be a few MB. |
| "I must hold all the edges" | **Semi-streaming + linear sketches** | Answer graph questions in memory proportional to *vertices*, one pass, with deletions supported. |
| "I hand-pick the page size" | **External-memory + cache-oblivious** | One recursive layout is I/O-optimal at *every* level of the hierarchy at once, with no tuning knobs. |
| "residency is my job" | **mmap + competitive paging** | The kernel's LRU is provably near-optimal. Manual paging is a job you can hand back. |
| "I materialise, then maintain" | **Differential / z-set algebra** | Hold the *change*, not the data. "Where the bytes live" becomes "we only ever hold deltas." |

## The punchline

Two earlier threads in this project both pointed at the same place. Magic sets (in
the [logic-language survey](../logic-language-survey/01-the-six-languages.md#an-onramp-from-datalog))
attacked "don't materialise the whole closure" by changing the *evaluation order*.
Reachability labeling attacks the identical target by changing the *data
structure*. When two independent angles converge on "stop materialising the full
closure," the materialisation itself is the constraint, and its memory placement
is a downstream symptom.

So the highest-leverage move is one level up from paging: **shrink or re-represent
the object** — by labeling, by compression, by sketching, by deltas — until the
working set fits, then let mmap page whatever is left. The manual-paging story
spends effort moving a big thing through a small window. Most of this field's
answer is: make the thing small enough that there is no window.

## How to use this series

[01 — the six bodies](01-the-six-bodies.md) takes each field in turn: the core
idea, the concrete size and query-time bounds, the canonical citations, and a
sprefa onramp. The [reading list](01-the-six-bodies.md#reading-list) at the end is
a single table of papers with their bounds, sorted by how cheaply you could try
them against the current engine.
