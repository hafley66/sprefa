# Paired comment markers as a living closure ledger — 2026-07-17

Status: TECHNIQUE CAPTURE. Proven in smashy's V1/V3-to-V4 parity ledger; this
plan turns the reusable shape into Sprefa documentation and a runnable example.

## Context

We needed one record that stayed beside executable source, exposed every known
missing counterpart, rejected ambiguous claims, and generated its own completion
readout. A hand-maintained task table drifted. Sprefa already had every needed
piece: grammar-backed `comment_node`, set relations, `count`, integer arithmetic,
negation, and convergent `gen(:zone)` output.

This technique measures **inventoried marker closure**, not total project
completion. Unknown work is outside the denominator and must never be implied by
the displayed percentage.

## The pattern

1. Put the same semantic key beside both executable expressions:

   ```text
   // parity(surface-admission): source behavior expressed here
   // parity(surface-admission): target behavior expressed here
   ```

2. File provenance assigns the role. Source-tree comments are left/open;
   target-tree comments are right/close. The prose explains the local witness;
   the key is identity.
3. `comment_node` extracts only real comments, so marker-looking strings do not
   become facts.
4. Count each side per key. A key closes only with exactly one left and exactly
   one right witness. Missing sides become the task list; repeats become an
   ambiguity list.
5. Union both sides into a unary key relation. Set semantics deduplicate the
   denominator. Closed unique keys form the numerator.
6. Sprefa computes `closed * 100 / total` and writes the percentage, raw counts,
   closed pairs, one-sided keys, and repeats into named generated zones.

Core DL shape:

```dl
rel key(k: text).
key(k) <- left(k, _, _).
key(k) <- right(k, _, _).

rel left_n(k: text, n: int).
left_n(k, count(path)) <- left(k, path, _).
rel right_n(k: text, n: int).
right_n(k, count(path)) <- right(k, path, _).

rel closed(k: text).
closed(k) <- left_n(k, 1), right_n(k, 1).

rel total(n: int).
total(count(k)) <- key(k).
rel done(n: int).
done(count(k)) <- closed(k).
done(0) <- total(_), !closed(_).

rel closure(percent: int, done: int, total: int).
closure(done * 100 / total, done, total) <- done(done), total(total), total > 0.
```

Integer division floors deterministically. Show the raw fraction beside the
percentage so the rounding and denominator remain inspectable.

## Decisions

- Call the result **marker closure**, never “project completion.”
- Denominator: every unique key observed on either side.
- Numerator: keys with exactly one witness on each side.
- Right-only, left-only, and repeated keys remain unfinished.
- Roles come from configured path sets, not words such as V1/V4 or old/new.
- No manually authored totals, scores, line numbers, or model estimates.
- Generated output is a view; comments beside executable witnesses are the
  living record.
- This is composition of existing Sprefa language features. No engine change.

## Documentation landing

- Add a terse “Paired comment closure ledgers” recipe near the README codegen
  loop. Link the runnable example and state the inventory-completeness caveat.
- Let `examples/gen-reference.dl` add the example to README and
  `docs/reference/examples.md`; do not hand-edit generated indexes.
- Keep canonical operator copy in `src/engine/decls.rs::op_docs()` unchanged;
  the technique composes documented `comment_node`, aggregation, arithmetic,
  negation, and `gen(:zone)` behavior.

## Runnable example

Add `examples/paired-marker-closure.dl` plus tiny Rust/TypeScript fixture sides
and a Markdown ledger fixture. The example should contain:

- one closed key;
- one left-only key;
- one right-only key;
- one repeated-side key;
- generated summary, closed, missing-left, missing-right, and repeated zones;
- a first-file comment concise enough for the generated examples index.

The example must use `comment_node`, not raw regex over file contents, and must
derive marker identity from the stripped comment text. Use the repository's
existing `examples/gen-type-table.dl` as the `gen`-zone house style.

## Verification

```sh
dl examples/paired-marker-closure.dl --parse-only
dl examples/paired-marker-closure.dl --no-daemon
dl examples/paired-marker-closure.dl --no-daemon
dl examples/gen-reference.dl --no-daemon
```

The first full run must produce the expected closed/open/repeated classification
and machine-derived percentage. The second must make no file change. The
reference generator must list the example in both generated indexes.

## Staffing

One implementer owns example, fixtures, and README recipe as one documentation
slice. Review checks the fixture semantics and the “marker closure, not project
completion” wording. No engine worker is needed.

## Origin pointers

- smashy `.dl/gen-parity.dl`
- smashy `docs/generated/PARITY.md`
- Sprefa `src/engine/extract/text.rs` and `src/cst.rs` (`comment_node`)
- Sprefa `src/engine/decls.rs::op_docs()` (operator contract)
- Sprefa `examples/gen-type-table.dl` (generated-zone precedent)
