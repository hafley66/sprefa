# extract move rule path: back to the 0.12 s band

## Context

Issue `move-rule-perf`: `extract move` dry run measured 0.12 s wall / 0.28 s user
before arc C (#475, the YAML rule path with `FactMatcher`), 0.35 s wall / 0.63 s
user after. Byte-identical output, so the regression is cost only. Target:
the 0.12 s band, byte-identical.

Measured on the installed binary from origin/main (this machine, 12 cores,
`extract_thread_cap` -> 7 workers, `v6/sprefa-extract/src/project.rs:468-487`),
dry run `extract move v6/prolog/compile/registry.pl v6/prolog/next/registry.pl
--state $(mktemp -d)`:

| run | wall (`/usr/bin/time -p`) | user CPU |
|---|---|---|
| x1 | 0.42 | 0.63 |
| x2 | 0.36 | 0.63 |
| x3 | 0.36 | 0.62 |

Phase wall, from `RUST_LOG=debug extract::source_move` timestamps on one run
(`move prescan parsed=174 skipped=109 corpus=283` at `src/0_move.rs:216`):

| phase | wall | share |
|---|---|---|
| prescan (174 files parse + rule scan on `EXTRACT_POOL`) | 167 ms | 42% |
| drain (26 files, sequential) | 180 ms | 45% |
| soopy commit (dry run) | 29 ms | 7% |

The two work phases are the target. Both run the rule scan; the drain is the
only fully sequential one.

## Where time goes (measured, scratch binary linked to the crate)

Prescan single-thread decomposition over the 174 needle-admitted files
(`carries_specifier`, `src/0_move.rs:295-299`):

| step | total | per file |
|---|---|---|
| memchr needle test | 2.7 ms | - |
| tree-sitter parse (`AstGrep::new`, `src/0_move.rs:328`) | 150 ms | 860 us |
| rule scan (`find_all(&rule.matcher)`, `src/0_move.rs:331`) | 214 ms | 1230 us |

The rule scan is the largest single cost. Its floor is `kind: atom` alone at
45 ms (257 us/file); the `follows: ... stopBy: end` and `inside` relations add
~170 ms. Scan cost is load-imbalanced: the top 5 files are 51% of the scan
(plunit_tests.pl 566 KB alone is ~38 ms), so the parallel prescan scales to
only ~2.2x on 7 threads.

Drain decomposition, 26 files sequential (replicates `drain_file`,
`src/0_move.rs:400-443`):

| step | total | per file |
|---|---|---|
| read | 7.5 ms | - |
| open (parse + blake3) | 73 ms | 2806 us |
| rule scan with facts | 103 ms | 3965 us |

Drain is read + parse + scan bound, and fully sequential (`for (rel, facts) in
&named`, `src/0_move.rs:236-244`).

## Decisions

Three arcs, ordered low-risk-first. Each is an independent saving; all three
are required to reach the 0.12 s band (projected 0.10-0.12 s wall, ~0.25 s user).

### Arc A: bounded rule relations (rules/move_specifier.yml)

The rule matches every `atom` and walks `follows: regex ^(use_module|...)$
stopBy: end` for each one (`rules/move_specifier.yml:18-19`), an unbounded
previous-sibling walk (`relational_rule.rs:254-264`, `StopBy::End` at
`stop_by.rs:138`). `potential_kinds` is `Some(atom)` so `find_all` visits every
atom (`node.rs:323-334`), and each pays the walk. `kind: atom` alone is 45 ms;
the relations add ~170 ms.

Replace the two unbounded relation legs with bounded ones, keeping the matched
node the spec `atom` (so the `SpecifierRewrite` keyed on raw text,
`src/0_move.rs:451-459`, is unchanged):

```yaml
all:
  - kind: atom
  - inside:
      kind: compound_term
      has:
        field: functor
        regex: '^(use_module|ensure_loaded|consult|include|reexport)$'
  - follows:
      regex: '^\($'
      stopBy: neighbor
```

`inside` default `stopBy` is `neighbor` (immediate parent), and `has field:
functor` is a field-child check on that parent; `follows: ^\($ stopBy:
neighbor` pins argument ONE (its immediate previous sibling is the open paren)
and excludes the functor (which has no previous sibling). All three are O(1)
walks, so the per-file scan drops from the 1230 us `stopBy: end` form to a
bounded one. Measured: **identical 598 matches / 246 distinct on the corpus**,
scan 214 -> 125 ms prescan, 103 -> 64 ms drain. This is the rule-gated scan the
issue calls "running the rule only inside directive nodes instead of whole-file
stopBy: end".

### Arc B: prescan name-gate (src/0_move.rs)

The prescan rule-scan answers one question: which specs resolve to the moved
file `old` (`src/0_move.rs:184-199`). A spec that names `old` must contain
`old`'s basename textually (resolution at `resolve`, `src/0_move.rs:490-501`,
never invents the filename). So the existing needle gate
(`carries_specifier`, `src/0_move.rs:295-299`) can be extended to also require
the moved file's stem via memchr before parsing + scanning.

Measured: of 174 needle-admitted files, only **48 contain `registry`** (the
moved stem); the 26 real candidates are a subset. Gating parse + scan to those
cuts prescan CPU from ~367 ms to ~76 ms (parse 48 x 860 us = 41 ms, scan 48 x
726 us = 35 ms). `old` itself is admitted (it contains its own stem), so its
module name still parses.

