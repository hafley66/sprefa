# Plan: option<T> rows vs SQL NULL 3VL, measured

Status: complete (pass 1 of 2). Measures the SQLite cost of v6's
option-to-some/none-rows lowering against plain NULL + 3VL on equivalent
workloads. Numbers and query plans only. No recommendation.

## TOC

- Context
- Question
- Encodings
- Harness
- N and density cut
- Results
- Query plans
- File and index sizes
- Anomalies
- Artifacts
- Open follow-ups

## Context

sprefa v6 lowers `option(T)` columns to separate some/none row tables instead
of a nullable column. The question is what that costs in SQLite on equivalent
data. Pass 1 produces the measurements; a debur pass follows.

## Question

Measure W1-W5 on three encodings of the same logical data. Numbers and query
plans for the coordinator to interpret. Do not recommend.

## Encodings

Three separate db files, identical logical data at every density.

- E1 baseline 3VL: `person(person_id INTEGER PRIMARY KEY, email TEXT)`, absent =
  NULL.
- E2 option enum instance: the landed shape from
  `v6/prolog/compile/out/option_text_column_reads_through_tag_join.ts`.
- E3 split rel: `person(person_id INTEGER PRIMARY KEY)` +
  `person_email(person_id INTEGER PRIMARY KEY, email TEXT NOT NULL)`, absence =
  no row.

## Harness

- One file `bench.mjs`, node `v24.15.0`, builtin `node:sqlite` DatabaseSync.
- No package installs, no package manager.
- pragmas: `journal_mode=WAL`, `synchronous=NORMAL`, `cache_size=-64000`.
- Each timed leg: 1 warmup + 5 runs, median ms reported.
- Absent is deterministic on `(person_id * 2654435761) % 100 < D`.
- Emails are `'user' + person_id + '@example.com'` for present rows.
- W3 uses 10,000 random ids drawn once from a fixed seed (mulberry32, seed 42),
  shared across all encodings and densities.

## N and density cut

Brief fixed N = 1,000,000, with a permit to cut to 100,000 if any single timed
leg exceeds 60 s. On N = 1,000,000, E2 W3 (point reads) took about 124 s for a
single run: the `__opt_text_some` value table is keyed on `value`, not `id`, so
each id lookup scans the whole present set. That exceeds the 60 s budget, so N
was cut to 100,000 and everything was rerun. All numbers below are N = 100,000.
The three encodings hold identical logical data at every density (verified:
D1 = 99,000 present / 1,000 absent, D50 = 50,000 / 50,000, D99 = 1,000 / 99,000
in every encoding).

## Results

Medians in ms, N = 100,000.

### W1 bulk insert

| encoding | D1 | D50 | D99 |
| --- | --- | --- | --- |
| E1 NULL 3VL | 58.9 | 48.5 | 40.1 |
| E2 option rows | 295.7 | 215.4 | 123.3 |
| E3 split rel | 83.9 | 62.1 | 37.8 |

### W2 present-scan

| encoding | D1 | D50 | D99 |
| --- | --- | --- | --- |
| E1 NULL 3VL | 4.7 | 3.5 | 1.7 |
| E2 option rows | 45.9 | 28.6 | 9.9 |
| E3 split rel | 3.8 | 1.9 | 0.1 |

### W3 point reads

| encoding | D1 | D50 | D99 |
| --- | --- | --- | --- |
| E1 NULL 3VL | 15.5 | 15.4 | 14.0 |
| E2 option rows | 12097.1 | 9109.3 | 257.7 |
| E3 split rel | 14.2 | 12.8 | 10.6 |

### W4 grouped

| encoding | D1 | D50 | D99 |
| --- | --- | --- | --- |
| E1 NULL 3VL | 11.4 | 6.7 | 1.7 |
| E2 option rows | 23.5 | 16.8 | 11.2 |
| E3 split rel | 10.2 | 5.1 | 0.1 |

### W5 absent-scan

| encoding | D1 | D50 | D99 |
| --- | --- | --- | --- |
| E1 NULL 3VL | 2.0 | 2.1 | 1.8 |
| E2 option rows | 13.1 | 12.0 | 11.5 |
| E3 split rel | 14.4 | 11.7 | 7.9 |

## Query plans

See REPORT.md EXPLAIN QUERY PLAN section (verbatim). Key shapes:

- E1 W2/W5: `SCAN person`; W3: `SEARCH person USING INTEGER PRIMARY KEY`;
  W4: `SCAN person` + temp b-tree group.
