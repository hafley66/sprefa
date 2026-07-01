# Seam benchmark

A scoring rig for the open question: **are these discrete joins actually useful
for AI-session / cross-repo change-awareness, and would embeddings add
anything?** It answers empirically, not by vibe.

## What it is

- `corpus/` — a tiny polyglot codebase + docs with KNOWN cross-seam answers.
  Rust + TypeScript + Kotlin each declare a `PaymentClient`; a doc references it
  and also carries a planted stale heading (`oldCharge`).
- `authors.tsv` — `prefix <TAB> name <TAB> email`. The harness reconstructs a
  git history committing each subtree as its own author, so the `created`
  built-in recovers who-made-what.
- `*.dl` — one checked-in seam query each, ending in a single `?` scored query.
- `gold.json` — the ground-truth set per seam: `{query, key, gold}`.
- the harness `tests/it/seam_bench.rs` runs every `.dl` via `--query-json`,
  projects the scored query to its `key` columns, and reports
  precision/recall/F1 against `gold`.

## Seams shipped

| seam | kind | stitches |
|---|---|---|
| `xlang-siblings` | code↔code | one symbol name across Rust/TS/Kotlin (type graph) |
| `doc-drift` | doc→code | headings resolving to no symbol (negation anti-join) |
| `authorship` | git | who created each file (`created`) |
| `impact` | integrator | one symbol's blast across the xlang + doc seams at once |

All four are exact on the planted corpus (P = R = 1.0). A miss is a real
measurement — an extractor dropped a planted fact — not a flaky test. Fix the
corpus or the query; do not loosen the bar.

## Run it

```
cargo test --test it seam_bench -- --nocapture
```

## Adding a seam (this is where AI authors the stitch)

1. Write `bench/seams/<name>.dl`. Scan the corpus, derive your relation, end
   with one `? <query>(...)`.
2. Add a `gold.json` entry: `"<name>": {"query": "<query>", "key": [...cols],
   "gold": [[...], ...]}`.
3. Re-run. The rig scores it automatically.

## The embedding A/B

This is the apparatus for "do embeddings help at all." The discrete seam gets a
number now. To test an embedding-augmented variant:

1. Add the embedding tool to `dl` (e.g. a `graph_similar(sym, other, score)`
   built-in backed by `sqlite-vec` over node2vec walks of the existing edges).
2. Write a second `.dl` for the SAME seam that uses the fuzzy seed step, scored
   against the SAME `gold.json` entry.
3. Compare F1. If the embedding variant does not beat the discrete one on this
   ground truth, embeddings are not earning their keep for that seam — keep it
   discrete. Embeddings should only win where the gold itself is fuzzy (a
   natural-language query with no nameable relation), which is the seam to add
   when you want to actually exercise them.
