# 6. Gold standards, and your engine

**The question:** the two systems everyone cites for code-facts-at-scale are
Glean and Zoekt. What did each actually figure out, and how much of it have you
already built?

## Zoekt: trigram index + mmap

Zoekt answers "find this text/regex fast across a huge corpus" without ever
scanning the corpus.

```
index every 3-char sequence (trigram) -> posting list of which docs contain it
query "hello"  decompose to  "hel" AND "ell" AND "llo"
   intersect those posting lists -> a few candidate docs
   run the real regex only on candidates
```

It prunes to candidates, then the bytes live in **mmap'd shards** so the index
can be larger than RAM and the OS pages it; only offset tables stay resident.
That is Chapter 5's discipline taken to its sharpest form. Zoekt is a *search*
engine, not a fact/graph engine, so it is adjacent to you: borrow the storage
seam (mmap a big on-disk structure, keep offsets resident), not the model.

## Glean: facts + ownership + stacked DBs

Glean is your architecture at monorepo scale. Three ideas:

```
1. typed, deduplicated facts referencing each other by integer id (a fact DAG),
   stored in RocksDB. = your `def`/`calls`/`reaches` rows, interned, on disk.

2. ownership sets: every fact is tagged with the unit (file) that produced it,
   stored compactly. derived facts inherit ownership from their inputs.
   = your `_prov(rel, path)`, extended through the derivation graph.

3. incremental by STACKING immutable DBs: to update a file, add a new layer that
   adds the new facts and hides the changed file's old facts. update cost is
   O(changed files), not O(corpus).
```

You have idea 1 (facts in SQLite) and the source half of idea 2 (`_prov` tags
each source fact with its file). The two pieces Glean has that you do not, yet:
ownership propagated to *derived* facts (Chapter 4), and non-destructive stacking
(you delete-and-reinsert instead, which is fine for a single embedded store).

## How `dl` is every chapter of this book

```
Chapter 1  facts + rules        rel def/call (source) ; calls/reaches/unused (derived)
Chapter 2  recursion/fixpoint   the two reaches rules ; loop INSERT until 0 new rows
Chapter 3  cycles               INSERT OR IGNORE halts the cycle ; SCC = the next step
Chapter 4  incremental          _prov file-keyed source retraction ; content-hash + mtime
                                ; derived = wipe+recompute (today) -> ownership (next)
Chapter 5  where bytes live     facts in SQLite on disk ; one file's parse resident ; ~133 MB
Chapter 6  gold standards       same shape as Glean, minus a Haskell server
```

You did not copy Glean. You rebuilt its source-layer half from first principles,
because it is the natural shape once you take "facts on disk, retract by file"
seriously. That convergence is the signal you are on a real path, not a novel
one that needs justifying.

## The one next step

Everything points at the same single move:

> Make `reaches` SCC-condensed and give derived facts ownership tags, so a file
> edit retracts exactly the affected derived facts at bounded memory, instead of
> recomputing the whole derived layer.

That turns "rebuild the closure every tick" into "touch one SCC," keeps RAM bound
to the working set, and is the difference between your engine and Glean collapsed
to one feature. The research doc has the schema.

## Intuition

> Zoekt = inverted index + mmap (a storage trick to borrow). Glean = typed facts
> + ownership + stacked incremental (your architecture at scale). You already
> built the source half of Glean; the one feature left is ownership-propagated,
> SCC-condensed derived facts. You are not behind these systems conceptually; you
> are a feature away, on a smaller, embeddable, bounded-RAM footing they do not
> have.

## Exercises

1. Why is Zoekt's model adjacent to yours rather than the same? What is the one
   thing worth taking from it?
2. Name the three Glean ideas and which you already have.
3. In one sentence each, say what every chapter of this book maps to in `dl`.
4. State the single next feature and what it changes about the reactive loop.

## Where to go from here

You have, in this repo: a working engine (`v5/src`), examples that run on the
real kernel, a benchmark harness, a research doc on the incremental-recursion
frontier, and this book. The honest next actions, smallest first: (1) read your
own `engine.rs` end to end now that you have the vocabulary; (2) implement the
SCC-condensed `reaches`; (3) only then, if a real corpus makes wholesale
recompute slow, add ownership-propagated derived retraction. Stop at (1) for a
while if you want. The point of the book is that you can now read any system in
this space and name which of these few ideas it picked.

## Answers

1. Zoekt searches text/regex; you store and query facts/relations. Different
   model. Take the storage seam: mmap a large on-disk index, keep only offsets
   resident (you get a version of this free from SQLite's page cache / mmap).
2. Typed deduped facts (have it: SQLite rows), ownership tags (have the source
   half: `_prov`), stacked immutable incremental (do not have; you delete-insert
   instead, fine for embedded).
3. ch1 = your rel decls + rules; ch2 = the reaches fixpoint loop; ch3 = cycles,
   handled by INSERT OR IGNORE, next by SCC; ch4 = `_prov` + content-hash/mtime
   incremental; ch5 = facts in SQLite, working set resident; ch6 = the same shape
   as Glean.
4. Next feature: SCC-condensed `reaches` + ownership-tagged derived facts. It
   changes the derived step of the reactive loop from "wipe and recompute the
   whole closure" to "retract only the facts owned by the changed file and
   re-derive within the affected SCC."