Signature change: `carries_specifier(bytes, stem) -> bool`; the stem is
`old.file_stem()`. This generalizes to any move target.

### Arc C: parallel drain with sequential merge (src/0_move.rs)

The drain loop is fully sequential (`src/0_move.rs:236-244`): 26 files, each
re-read + re-parsed + re-scanned, ~180 ms wall = 45% of the run. `drain_file`
is pure given `(root, rel, rule, facts, by_raw, source)`; only the final
`edit_stage` order must not move (`src/0_move.rs:139-140` keeps action order
and previews stable). `named` is a `BTreeMap` keyed by rel
(`src/lang/fact.rs:117-157`), so iteration is already sorted.

Run the per-file read + `drain_file` parse + scan on `extract_pool()`
(`src/project.rs:501-503`) in parallel, then merge the `edit_stage` back in rel
order. Saves ~140 ms wall (180 -> ~40 ms).

## Rejected / already-satisfied levers (measured, each under the 5% bar or done)

| lever | verdict | why |
|---|---|---|
| `potential_kinds` on rule | already active | `Some(atom)` set, And keeps non-null set (`ops.rs:31-38`); too broad to prune further |
| `potential_kinds` on `FactMatcher` | already fine | returns `None` (matcher.rs:39 default); And xor keeps the atom set |
| fact preload once | already done | `FactSet::load_by` once before the drain loop (`src/0_move.rs:233`) |
| rule parse once per run | already done | `specifier_rule()` once (`src/0_move.rs:136`) |
| prescan parallel | already done | `extract_pool().install(|| corpus.par_iter()...)` (`src/0_move.rs:141`) |
| parallel prescan improvement | rejected | already parallel; wall is load-imbalance-bound (top-5 files = 51%), not thread-count-bound |

## Verification

Per arc, after each lands and before the PR:

- `cargo test --release --features cli --test 1_move --test 37_fact_matcher`:
  move byte-identical + fact-matcher cases stay green.
- `cargo test --release --features cli`: whole crate.
- Byte-identical receipt from PR #475: normalize stage ids, diff stdout against
  a base capture, expect empty.
- Perf receipt, x3 each: `extract move v6/prolog/compile/registry.pl
  v6/prolog/next/registry.pl --state $(mktemp -d)`, plus `RUST_LOG=debug` span
  check (`move prescan parsed=48 ...`, drain timings).
- `cargo fmt` once immediately before commit (per AGENTS.md).

Expected final: ~0.10-0.12 s wall, ~0.25 s user, byte-identical.

## Staffing

Implementer: deepseek-v4-flash-0731 (flash4), worktree `plan/move-rule-perf`,
base SHA 4e478c60725d3d4cf8d86f2674af4c44b5723a81. Suite budget: the two test
commands above only (no new tests needed; existing byte-identical + fact-matcher
suites cover the rule and gate; a drain-order unit test is optional).

## Files touched / forbidden

Touched: `v6/sprefa-extract/rules/move_specifier.yml` (Arc A),
`v6/sprefa-extract/src/0_move.rs` (Arcs B, C).
Forbidden: `src/lang/**` (arc C owns `fact.rs` per #475 note), `src/project.rs`
(pool is read-only, `src/project.rs:501`), `tests/**` except read.

## Risk table

| arc | risk | citation / mitigation |
|---|---|---|
| A | rule over- or under-matches a spec spelling | equivalence gate: 598/246 must hold on corpus; `tests/1_move.rs` + fact-matcher cases catch drift |
| A | `operator_atom` functor (`:-`, operators) no longer matched | prolog grammar functor field types are `atom`+`operator_atom` (tree-sitter-prolog node-types.json); equivalence gate on corpus covers the observed set; add one operator case if the corpus lacks it |
| B | name-gate misses a spec that names `old` | impossible: resolution never invents the filename (`resolve`, `src/0_move.rs:490-501`), any candidate spec contains `old`'s stem; `old` self-admitted for module name |
| B | stem too common (inflates admitted set) | cost only; 48 vs 174 measured, still a 3.6x cut |
| C | action order / previews move | merge `edit_stage` in rel order (`named` BTreeMap already sorted); byte-identical receipt catches it |

## Build-vs-buy

No new dependency proposed. The bounded-rule rewrite uses only the ast-grep
relation primitives already in the crate (`inside` with nested `has` + `field`,
`follows` with `stopBy: neighbor`, all in `ast-grep-config-0.38.7`
`relational_rule.rs` / `rule/stop_by.rs`). Parallelism reuses the existing
`extract_pool()` (`src/project.rs:501-503`). A bespoke pre-rule scanner was
considered and rejected: the ast-grep `has field: functor` leg already confines
the walk to directive calls, which is the same bound a custom scanner would
hand-roll with no measured advantage. `memchr` (the stem gate) is already a
transitive dependency of ast-grep-core; no new crate.

<!-- todo(perf): Arc A bounded rule: verify a directive using an operator_atom or quoted functor ('use_module'(...)) still matches, add a test case if the corpus lacks one -->
<!-- todo(perf): Arc B name-gate: confirm old itself is always admitted (it contains its own stem) and its module name still parses -->
<!-- todo(perf): Arc C parallel drain: assert edit_stage order equals sorted rel order in a unit test before relying on the byte-identical receipt -->
