# multirepo_crawl — stopping-point program #5, graded against v5

`just multirepo-golden`

The v5 engine and the v6 engine each answer the same cross-repo dependency
question over the same four-repository corpus, and the answers are diffed rel
by rel.

| file | what it is |
| ---- | ---------- |
| `0_multirepo_crawl.dl6` | the v6 port of `examples/version-skew.dl` |
| `1_corpus.sh` | builds the pinned corpus: 4 one-commit git repos + the v5 config |
| `2_gate.sh` | the rig (v5 leg, v6 leg, the grade) |
| `3_classify.py` | buckets every difference, proves each bucket from corpus bytes |

## The result

    MULTIREPO CRAWL GRADED: 4/4 rels byte-identical, 0 classified,
                            1 named gap (dep_ver), 0 unclassified

`dep_pin`, `skewed`, `skew_row` and `skew_width` come out byte-identical
between the two engines, and both match a third opinion — the classifier reads
the corpus `go.mod` bytes itself rather than trusting either engine's regex.

## The one named gap

v5 writes the two witness versions with aggregates:

    dep_ver(m, min(v), max(v)) <- dep_pin(_, m, v).

v6 refuses that by name: `aggregate_operand_not_number(min, _, text)`. min/max
lower to a delta-compare against the stored extremum and the emitter only has
the numeric comparison, so a version *string* has no lowering. `dep_ver` and
the `lo`/`hi` columns of `skew` are therefore not expressible today.

Skew **membership** is not blocked, because the skew test never needed an
ordering — v5's own program header says to treat `lo`/`hi` as "two witnesses,
not oldest/newest". "More than one distinct version exists" is a self-join and
a disequality, so `skewed`, `skew_row` and `skew_width` all grade in full. The
gap costs two columns and no rows, and the gate prints v5's three `dep_ver`
rows so the thing that is missing is visible rather than absent.

## Two differences in kind, stated rather than hidden

**The repo set.** v5 reads `repo(slug, root, url)` out of `$SPREFA_CONFIG` —
the multi-repo set is ambient configuration loaded before the program runs. v6
has no such file and should not grow one (ruling `spine_residency`: the git/fs
spine is hosted *in the language*), so the repo set arrives as ordinary EDB
rows posted to `/arrivals`. v5's repo set is configuration; v6's is data.

**The regex dialect.** v5 uses the rust regex crate; the v6 leg uses python
`re` inside `parity-grep.py` and cannot spell a backslash at all (one in a
`.dl6` string constant is eaten twice on the way to a host). So `\S` becomes an
explicit character class and `\s+` becomes `[ ]+`. Assertion 2 in the gate
proves the two dialects select the same lines on this corpus instead of
assuming it, the same way the flagship rig pins its globset-vs-pathspec
divergence.

## A v5 oddity this rig noticed

v5's own `rel_dep_ver_txt` returns `lo=v0.9.1 hi=v0.8.0` for
`github.com/pkg/errors` — min/max reversed against the lexicographic order its
header claims — while `example.com/shared` comes back correctly ordered. Since
v6 refuses the construct entirely there is nothing to diff, so this is written
down rather than graded. It is the kind of thing a grading rig exists to
notice.

## Zero new constructs

The v6 program is one `sh` host, four rules, a self-join, a disequality, and a
`count` aggregate. Crossing repositories needed only the repo root to become an
input column, after which `cd '{root}'` in the host template is the entire
adaptation — the witness now includes the root, so each repo is one subprocess
and asking twice is free.
