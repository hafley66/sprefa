# What is on your screen, as a table you can query

Plain version. No citations. Pictures.

## Contents

1. [The one-sentence version](#1-the-one-sentence-version)
2. [What happens today](#2-what-happens-today)
3. [What we want instead](#3-what-we-want-instead)
4. [The two tables](#4-the-two-tables)
5. [Why two tables and not one](#5-why-two-tables-and-not-one)
6. [The other way, and why we are not doing it](#6-the-other-way-and-why-we-are-not-doing-it)
7. [The one snag](#7-the-one-snag)
8. [Finding the diagrams](#8-finding-the-diagrams)
9. [What instant has to change](#9-what-instant-has-to-change)
10. [Numbers](#10-numbers)
11. [Things only you can decide](#11-things-only-you-can-decide)

---

## 1. The one-sentence version

instant already works out, sixty times a minute, exactly which messages are on
your screen and which of them contain a mermaid or d2 diagram, and then it
forgets. Give it a table to write that into and "is a diagram on screen right
now" becomes a one-line query.

---

## 2. What happens today

```mermaid
flowchart LR
  A["you scroll"] --> B["instant repaints"]
  B --> C["works out:<br/>top row, bottom row,<br/>which messages,<br/>which have diagrams"]
  C --> D["draws the diagrams"]
  C --> E["forgets everything else"]
  E --> F(["nothing on disk"])
  style E fill:#fee,stroke:#c33
  style F fill:#fee,stroke:#c33
```

The answer is computed. It lives in a local variable for one frame. Nobody can
ask it a question.

---

## 3. What we want instead

```mermaid
flowchart LR
  A["you scroll"] --> B["instant repaints"]
  B --> C["works out the same things"]
  C --> D["draws the diagrams"]
  C --> G["writes one row:<br/>surface, session,<br/>first message, last message"]
  G --> H[("boop.db")]
  H --> I["any query, any hook,<br/>any other agent"]
  style G fill:#efe,stroke:#3a3
  style H fill:#efe,stroke:#3a3
```

One extra row per scroll-stop. That is the whole change.

---

## 4. The two tables

```mermaid
flowchart TD
  V["agent_viewport<br/>ONE row per open window<br/>overwritten every time you scroll"]
  S["agent_viewport_span<br/>ONE row per place you STOPPED<br/>kept as history"]
  V -->|"held still 2 seconds?"| S
  V --> Q1["what am I looking at RIGHT NOW"]
  S --> Q2["what was I looking at<br/>when that lane crashed"]
```

The first table is a sticky note. The second is a diary.

---

## 5. Why two tables and not one

```mermaid
flowchart LR
  subgraph one["if only the sticky note"]
    A1["fast"] --> A2["cannot answer<br/>'were you watching<br/>when it died'"]
  end
  subgraph two["if only the diary"]
    B1["complete"] --> B2["every scroll tick<br/>is a row"]
    B2 --> B3["25,000 rows a day<br/>of you passing through"]
  end
  subgraph both["sticky note + diary"]
    C1["sticky note updates<br/>on every tick, free"] --> C2["diary only records<br/>where you STOPPED"]
    C2 --> C3["hundreds of rows a day"]
  end
  style both fill:#efe
```

The rule that makes it work: a row reaches the diary only if the view held still
for two seconds. Scrolling past something is not looking at it.

boop already uses this exact sticky-note-plus-diary pair for "which agents are
alive". Same shape, same closing logic, one piece of code serves both.

---

## 6. The other way, and why we are not doing it

The other way is: read the text off a tmux pane and try to work out which
messages produced it. That does not work, and the reasons are measured, not
guessed.

```mermaid
flowchart TD
  T["read the pane text"] --> O1
  O1["4 out of 5 stored messages<br/>have NO text at all<br/>(they are tool calls)"] --> X["cannot match"]
  O2["the screen shows a DRAWN table<br/>with box characters;<br/>the stored text is markdown pipes"] --> X
  O3["half the screen is not messages:<br/>'Cooked for 55s', the input box,<br/>the token counter"] --> X
  O4["text is wrapped to the pane width<br/>before you see it;<br/>un-wrapping means rewriting<br/>Claude Code's renderer"] --> X
  O5["long output is collapsed to<br/>'+42 lines' with no way back"] --> X
  style X fill:#fee,stroke:#c33
```

So: viewers that cooperate, yes. Guessing from pixels, no.

There is a middle rung worth knowing about. The harness itself could say "the
transcript is at message 340 and the user has not scrolled up". That is free and
exact for the common case of watching a lane run. It is a separate small job, not
part of this one.

```mermaid
flowchart LR
  R1["1. viewer tells us<br/>EXACT"] --> R2["2. harness tells us the bottom<br/>EXACT while following"]
  R2 --> R3["3. tmux tells us which pane<br/>SESSION only, no messages"]
  R3 --> R4["4. guess from pane text<br/>REJECTED"]
  style R1 fill:#efe
  style R4 fill:#fee
```

---

## 7. The one snag

boop and instant count messages differently, and neither is wrong.

```mermaid
flowchart TD
  J["one line in the transcript file<br/>= one assistant reply"]
  J --> B["boop counts it as 4 things:<br/>the text, then each tool call"]
  J --> I["instant counts it as 1 thing"]
  B --> X["so 'message 340' means<br/>two different messages"]
  I --> X
  style X fill:#fee,stroke:#c33
```

The fix: both sides already know the transcript's own record id. boop just does
not save it. Save it, and both sides have a name for the same thing.

```mermaid
flowchart LR
  I2["instant says:<br/>'top = record abc123<br/>bottom = record def456'"] --> B2["boop looks both up"]
  B2 --> R["writes the row<br/>in its own numbering"]
  style R fill:#efe
```

One extra column. Backfilling it means re-reading the transcripts once.

---

## 8. Finding the diagrams

Four ways to know a message contains a diagram. Three work, one is a trap.

```mermaid
flowchart TD
  Q["which messages have a diagram?"]
  Q --> A["search only the messages<br/>on screen right now"]
  Q --> B["search all 197,000 messages<br/>every time you ask"]
  Q --> C["mark each message once,<br/>when it arrives"]
  Q --> D["build a full-text search index"]
  A --> A2["instant. costs nothing.<br/>answers the on-screen question"]
  B --> B2["a tenth of a second.<br/>fine for a report,<br/>too slow to do constantly"]
  C --> C2["a tenth of a second ONCE.<br/>answers everything after."]
  D --> D2["adds 57 MB to the database<br/>AND still needs a second check<br/>because it matches the word<br/>'mermaid' in ordinary prose"]
  style A2 fill:#efe
  style C2 fill:#efe
  style D2 fill:#fee,stroke:#c33
```

Ship the first and the third. The full-text index is a good idea for a different
feature and a bad trade for this one.

---

## 9. What instant has to change

One function. The one that draws the diagram overlays.

```mermaid
flowchart TD
  P["the repaint function"] --> H1["it already knows the top row"]
  P --> H2["it already knows the bottom row"]
  P --> H3["it already knows every message"]
  P --> H4["it already knows which<br/>diagrams are visible"]
  H1 & H2 & H3 & H4 --> M["MISSING: which message<br/>is at the top and bottom<br/>when there is no diagram"]
  M --> F["the fix already exists elsewhere<br/>in the same app: right-clicking<br/>a message finds its exact<br/>first and last screen row,<br/>then throws those away"]
  F --> W["reuse that, write the row"]
  style W fill:#efe
```

instant already opens SQLite databases for two other things, so writing the row
is one more small function. No new server, no new process.

There is a bonus second place: the session sidebar tree already knows its exact
visible range with no guessing at all, because it is a proper virtual list. That
becomes a second row in the same table, describing a second surface.

---

## 10. Numbers

| thing | measured today |
|---|---|
| messages stored | 197,662 |
| of those, with any text at all | 38,857 (one in five) |
| messages containing a mermaid diagram | 169, across 56 sessions |
| messages containing a d2 diagram | 42, across 31 sessions |
| checking the whole history for diagrams | about a tenth of a second |
| checking only what is on screen | too fast to measure |
| marking every message once, up front | about a tenth of a second |
| full-text index instead | 57 MB of extra disk, still not exact |
| writing one viewport row | half of one thousandth of a second |
| rows the diary would collect per day | hundreds, once the two-second rule is on |

---

## 11. Things only you can decide

```mermaid
flowchart TD
  Q1["save the transcript record id<br/>on every message?"] --> A1["needed so instant and boop<br/>can name the same message.<br/>costs one column and<br/>one re-read of the transcripts"]
  Q2["turn on WAL mode<br/>for boop.db?"] --> A2["needed before a second program<br/>writes to it. otherwise every<br/>write locks the whole file"]
  Q3["how long is 'stopped'?"] --> A3["two seconds proposed.<br/>this is the knob between<br/>'where you looked' and<br/>'where you scrolled past'"]
  Q4["one row per window,<br/>or one per app?"] --> A4["per window proposed,<br/>so the terminal and the sidebar<br/>are two separate rows"]
  Q5["which content kinds<br/>get marked?"] --> A5["mermaid and d2 now.<br/>code, sql, tables cost<br/>nothing extra to add"]
```
</content>
</invoke>