- E2 W2: `SCAN some` + `SEARCH tag (id=? AND tag=?)` + `SEARCH str (rowid=?)`
  + bloom filter on p + covering-index `SEARCH p (email=?)`.
- E2 W3: `SEARCH p USING PRIMARY KEY (person_id=?)` then `SCAN some LEFT-JOIN`
  then `SEARCH str (rowid=?)`. The `SCAN some` is the point-read cost driver.
- E2 W4: `SCAN p` + `SEARCH tag (id=? AND tag=?)`.
- E3 W2: `SCAN person_email`; W5: `SCAN person` + correlated scalar subquery +
  `SEARCH e (rowid=?)`.
- E2 W2/W4/W5 all key the tag table on `(id, tag)`, an integer composite key.

## File and index sizes (bytes after wal_checkpoint TRUNCATE)

| encoding | density | file | table / index bytes (dbstat) |
| --- | --- | --- | --- |
| E1 | D1 | 2,994,176 | person 2,990,080 |
| E1 | D50 | 1,953,792 | person 1,949,696 |
| E1 | D99 | 917,504 | person 913,408 |
| E2 | D1 | 10,047,488 | person 1,306,624; __opt_text_none 12,288; __opt_text_some 1,294,336; __opt_text_tag 1,228,800; __str 2,969,600; sqlite_autoindex___str_1 3,231,744 |
| E2 | D50 | 6,660,096 | person 1,306,624; __opt_text_none 442,368; __opt_text_some 634,880; __opt_text_tag 1,163,264; __str 1,490,944; sqlite_autoindex___str_1 1,617,920 |
| E2 | D99 | 3,383,296 | person 1,306,624; __opt_text_none 872,448; __opt_text_some 16,384; __opt_text_tag 1,110,016; __str 36,864; sqlite_autoindex___str_1 36,864 |
| E3 | D1 | 3,764,224 | person 790,528; person_email 2,969,600 |
| E3 | D50 | 2,293,760 | person 790,528; person_email 1,499,136 |
| E3 | D99 | 831,488 | person 790,528; person_email 36,864 |

The `sqlite_autoindex___str_1` is the UNIQUE index backing the `__str.content`
dictionary key. E2 stores every distinct email as a dictionary entry plus its
autoindex copy, which is the bulk of the E2 file gap at D1.

## Anomalies

- E2 W3 point reads are 2 to 3 orders of magnitude slower than E1 and E3 at
  D1 and D50 (12,097 ms and 9,109 ms vs about 15 ms and 14 ms). Cause in the
  plan: after the person_id lookup, `SCAN some LEFT-JOIN` walks the whole
  `__opt_text_some` present set for every id, because that table is keyed on
  `value`, not `id`. At D99 the present set is 1,000 rows, so the scan shrinks
  to 257.7 ms, which tracks present-set size. This single query drove the N cut.
- E2 W2 present-scan runs four relations (tag, some, str plus a covering-index
  lookup on person) and is about 10x E1 W2 and 12x E3 W2 at D1, narrowing as
  the present set shrinks.
- E2 D99 (mostly absent) compresses toward E1 and E3 on the scan workloads: W2
  9.9 ms vs E1 1.7 ms / E3 0.1 ms, W4 11.2 vs 1.7 / 0.1. The option tables for
  absent rows still carry a tag row per person, so E2 W5 stays near E3 W5 at
  every density.
- E2 insert is 5.0x E1 at D1 and 3.3x E3 at D1, partly because each present row
  writes the dictionary plus the some and tag tables, and each distinct email is
  a new `__str` dictionary entry.
- E1 and E3 W3 are flat across densities (both are single PRIMARY KEY point
  lookups). E2 W3 tracks present-set size, not total N.

## Artifacts

- `bench.mjs`: the whole harness, one file.
- `results.json`: raw, every run.
- `REPORT.md`: E2 DDL verbatim, tables, plans, sizes, anomalies.
- This plan doc.

## Open follow-ups

- Debur pass: decide whether E2 W3 should use the landed shape as-is (which scans the
whole present set per point read) or whether the lowering is expected to add an
id index on `__opt_text_some`. Not evaluated here because the brief forbids
changing the E2 shape.
- The `__str` dictionary stores every distinct email, so the benchmark exercises
  the worst case for the dictionary (all emails distinct). A repeated-value
  email workload was out of scope.

## Lab cull
Lab worktree lane/opt3vl-bench (bench.mjs, results.json, REPORT.md) last copy: commit 408b58c3.
Defect surfaced: emitted __opt_text_some has PRIMARY KEY(value) and no id index; id->value reads SCAN. Fix queued.
