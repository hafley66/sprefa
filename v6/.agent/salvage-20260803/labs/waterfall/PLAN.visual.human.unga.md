SESSION WATERFALL - PLAIN WORDS AND A PICTURE
==============================================

THE SHAPE OF IT
---------------
The in-tab strip (the little bar under a terminal that lists the agent shells
you jumped to) has a checkbox on it now, next to the scope switch and refresh:

    [ ] Show active

It comes checked. Checked = exactly what you see today. Just the going-on
shells in the normal table. Nothing new.

Uncheck it and the bar switches into history mode. Now it turns into a
devtools network timing view, but for agent sessions instead of web requests.
One session per row. A bar that runs from when the session started to its last
activity. Little colored dots on the bar, one per message, color and shape by
what kind of message it is. Across the top a brush strip that sets the time
window. The table below only holds sessions that touch that window, so it
stays small no matter how much history exists.

ASCII PICTURE
-------------

  [ ] Show active         scope: related            refresh

  brush strip (the whole timeline, all sessions squashed to one thin band)
  +----------------------------------------------------------------------+
  |  ▁▁▁██▁▁▁▁▁▁██▁▁██▁▁▁▁▁▁▁██▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁   |
  |        [   dragged window: shows only this slice below       ]       |
  +----------------------------------------------------------------------+

  waterfall (detail = whatever the brush window covers)
  10:00      10:10      10:20      10:30      10:40
  |----------|----------|----------|----------|----------|
  coordinator  ██████████████████████████████        <- session bar
                  ●        ●        ●                      <- message dots
  oc-lane         ████████████████
                   ●       ●        ●●
  codex-other               ██████████
                            ●       ●

  table below (only the slices above, so it is short)
  session        harness   status   activity
  coordinator    claude    live     10:41
  oc-lane        opencode  idle     10:35

HOW THE PIECES WORK
-------------------
One bar per session. The left end is when it started, the right end is its
last activity or right now if it is still going.

Dots are messages. A user message is one color, an assistant message another,
a tool call another, a reasoning step another. Five kinds total. You can tell
at a glance whether a session spent its time answering or hammering tools.

The brush strip at the top is the whole ball of wax. You drag a window across
it. The waterfall below zooms to that window, and the table only lists
sessions that overlap the window. Drag the window to a quiet morning and the
busy afternoon sessions drop out of the list. That is how the table stays
small while you scroll back through hours of history.

A dot in the shop is a transcript message of one session. The same session
spans and dots are the same data the rest of the app already reads, just
projected onto time.

WHY THIS LIBRARY PICK
---------------------
Plain answer: for what we need (a drag-a-window brush and a time scale) the
canonical tiny building blocks exist, so we take them instead of writing
brushes and scales by hand. Two small packages. The bars and dots we draw
ourselves as svg because that is a thin ~80-line layout and none of the big
chart libraries is a fit.

The big devtools visualizer itself is too bolted onto Chrome's internals to
lift out as a component. We copy its look, not its code. The heavyweight
timeline libraries are either enormous or half-finished, and the ones that
only draw static charts have no brush gesture at all. So: two small blocks +
our own svg row.

WHAT CHANGES WHERE
------------------
Four spots plus a couple of tests:

- The types file learns the new shapes (session span, message dot, the brush
  window) and the checkbox field on the entry.
- A new pure file does all the math: build spans from sessions, turn a message
  into a dot, figure out which sessions and dots are inside the brush window.
  Pure and unit-tested, no screen involved.
- A new component draws the brush, the bars, the dots, and reuses the existing
  table for the constrained list.
- The strip file adds the checkbox and swaps to the waterfall when it is off.

Data comes straight out of what is already on disk and already read: session
start/end from the harness store rows, message dots from the per-session
transcript reader. Nothing new written to disk. The lazy rule holds: a
session's messages are only loaded if its bar is inside the window you are
looking at, never all of history at once.

THE ROAD (each step lands green on its own)
-------------------------------------------
1. Just the math module + tests. Nothing on screen changes.
2. The checkbox appears, checked, and remember its state. Still the same table
   when checked. Old tests still pass.
3. The waterfall draws (bars, dots, constrained table) with seeded fake data.
4. Dragging the brush filters the table and the dots live.
5. Polish: refresh on change, empty text, live. Full lint and build green.

DONE
----
A checkbox, default on, that flips the strip between "whats going on now" and
"the whole day as bars and dots you can brush over." Table stays small. Users
rule on the library call; the plan's pick is the small one.
