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
so the ratchet leg (`v6/sprefa-extract/tests/ratchet_recall.rs`, the two tsvs in
this dir) never runs in CI and is not a CI-KNOWN-RED row. It runs in-process on
this machine only, one test process per `(lang, tier)` leg: 3 `diet_scip`
(resolve call+types) runs, median wall, process-peak RSS, then every
`bench::cases()` row naming that leg scored against its oracle tsv.

| file | key | what it holds | tolerance |
|---|---|---|---|
| `RATCHET.tsv` | `(lang, family, tier, oracle)` | recall, precision | 0.10 pt |
| `RATCHET.cost.tsv` | `(lang, tool, tier)` | files, wall_ms, rss_mb, pid | wall +15%, rss +10% |

### The measure id

The accuracy key joins into one greppable string,
`{lang}.{family}.{tier}.{oracle}`, four dot-delimited parts and no part
repeating another. `Case::id` writes it, `Case::parse_id` reads it back, and
`every_case_id_round_trips_through_four_parts` in
`v6/sprefa-extract/tests/bench/mod.rs` asserts the round trip over every row of
`cases()`.

| part | values |
|---|---|
| `lang` | `ts5`, `go`, `rust` |
| `family` | `call`, `module`, `type` |
| `tier` | `syntax`, `checker`, `scip` |
| `oracle` | the oracle's TOOL (`codeql`, `codeql2`, `madge`, `scip_override`, `oracle` for our own reference producer), plus an optional variant |

A variant joins to its tool with `-`, never `.`: the go VTA-bare oracle is
`oracle-vta-bare`, the typedecl-projected type oracles are `oracle-typedecl`.
`-` appears in no `lang`, `family` or `tier`, so the four-way split stays
unambiguous. The 18 ids:

```
go.call.syntax.codeql2            rust.call.checker.codeql
go.call.syntax.oracle-vta-bare    rust.call.checker.oracle
go.module.syntax.oracle           rust.call.checker.scip_override
go.type.syntax.oracle-typedecl    rust.type.syntax.oracle-typedecl
rust.call.syntax.codeql           rust.type.checker.oracle-typedecl
rust.call.syntax.oracle           ts5.call.syntax.codeql2
rust.call.syntax.scip_override    ts5.call.syntax.oracle
ts5.call.checker.codeql2          ts5.module.syntax.madge
ts5.call.checker.oracle           ts5.module.checker.madge
```

**No file name lives in an id or in `RATCHET.tsv`.** The tsv a case scores
against is a lookup, `oracle_files()` beside `cases()`, keyed on
`(lang, family, oracle)`; tier never picks a file, since the syntax and checker
legs score against the same oracle. The map is not derivable from the key: ts5
scores against `ts.*` files for every tool but its own (`ts5.oracle.call.tsv`),
and a variant that is `-` in an id is `.` in a file name
(`go.oracle.call.vta.bare.tsv`). A key with no listed file, or a listed file
that is not on disk, is a named panic in both `oracle_path` and the ratchet leg,
never a skipped row.

Cost belongs to the PRODUCER: `rss_mb` is `getrusage(RUSAGE_SELF)` of the named
pid, so a row is truthful only for the process that folded its rows. Our own
rows carry `tool=sprefa`; an out-of-process producer (codeql, madge, node/tsc)
gets its own `tool` row rather than borrowing that figure.

`RATCHET_BUMP=1` improves only, `RATCHET_FORCE=1` rewrites; a duplicate key in
either file is a panic. The legs are `ts5.syntax`, `ts5.checker`, `go.syntax`,
`rust.syntax`, `rust.checker`; `just extract-ratchet` runs all five under one
feature set (`cli,rust-checker,ts-checker`). File rules: ts5 `src/**` minus
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
