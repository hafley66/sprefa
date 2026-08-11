# Reference docs from comment nodes

## TOC

1. Goal
2. Three inputs, one join
3. Attachment fork
4. Sources, prolog included
5. The join in rules
6. Render
7. Staleness gate
8. Open questions

## 1. Goal

One dl6 program turns repo comments into per-construct reference pages.

Constructs come from the registry. Prose comes from comment nodes. Examples come from the golden file.

Output: markdown, written by a host, checked in.

No code lands today. This plan reads.

## 2. Three inputs, one join

```text
surface/5 registry rows      comment nodes            golden examples
(name, arity, axis, status)  (path, line, kind, text)  (worked code)
        |                        |                           |
        +----- key: construct name/arity ------+              |
        |                                     |              |
        |            grammar witness join     |              |
        +-----------> doc note <--------------+              |
        |                 |                                 |
        |                 +----------- ref_row ------------+
        |                     (prose + example)
        v
    render plane
        |
        v
    reference markdown (host writes the file)
```

The key that joins all three is the construct signature: name plus arity.

## 3. Attachment fork

Three ways to bind a comment to a construct:

```text
preceding-decl    comments sit above the declaration
section-header     ==== headers group a domain
marker             a docs: tag names the construct in the comment
```

The lab inventory already gives marker capture.

A marker is the only one that names the construct, so it is the only one that can join to the registry by name.

Preceding-decl is weak here: the golden file often explains a construct in its header, far from the citing rule. It also re-opens the trailing-comment ambiguity.

Section headers are a grouping wrapper, not an attachment.

Recommendation: a docs: marker in the comment, captured by a per-marker host, joined to the comment node for the grammar witness.

```text
USER FORK (not decided):
  add docs: markers to the golden file (exact, edits the golden)
  or
  match the existing loose prose signatures in a host (no edit, tolerates drift)
```

## 4. Sources, prolog included

```text
dl6 # comments   proven (fixtures)
Rust ///         proven (745/745 parity)
Prolog %         extractable with the existing extractor
Markdown         the one hole, not needed here
```

Prolog is in scope for phase 1.

The extractor has a prolog source using tree-sitter-prolog, and it emits comment nodes through the cst family. The corpus that proved the machinery was prolog itself.

A14, the open trailing-comment question: does a comment span include a trailing comment on a code line.

```text
code(); // note     does this trailing note belong to the span?
```

This rail does NOT force that decision. Doc comments are leading in every source it reads. A trailing comment never changes which comment documents a construct. So A14 stays open.

## 5. The join in rules

Read the registry into rows (same host self-map uses):

```text
sh sm_surface(path, digest) -> (record, functor, arity, axis, status) = ...
rel construct(functor, arity, axis, status)
    <- source(path, digest), sm_surface(path, digest, 'surface', ...).
```

Read comments into rows:

```text
sh comment_fact(path, digest) -> (line, kind, text) = ...
rel comment_node(path, digest, line, kind, text)
    <- file(path, digest), comment_fact(...).
```

Read markers into rows:

```text
sh doc_marker(path, digest) -> (line, name, arity, doc_text) = ...
rel doc_note(path, line, name, arity, doc_text)
    <- doc_marker(...), comment_node(...).
```

The witness join is the safety gate: a marker counts only where the parser agrees a comment covers that line.

The decision row:

```text
rel doc_source(name, arity, axis, status, doc_text)
    <- construct(name, arity, axis, status),
       doc_note(path, line, name, arity, doc_text).
```

Attach the example:

```text
rel ref_row(name, arity, axis, status, prose, example)
    <- doc_source(name, arity, axis, status, prose),
       golden_cst(path, example_line, example),
       section_covers(path, example_line, name).
```

Every construct named here is a live registry row.

## 6. Render

The closest rail to copy is devlog.

```text
rows -> devlog_line(ordinal, line) -> group_concat fold -> write host writes DEVLOG.md
```

self-map adds what prose reference docs want: per-section folds and a checked-in write host.

```text
rel ref_line(section, ordinal, line_text)
rel section_text(section, group_concat(line_text, '\n', ordinal))
rel ref_doc(document) <- section_text('registry', body),
    document := concat([body, '\n']).
```

## 7. Staleness gate

Generated docs become a checked-in artifact with a working rail.

```text
run the one-shot
      |
      v
git diff --exit-code on the doc
      |
      +-- clean          fresh
      +-- non-zero       stale, regenerate
```

Two precedents:

```text
SYNTAX.md marker-section    only the region between markers regen; reader prose stays
self-map whole-file         the whole doc is generated; byte-stable; sabotage receipts
```

Use the whole-file self-map pattern. Add sabotage: a fake registry row or a renamed golden construct must change the doc, then the revert must return it byte-identical.

## 8. Open questions

```text
docs: marker convention   where markers live (fork, user)
A14 trailing span         not forced; stays open
section_covers            spelling depends on the attachment fork
```

State: read-only recon. No code, no fixture edits, no fork decisions.
