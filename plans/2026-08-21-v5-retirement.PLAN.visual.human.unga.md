# Retiring v5, in plain words

No file paths, no line numbers. The version with receipts sits beside this one.

## Contents

- [The short version](#the-short-version)
- [Phase 0: what is actually holding v5 alive](#phase-0-what-is-actually-holding-v5-alive)
- [Phase 1: cut the last cord](#phase-1-cut-the-last-cord)
- [Phase 2: sweep the corpses](#phase-2-sweep-the-corpses)
- [Phase 3: the one thing that needs your word](#phase-3-the-one-thing-that-needs-your-word)
- [Phase 4: delete](#phase-4-delete)
- [The other story: the old rails](#the-other-story-the-old-rails)
- [What you are choosing to lose](#what-you-are-choosing-to-lose)
- [Your one next move](#your-one-next-move)

## The short version

You asked why usurping v5 is taking so long. It is not taking long any more.
One automated test still runs the old engine. Everything else that reaches it is
already broken, already marked as expected-to-fail, or a shortcut in a file
nobody runs.

The new extractor is not behind the old one. It pulls out MORE facts from source
code than v5 ever did. What is behind is the doorway: a program written in the
new language can only ask the extractor for about half of what the extractor
knows. That is wiring, and it is small.

## Phase 0: what is actually holding v5 alive

Four things were believed to depend on the old engine. Three of them stopped
working weeks ago and nobody noticed.

```mermaid
flowchart TD
    V5[the old engine binary]
    CI[the CI test suite] -->|removed 10 days ago| X1[nothing]
    LSP[editor squiggles bridge] -->|its new-side server was deleted| X2[already broken]
    FLAG[the headline v5-vs-v6 comparison] -->|its saved answer went stale| X3[marked expected-to-fail]
    CRAWL[the multi-repo crawl test] -->|hard-fails without it| V5
    X1 -.-> V5
    X2 -.-> V5
    X3 -.-> V5
    style CRAWL fill:#c33,color:#fff
    style V5 fill:#333,color:#fff
```

Only the red box is real. One test.

## Phase 1: cut the last cord

That test already has the old engine's answer saved in the repository as a
file. It does not need to re-run the old engine to compare against it; it just
insists on finding the binary first.

```mermaid
flowchart LR
    subgraph today
      A1[test starts] --> A2[look for the old binary]
      A2 -->|missing| A3[FAIL]
      A2 -->|found| A4[read the saved answer]
      A4 --> A5[compare with the new engine]
    end
    subgraph after
      B1[test starts] --> B2[read the saved answer]
      B2 --> B3[check its fingerprint]
      B3 --> B4[compare with the new engine]
    end
```

Delete the "look for the old binary" step, add a fingerprint check on the saved
file so it cannot silently drift. That is the whole change, and the day it lands
is the day the old engine stops building.

## Phase 2: sweep the corpses

Three scripts and one bridge that already do not work. Deleting them is not a
risk; it is admitting what is true.

```mermaid
flowchart TD
    S[sweep] --> S1[the headline comparison test<br/>and its expected-to-fail entry]
    S --> S2[the editor squiggles bridge<br/>its other half no longer exists]
    S --> S3[three comparison scripts<br/>on the paused TypeScript path]
    S --> S4[one staleness check<br/>that watches the old binary]
```

After this the word "v5" appears in the new tree only in prose.

## Phase 3: the one thing that needs your word

There is a shortcuts file at the top of the repository with twelve one-line
commands that run old rails by hand. No test uses it. No CI job uses it. It is
there for a human who wants to poke at something.

```mermaid
flowchart LR
    Q{do you ever type<br/>these twelve commands?}
    Q -->|no| D[delete the file with the rest]
    Q -->|sometimes| K[keep it, and keep the old engine<br/>building just for it]
    style Q fill:#446,color:#fff
```

This is the only decision in the whole plan.

## Phase 4: delete

Once phases 1 to 3 are done, the old engine moves to the archive folder beside
the other two archived versions. It is a large move and a boring one.

```mermaid
flowchart LR
    direction LR
    R[repository root] --> E[engine source<br/>106k lines]
    R --> T[its test suite<br/>58k lines]
    R --> L[195 old rail programs]
    R --> V[vendored grammars]
    R --> X[the vscode extension]
    R --> P[the release pipeline]
    E --> AR[archive folder]
    T --> AR
    L --> AR
    V --> AR
    X --> AR
    P --> AR
    style AR fill:#363,color:#fff
```

Then four helper definitions that teach an assistant how to add features to the
old engine get deleted too, so nobody is handed a checklist for a thing that no
longer exists.

## The other story: the old rails

There are 195 little programs written in the old language: lint rails, graph
reports, code generators. They are a separate question from the binary, and they
do not hold the retirement up.

```mermaid
pie showData
    title 195 old rail programs
    "blocked on a doorway gap" : 110
    "would work today, needs a rewrite" : 68
    "already have a new twin" : 5
    "dead, nothing names them" : 12
```

That chart moved 33 slices while this census was being written. A doorway that
lets a program search source code by shape, and read the text it found, landed
on the main branch. It is the single thing 33 of those rails were waiting for,
and it is why both of the two rails you named as live now ship as ports here
rather than one shipping and one being filed as impossible.

The blocked ones are not blocked by anything missing in the extractor. They are
blocked by the doorway. Two gaps account for almost all of them:

```mermaid
flowchart TD
    G0[search source by shape<br/>and read what you found] --> R0[33 rails freed]
    G1[write a file from a program] --> R1[25 want it<br/>11 need nothing else]
    G2[ask for cross-file<br/>resolved links] --> R2[66 want it<br/>0 need nothing else]
    G3[ask for import graphs<br/>or control flow] --> R3[15 want it]
    G4[the older tree-sitter<br/>query form] --> R4[11 want it<br/>4 need nothing else]
    R0 --> D[DONE, landed 21 August]
    R1 --> W[fix the rest and 81<br/>more rails go green]
    R2 --> W
    R3 --> W
    R4 --> W
    style D fill:#363,color:#fff
```

Writing a file is now the cheapest buy: 11 rails need nothing else, and the card
for it is already open. The resolved-links gap is the biggest group but frees
nothing on its own, because every rail that wants one resolved link wants two.

The extractor knows every one of these things. Its command-line tool will print
them for you today. The engine just does not know how to ask.

## What you are choosing to lose

Seven capabilities the old engine had that the new one will not, listed so
nobody re-discovers them as bugs in six months.

| capability | last used |
|---|---|
| code similarity by embedding, and graph embeddings | 2026-07-02 |
| suggested refactorings (extract this, clone that) | 2026-07-09 |
| type-shape generalization | 2026-07-09 |
| the drawable flow panel and its graph sinks | 2026-07-20 |
| reading the assistant's own edit trail as facts | 2026-07-09 |
| the engine describing its own relations to itself | 2026-07-20 |
| who first wrote each file | 2026-07-01 |

Nothing in the last seven weeks has asked for any of them.

## Your one next move

Answer the Phase 3 question: do you ever type the twelve shortcut commands at
the top of the repository?

Yes, and the old engine keeps building for you alone. No, and it stops building
the day one small test change lands.
