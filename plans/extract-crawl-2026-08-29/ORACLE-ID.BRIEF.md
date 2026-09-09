# Lane `feat/extract-oracle-id` (opus): the measure key becomes `{lang}.{family}.{tier}.{oracle}` with no field repeating another

First action: `git merge --ff-only db081a3715cad6251e4b550fbb1f336a2972b410`. Failure or
missing tree = STOP, beep the coordinator, do not work around it.

## Goal, one sentence

The 4th field of the measure key holds the oracle's TOOL (plus an optional
variant), never a file name, so `{lang}.{family}.{tier}.{oracle}` parses back
into exactly four parts and no part repeats another.

## The user's words

"can we do {lang}.{family}.{tier}.{oracle} as it seems that is all the fields",
then "i want best organization format".

## Why (zero-context reader)

PR #632 landed the four-column key in
`plans/extract-bench-2026-08-29/RATCHET.tsv`:
`lang family tier oracle recall precision measured_at_sha`. The `oracle` column
holds a FILE NAME, and every oracle file name already encodes lang and family:

| file name | lang | tool | family | variant |
|---|---|---|---|---|
| `rust.codeql.call.tsv` | rust | codeql | call | none |
| `ts.codeql2.call.tsv` | ts | codeql2 | call | none |
| `ts.madge.module.tsv` | ts | madge | module | none |
| `go.oracle.call.vta.bare.tsv` | go | oracle | call | vta.bare |
| `go.oracle.call.cha.tsv` | go | oracle | call | cha |
| `go.oracle.type.typedecl.tsv` | go | oracle | type | typedecl |
| `rust.oracle.type.kinds.tsv` | rust | oracle | type | kinds |

Two defects follow.

1. **Repetition.** Joining the key gives
   `rust.call.checker.rust.codeql.call.tsv`: lang twice, family twice.
2. **The dot separator cannot be parsed back.** `go.oracle.call.vta.bare.tsv`
   has a variant containing a dot, so `go.call.syntax.oracle.vta.bare` gives no
   way to say where the tool ends and the variant begins.

Run `ls plans/extract-bench-2026-08-29/*.tsv` and read the naming convention
yourself before designing. Confirm or correct the table above in the PR body;
the coordinator built it from a partial listing and it is a hypothesis, not an
edict.

## Design (implement exactly this)

### 1. The oracle field

`oracle` becomes `tool[-variant]`, lowercase, with `-` joining variant parts
that were dots in the file name:

```
   rust.call.checker.codeql
   ts5.call.checker.codeql2
   ts5.module.syntax.madge
   go.call.syntax.oracle-vta-bare
   go.type.syntax.oracle-typedecl
   rust.type.checker.oracle-typedecl
```

Four dot-delimited parts, always. A `-` never appears in `lang`, `family` or
`tier`, so the split is unambiguous.

### 2. The file path is a lookup, never the key

Add a resolver on the `Case` row (or a small table beside `cases()`) mapping
`(lang, family, oracle)` to the tsv file name under `BENCH_DIR`. NO file name
appears in `RATCHET.tsv` or in any id. Assert at load that every case's file
resolves and exists; a missing file is a named panic, never a skip.

### 3. The derived id

```rust
impl Case {
    /// `{lang}.{family}.{tier}.{oracle}`, the measure signature of
    /// COMMON.md:58 as one greppable string.
    pub fn id(&self) -> String;
    /// The inverse. Panics on anything that is not four dot-delimited parts.
    pub fn parse_id(text: &str) -> (String, String, Tier, String);
}
```

Columns stay the storage; the id is derived. Do NOT collapse the four columns
into one string column: the cost file joins on `(lang, tier)` alone, and a
joined string would have to be re-parsed on every join. This is the repo's
dictionary-key law (`.claude/skills/sql-relational-design`, mandatory read).

### 4. Round-trip rail

A unit test that, for EVERY row of `cases()`, asserts
`parse_id(case.id()) == (case.lang, case.family, case.tier, case.oracle)`, and
that every id has exactly four dot-delimited parts. This is what makes the
dash-for-dot rule load-checked rather than a convention in a comment.

### 5. Files that change

- `RATCHET.tsv`: the `oracle` column's VALUES change spelling. Recall,
  precision and `measured_at_sha` are byte-unchanged. Rewrite by hand or by a
  one-shot bump; either way the numbers must not move.
- `RATCHET.cost.tsv`: unchanged, it has no oracle column.
- `COMMON.md`: the ratchet section's table gains the id spelling and the
  dash-for-dot rule.

## Acceptance receipts (PR body, no exceptions)

1. `cd v6 && just extract-ratchet` rc=0, all five legs, every recall and
   precision byte-equal to what PR #632 planted. Capture rc directly, no pipes.
   Paste all 18 accuracy rows.
2. The round-trip test from section 4, red before the id exists, green after.
3. `cargo test --release --features cli,rust-checker,ts-checker` rc=0 over the
   WHOLE test suite, not one target. PR #632 was landed after finding that
   `tests/79_rust_type_dump.rs` also does `mod bench;` and broke on a rename;
   do not repeat that miss.
4. `git diff --stat origin/main...HEAD` lists only owned files.
5. Confirm or correct the file-name convention table above.

## Files you own

- `v6/sprefa-extract/tests/bench/mod.rs`
- `plans/extract-bench-2026-08-29/RATCHET.tsv`
- `plans/extract-bench-2026-08-29/COMMON.md`, the ratchet section only

FORBIDDEN: `v6/sprefa-extract/src/**` (this lane moves no number and touches no
resolver), every other file under `tests/`, `v6/justfile`, `RATCHET.cost.tsv`,
`docs/failure-modes.md`, the corpus checkouts, `plans/extract-crawl-*`,
`v6/prolog`, `v6/tsv2`, root `src/`.

## Laws in force

- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers. The word is "oracle", never "ground truth".
- Comment budget: comments state constraints the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
- Descriptive names, never single-letter.
- Every accuracy number you write carries the full measure signature of
  `COMMON.md:58`. A percent with no fraction beside it is banned.
- Doubt yourself before asserting. Verify the naming convention against `ls`
  output, not against this brief.
- Never `--no-verify`. A blocked command ends the approach.
- 10-second law: no foreground wait over 10s. Batteries run in background with
  a log.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as feat-extract-oracle-id sprefa-coordinator "<one line>"
```
