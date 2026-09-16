# prose laws in dl6

## What it is

`v6/labs/prose_laws/` ports the prose Stop-hook rules from
`~/projects/claude-research/prose-prod/prose-prod.mjs` to a dl6 program
(`prose-laws.dl6`). Each rule is one dl6 rel over `sentence_row`, unioned into a
single `violation(rule_id, seq, sentence)` output rel. Hosts (the sh + node feed
scripts) fetch only raw sentences; all classification is `regexp/2` in dl6.

## TOC

- Rules
- Fixture receipt (verbatim)
- Gaps

## Rules

| rule_id | rel | prose-prod source |
|---|---|---|
| `em-dash` | `em_dash` | `em-dash` |
| `neg-parallelism` | `neg_parallelism` | `neg-parallelism` (same + cross sentence) |
| `deictic-filler` | `deictic_filler` | `deictic-filler` |
| `one-word-sentence` | `one_word_sentence` | `one-word-sentence` |
| `banned-stem` | `banned_stem` | `banned-word` + `decreed-stem` |
| `nothing-sentence` | `nothing_sentence` | `nothing-sentence` (evaluative + contrast + no receipt) |

## Fixture receipt (verbatim)

`v6/labs/prose_laws/fixture-sentences.json` carries one hit per rule plus one
clean sentence. `./run.sh fixture` output:

```
== PROSE-LAWS (mode=fixture) ==
run time: 1447 ms
sentences: 7 {'assistant': 4, 'user': 3}

em-dash: 1
    seq 0: The plan covers the full arc — from seed to ship.
neg-parallelism: 1
    seq 1: We should not patch the return path, but harden the store.
deictic-filler: 1
    seq 2: Here is the complete table of results.
one-word-sentence: 1
    seq 3: Stop.
banned-stem: 1
    seq 4: The substrate composes the lower layer.
nothing-sentence: 1
    seq 5: The explicit route is genuine rather than faked.
```

Fixture sentences, in order:

1. em-dash — `The plan covers the full arc — from seed to ship.`
2. neg-parallelism — `We should not patch the return path, but harden the store.`
3. deictic-filler — `Here is the complete table of results.`
4. one-word-sentence — `Stop.`
5. banned-stem — `The substrate composes the lower layer.` (`substrate`)
6. nothing-sentence — `The explicit route is genuine rather than faked.`
7. clean — `The wheel sits on the ground by the gate.` (bare `ground` is the noun, not flagged)

## Gaps

- Sentence split stays in the feed script (`feed-sentences.mjs`); a
  sentence-boundary builtin does not exist yet.
- dl6 refuses `(?i)` (`regexp_pattern_outside_subset`); case variants are
  spelled out in each pattern.
- dl6 refuses a literal apostrophe in a regexp (`quote_in_literal`, because the
  pattern lowers into a SQL single-quoted string). Apostrophe contraction forms
  from prose-prod (`Here's`, `n't`, `it's`/`that's`, `[A-Za-z'-]`) are dropped;
  each rule keeps its long/uneapostrophed forms.
- `banned-stem` treats `ground` as verb only: `grounded|grounding|grounds`, never
  the bare/prepositional noun. `support` is matched at the inflected-stem level
  (`support|supports|supported|supporting`).
- Cross-sentence `neg-parallelism` (a `not` sentence followed by one opening
  `It is`/`That is`/`This is`) is implemented and confirmed firing; the fixture
  delays it so the receipt stays one hit per rule.
