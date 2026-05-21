# 4. Incremental maintenance

**The question:** you edit one file. Recomputing every fact from scratch is
correct but wasteful. How do you update only what changed, and why is deleting
harder than adding?

## Inserts are easy, deletes are hard

Adding facts is monotone: new facts can only derive more facts, never underive
old ones. So an insert is just "run semi-naive starting from the new facts as the
frontier." Chapter 2 already did this.

Deleting is the hard direction, because removing one fact may or may not remove
things it helped derive — *another* derivation might still hold it up. Three
classic answers, in order of how much they respect that:

```
COUNTING   per fact, store #derivations; delete decrements; 0 ⇒ remove.
           cheap, but WRONG under recursion (Chapter 3). sound only on a DAG.

DRed       (delete-rederive) over-delete everything transitively derived from the
           deleted fact, then re-derive whatever still has a surviving support.
           correct under recursion, but the over-delete is pessimistic: it can
           delete then immediately re-derive huge sets.

B/F        (Motik backward/forward) before deleting a fact, search backward for
           an alternative derivation from surviving facts; only delete if none
           exists. exact (no over-delete), correct on cycles, but as published it
           keeps the whole materialization resident.
```

The lesson is not "pick the fanciest." It is that **exactness costs CPU and
deletion is where recursion bites**, so the cheapest correct design avoids
incremental deletion of the recursive layer entirely where it can.

## The split that makes it tractable: source vs derived

Recall Chapter 1's split. It is the whole game for incrementality:

```
SOURCE facts  (def, calls)   each has a home file  → retract by file, exactly
DERIVED facts (reaches, ...)  no home file          → recompute or invalidate
```

**Source retraction is trivial and exact** because every source fact knows its
file. Edit `b.rs`, and you delete exactly the source facts tagged `b.rs` and
re-extract them. No counting, no DRed, because a source fact has exactly one
support: its file.

```
edit b.rs:
   delete all source facts WHERE file = 'b.rs'   ← surgical, by the home tag
   re-extract b.rs                                ← add the new ones
   other files' facts: untouched
```

**Derived facts have no home**, so you cannot retract them by file. Two honest
options:

1. **Recompute the derived layer wholesale.** Wipe `reaches`/`unused`/etc.,
   re-run the rules over the (now-updated) source facts. Trivial to write,
   always correct, and cheap *if the rules are cheap* (joins over already-stored
   facts are milliseconds). This is what your `dl` does today, and it is a
   legitimate stopping point.
2. **Propagate ownership to derived facts** (the Glean idea). Tag each derived
   fact with the set of files it was derived from (the union/intersection of its
   inputs' tags), so a file change can retract exactly the derived facts that
   depended on it. This is the surgical version, and it needs Chapter 3's SCC
   trick to stay correct and cheap on the recursive `reaches`.

## Ownership: the principled "home" for derived facts

A derived fact's home is the set of source files that fed its derivation:

```
calls("main","run")  derived from  def(main)@a.rs, call(run)@a.rs   ⇒ owned by {a.rs}
reaches("main","lex") derived from edges in a.rs, b.rs, c.rs        ⇒ owned by {a.rs,b.rs,c.rs}
```

Now editing `c.rs` can find and drop exactly the derived facts whose ownership
set contains `c.rs`, then re-derive them. Update cost is proportional to what the
changed file touched, not to the whole program. This is Glean's mechanism, and it
is the upgrade path from option 1 to option 2.

## The reactive loop, end to end

Tie it to a file watcher:

```
file save (b.rs)
   │  watcher names the changed path
   ▼
SOURCE: delete facts WHERE file='b.rs';  re-extract b.rs       ← O(one file)
   │
   ▼
DERIVED: either wipe+recompute (cheap rules)                   ← option 1, today
         or retract facts owned-by b.rs + re-derive            ← option 2, next
   │
   ▼
publish (diagnostics / query results)
```

The two performance levers you already built sit on the source line: **content
hashing** (skip files whose bytes did not change) and the **mtime fast-path**
(skip even hashing files whose mtime did not move). They make the common case
"nothing changed" nearly free, and "one file changed" cost one file.

## Intuition

> Inserts are monotone and easy; deletes are hard because a fact may have other
> support. The trick is the source/derived split: source facts retract exactly by
> their home file (one support each), derived facts are either recomputed
> wholesale (cheap if rules are cheap) or retracted by propagated ownership (the
> surgical, Glean-style upgrade). Counting/DRed/B-F only matter once you
> incrementally delete the recursive layer.

## Exercises

1. Edit `b.rs` so `run` no longer calls `log` (`fn run(){ parse(); }`). Which
   source facts are deleted? Which derived `reaches` facts should disappear?
2. Why can you retract `calls("run","log")` exactly by file, but not
   `reaches("main","log")`?
3. Give the ownership set of `reaches("main","lex")` in the original example.
   Which file edits should invalidate it?
4. When is "wipe and recompute the derived layer" the right choice, and when does
   it stop being right?

## In your engine

You have the source half fully: `_prov(rel, path)` is the home tag, your
reconcile deletes a changed file's facts and re-extracts, and content-hash +
mtime make unchanged files free. You currently take option 1 for derived (wipe +
re-run the fixpoint). The research doc's recommended next step is option 2 for
the recursive layer: ownership-propagated derived facts + SCC-condensed `reaches`
so a file edit retracts only the affected derived facts at bounded memory.

## Answers

1. Delete source fact `calls("run","log")` (and re-extract b.rs, which no longer
   produces it). Derived `reaches` facts that vanish: `reaches(run,log)`,
   `reaches(main,log)`, `reaches(parse,log)`, `reaches(lex,log)` — every reach
   into log, since log was only reachable via run→log. log becomes unreachable
   (and unused).
2. `calls("run","log")` is a source fact with home `b.rs`; deleting by file is
   exact. `reaches("main","log")` is derived from a *chain* of edges across
   files; it has no single home, so deleting it requires knowing whether any
   other derivation survives (the hard problem) — unless you stored its ownership
   set.
3. `reaches("main","lex")` came from main→run (a.rs), run→parse (b.rs),
   parse→lex (c.rs). Ownership = {a.rs, b.rs, c.rs}. Editing any of those three
   should invalidate it.
4. Right when the derived rules are cheap relative to extraction (single repo:
   joins over stored facts are ms, extraction/parsing dominates). It stops being
   right when the corpus is large enough that re-running the full fixpoint each
   tick dominates — then you move to ownership-propagated incremental (option 2).
