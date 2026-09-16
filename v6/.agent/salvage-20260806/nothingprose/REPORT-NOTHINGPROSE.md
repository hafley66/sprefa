# REPORT-NOTHINGPROSE

Measurement of the "nothing sentence" form over the local Claude session
transcript corpus, computed by a dl6 program in `v6/labs/prose_nothing/`.

## TOC

- [1. The form and the classes](#1-the-form-and-the-classes)
- [2. The dl6 program](#2-the-dl6-program)
- [3. Validation: 6-sentence fixture](#3-validation-6-sentence-fixture)
- [4. Full corpus results](#4-full-corpus-results)
- [5. Top pure_nothing offenders](#5-top-pure_nothing-offenders)
- [6. Deviations](#6-deviations)
- [7. Artifacts and how to rerun](#7-artifacts-and-how-to-rerun)

## 1. The form and the classes

Mechanical definition, two halves plus an exemption.

| rule | regexp (case covered by a dual-case alternation, dl6 has no `(?i)` flag) |
|---|---|
| HALF 1 evaluative copula | `\b(is\|are\|was\|were\|looks?\|seems?\|remains?\|Is\|...)\s+(genuine\|real\|solid\|correct\|clean\|right\|fine\|good\|proper\|sound\|legit\|robust\|reasonable\|sensible\|Genuine\|...)\b` |
| HALF 2 strawman contrast | `\b(rather than\|as opposed to\|instead of being\|not merely\|not just\|Rather than\|...)\b` |
| EXEMPTION receipt | a digit, a hex run of 8+, a `.pl`/`.rs`/`.ts`/`.md` fragment, a colon-number, a percent sign, or a quoted span |

Derived classes (all inside the dl6 program, hosts feed raw sentences only):

| class | predicate |
|---|---|
| `evaluative_no_receipt` | HALF 1, no receipt |
| `strawman_contrast` | HALF 2, no receipt |
| `pure_nothing` | HALF 1 AND HALF 2, no receipt. The flagged shape. |

The three classes are independent, so the first two include the `pure_nothing`
subset. Counts are per (class, side), expressed as a rate per 10k sentences of
that side.

## 2. The dl6 program

`v6/labs/prose_nothing/prose-nothing.dl6`. The feed host holds the raw-facts
boundary; every regexp, negation and aggregate is a rule. Each snippet below
carries its pure-rxjs lowering in one line.

The sh feed host. `PROSE_NOTHING_FEED` and `PROSE_NOTHING_MODE` come from
`run.sh`; the third argument is the probe token, a chunk index in corpus mode.

```flang
sh feed_sentences(start_token: text) -> (side: text, seq: int, sentence: text) =
  `: {start_token}; "$PROSE_NOTHING_FEED" "$PROSE_NOTHING_MODE" {start_token}`.
```

rxjs lowering (the effect seam in `serve/1_hosts.ts`):

```
liveDemand$(plans) -> spawn(fillTemplate, {shell:true}).stdout -> decodeObjectItems -> project -> engine.submit(arrivals)
```

The sentence rows, joined from the probe:

```flang
sentence_row(side, seq, sentence) <-
  probe_row(start_token),
  feed_sentences(start_token, side, seq, sentence).
```

rxjs lowering:

```
probe_row$.pipe(concatMap(([start_token]) => feed$.pipe(filter(sel by start_token), map(([side, seq, sentence]) => [side, seq, sentence]))))
```

The receipt exemption:

```flang
rel receipt_bearing(side: text, seq: int).
receipt_bearing(side, seq) <-
  sentence_row(side, seq, sentence),
  regexp(sentence,
    "[0-9]|[0-9a-fA-F]{8,}|\\.(pl|rs|ts|md)\\b|:[0-9]+|%|[\"”“‘’]").
```

rxjs lowering:

```
sentence_row$.pipe(filter(([, , sentence]) => RECEIPT.test(sentence)), map(([side, seq]) => [side, seq]))
```

Half 1 and half 2:

```flang
rel evaluative(side: text, seq: int).
evaluative(side, seq) <-
  sentence_row(side, seq, sentence),
  regexp(sentence, "... dual-case evaluative alternation ...").

rel strawman(side: text, seq: int).
strawman(side, seq) <-
  sentence_row(side, seq, sentence),
  regexp(sentence, "... dual-case strawman alternation ...").
```

rxjs lowering:

```
sentence_row$.pipe(filter(([, , sentence]) => EVALUATIVE.test(sentence)), map(([side, seq]) => [side, seq]))
sentence_row$.pipe(filter(([, , sentence]) => STRAWMAN.test(sentence)), map(([side, seq]) => [side, seq]))
```

The flagged class, a join minus an antijoin:

```flang
rel pure_nothing(side: text, seq: int).
pure_nothing(side, seq) <-
  evaluative(side, seq),
  strawman(side, seq),
  not(receipt_bearing(side, seq)).
```

rxjs lowering:

```
combineJoin(evaluative$, strawman$, key by (side, seq)).pipe(filter(rows not in receiptSet), map(([side, seq]) => [side, seq]))
```

The count aggregate, one per class:

```flang
rel pure_nothing_count(side: text, sentence_count: int).
pure_nothing_count(side, count(seq)) <-
  pure_nothing(side, seq).
```

rxjs lowering:

```
pure_nothing$.pipe(groupBy(r => r[0]), mergeMap(g => g.pipe(count(), map(n => [g.key, n]))))
```

## 3. Validation: 6-sentence fixture

`fixture-sentences.json`, run through `run.sh fixture`. Expected counts
(overlapping classes, so sentence 1 counts in all three):

| # | side | sentence | evaluative_no_receipt | strawman_contrast | pure_nothing |
|---|---|---|---|---|---|
| 1 | assistant | `xrxgraph's trait-object indirection is genuine rather than faked.` | yes | yes | yes |
| 2 | assistant | `The proposed design is solid.` | yes | no | no |
| 3 | assistant | `We should prefer the explicit route not merely the convenient one.` | no | yes | no |
| 4 | user | `The fix is good in 12 lines.` (receipt: digit) | no | no | no |
| 5 | user | `The server boots and listens on a port.` | no | no | no |
| 6 | user | `The overall approach seems sensible if we keep it minimal.` | yes | no | no |

Expected per-side tallies: `evaluative_no_receipt` assistant 2 / user 1,
`strawman_contrast` assistant 2 / user 0, `pure_nothing` assistant 1 / user 0.
The receipt-bearing sentence 4 is excluded from every class.

Verbatim `run.sh fixture` output:

```text
posting 1 probe tokens
== PROSE-NOTHING (mode=fixture) ==
run time: 1633 ms
sentence sides: {'assistant': 3, 'user': 3}

CLASS evaluative_no_receipt:
  assistant         2   rate 6666.67 per 10k
  user              1   rate 3333.33 per 10k
  TOTAL             3

CLASS strawman_contrast:
  assistant         2   rate 6666.67 per 10k
  user              0   rate 0.00 per 10k
  TOTAL             2

CLASS pure_nothing:
  assistant         1   rate 3333.33 per 10k
  user              0   rate 0.00 per 10k
  TOTAL             1
```

The measured output matches the expected tallies.

## 4. Full corpus results

`run.sh corpus` over `~/.claude/projects/*/*.jsonl`. The feed emitted 84,445
sentences (assistant 63,222, user 21,223). Verbatim output:

```text
posting 17 probe tokens
== PROSE-NOTHING (mode=corpus) ==
run time: 51553 ms
sentence sides: {'assistant': 63222, 'user': 21223}

CLASS evaluative_no_receipt:
  assistant       421   rate 66.59 per 10k
  user            146   rate 68.79 per 10k
  TOTAL           567

CLASS strawman_contrast:
  assistant       517   rate 81.78 per 10k
  user             50   rate 23.56 per 10k
  TOTAL           567

CLASS pure_nothing:
  assistant         9   rate 1.42 per 10k
  user              0   rate 0.00 per 10k
  TOTAL             9
```

Measured table:

| class | assistant | user | rate per 10k (assistant) | rate per 10k (user) |
|---|---|---|---|---|
| sentences | 63,222 | 21,223 | - | - |
| evaluative_no_receipt | 421 | 146 | 66.59 | 68.79 |
| strawman_contrast | 517 | 50 | 81.78 | 23.56 |
| pure_nothing | 9 | 0 | 1.42 | 0.00 |

## 5. Top pure_nothing offenders

All 9 `pure_nothing` rows are assistant-side; there are no user-side rows, so
the "top 15" is the full set.

```text
seq   1118  The unifying trait should take the Lobby shape, with purity as a per-implementor contract rather than a trait requirement: Out-param over tuple-return is right for the hot loop (Vec reuse, zero alloc on no-effect events); is compatible with rollback because snapshots are whole-state assignments ( ), the same reason the ggrs handler works today.
seq   7996  And the tooling for spinning two fresh worktrees off a *branch* (rather than HEAD) is friction that buys little for background work - B ( ) and C ( ) are disjoint, but running them sequentially in the Spine worktree is correct and conflict-free.
seq   8613  So: the foundation is textbook-grade for a rollback platform fighter, the animation risk is real but has a known shape that fits your existing doctrine, and the design skill you're worried about is acquired through the playtest cadence you've already established rather than through more architecture.
seq   8616  Your model is right with one refinement: there is no diff step, so it's solid rather than react.
seq  23783  The agent reported it as observed rather than reconciling it to the expected number, which is right, but nobody has explained the growth.
seq  24684  RED/GREEN tests are real, not rigged - and crucially the GREEN test plants a key *inside* and asserts silence, proving the test-span exclusion does actual work rather than riding on absent delimiters.
seq  62534  Restating the two flagged findings without the banned stem: xrxgraph's trait-object indirection is genuine rather than faked (the dispatch really goes through ), and the three engine lanes' empty deviation sections held up under checking, with mdquery's STOP-instead-of-improvise remaining the standout behavior.
seq  65819  Notable: its uncommitted changes are in **src/** production files ( , , , , , , ) - those are real consumer off-ramps (LSP, SCIP, effect readers that pull interned columns and need decoding), not just test edits.
seq  71613  Your instinct was right that this should already exist, and the reason it didn't is a specific, fixable mistake rather than a missing feature.
```

## 6. Deviations

- **Sentence splitter is period/`!`/`?` only.** Clauses joined by `;`, `:`, `-`
  and `and` stay one extracted sentence. Several flagged rows (1118, 7996,
  8613, 23783, 71613) pair "is right"/"is correct"/"are real" from one clause
  with "rather than"/"not just" from another. Those are compound-split
  artifacts, not intents. A clause-aware splitter would cut the count.
- **`is right` and `are real` are the common evaluative fillers.** The adjective
  list includes `right`, `real`, `correct`, which occur in ordinary assertion
  ("your model is right", "tests are real") and pair with a nearby contrast
  phrase to trip the joint rule.
- **Genuine hits are visible.** seq 62534 is the exact flagged sentence ("is
  genuine rather than faked"), restated verbatim in a later session. seq 8616
  ("solid rather than react") and seq 23783 ("right rather than") are the
  intended nothing-shape.
- **Receipt exemption is broad.** A single digit exempts a sentence, which is
  why none of the corpus rows carry numbers. The flagged sentence carries no
  digit, so it stays in.
- **Chunked corpus ingest.** The served engine OOMs on a single ~20k-row
  arrival batch (macOS SIGKILL), so `run.sh` splits the corpus into 5,000-row
  chunks via one probe token each. A fresh node run rebuilds the same global
  seq range per chunk, so identities do not collide across chunks.

## 7. Artifacts and how to rerun

| file | role |
|---|---|
| `feed-sentences.mjs` | walk `~/.claude/projects/*/*.jsonl`, extract assistant/user text, strip fenced code and backticks, split sentences, chunk by seq |
| `feed-sentences.sh` | sh host body, dispatch to the node script |
| `prose-nothing.dl6` | the program: host, probe, class rels, receipt, counts |
| `run.sh` | server spin-up, probe post, idb reads |
| `fixture-sentences.json` | the 6-sentence validation set |

Rerun:

```text
./v6/labs/prose_nothing/run.sh fixture
./v6/labs/prose_nothing/run.sh corpus
```

No production code outside `v6/labs/prose_nothing/` was touched.
