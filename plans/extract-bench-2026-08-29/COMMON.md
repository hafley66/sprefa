# Bench lab, common contract (2026-08-29)

Question from the user: how far is sprefa-extract from 100% module and call
edges, what do the compilers and other static-analysis tools get on the same
corpora, and how much of scip's reach do we consume.

Corpora (read-only, never modify):
| lang | path | files |
|---|---|---|
| ts | /Users/chrishafley/projects/TypeScript-5.9 (src/**) | see ts5.REPORT.md |
| go | /Users/chrishafley/projects/typescript-go | 5,097 .go |
| rust | /Users/chrishafley/projects/rust-analyzer (crates/*/src/**) | 873 |

First action in every lane:
```
git merge --ff-only 136a28bc9439efea8676f10d7e61f513f0471b95
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree. Every
call under `timeout 10` except index builds (`timeout 900`, background, log).
Failure: STOP, `boop beep --no-wait --as <lane> sprefa-coordinator "<one line>"`.

Edge normal form, every tool, every lane, so the tables join:
`plans/extract-bench-2026-08-29/<lang>.<tool>.<family>.tsv` with columns
`src_path  src_name  dst_path  dst_name` (paths relative to the corpus root,
names bare; a module edge has empty names). Families: `module` (file imports
file), `call` (fn calls fn), `type` (type refs / implements / extends).
Counts alone are not a receipt; the tsv is.

Comparison script (write once, share): `bench.py <a.tsv> <b.tsv>` prints
`|a|, |b|, |a∩b|, a-only, b-only` and a 20-row sample of each difference set.

Deliverables per lane: the tsvs, a REPORT.md with one table per language
(rows = tool, columns = family counts + overlap with our parse resolve +
overlap with raw scip), and a "what it took to run" table (install steps,
wall, disk, failures). Zero prose paragraphs; tables and file paths.
Laws: no em dashes, no eprintln, descriptive names, never --no-verify.

## Ratchet (2026-08-29, `just extract-ratchet`, LOCAL-ONLY)

The corpora above are machine-local checkouts under `/Users/chrishafley/projects`,
so the ratchet leg (`v6/sprefa-extract/tests/ratchet_recall.rs`, RATCHET.tsv in
this dir) never runs in CI and is not a CI-KNOWN-RED row. It runs in-process on
this machine only: 3 `diet_scip` (resolve call+types) runs per corpus, median
wall, process-peak RSS, recall/precision against the oracle tsvs, floors in
RATCHET.tsv (tolerances 0.10 pt / +15% wall / +10% rss; `RATCHET_BUMP=1`
improves only, `RATCHET_FORCE=1` rewrites). File rules: ts5 `src/**` minus
`src/lib`, go every `.go` under the root, rust every `.rs` under `crates/`
with a `src` path component; roots overridable via `RATCHET_TS_ROOT`,
`RATCHET_GO_ROOT`, `RATCHET_RUST_ROOT`.

## Measure signature (user-set 2026-08-31, every report, every PR body, every chat number)

A percent with no fraction beside it is banned. Every accuracy number carries
all six slots:

```
⟨lang.family.tier⟩ vs ⟨oracle⟩ on ⟨corpus, n files⟩ :
    ⟨metric⟩ = numerator/denominator ⟨row unit⟩
```

Example: `rust.call.checker vs codeql on rust-analyzer(873f): recall 73.37% =
37,915 matched / 51,679 oracle edge-rows`.

The only denominators that exist:

| metric | fraction | plain reading |
|---|---|---|
| recall | matched / ORACLE's rows | of what exists, found |
| precision | matched / OUR rows | of what we said, true |
| 3-bucket | OUR rows = matched + contradicted + unjudged | contradicted = oracle names a different dst for the same src |
| wall | ms, one process over the stated file count | |
| rss | MB, that process's getrusage high-water | |

Tier ∈ {syntax, checker, scip}. An edge-row is one (caller, callee) pair in
the 4-col normal form. `matched` is always the byte-equal 4-col intersection.
