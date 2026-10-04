# Brief: macrotime on deltas (page 1 of the IVM-all-the-way plan)

User (2026-10-04): "idea was using ivm to dogfood the stuffs all the way thru ... lets do it".
Base: local main (sprefa `404a28e78` or later; sqlite_ivm `b791f71`).

## Today (`src/_1_macrotime/_4_expand.rs:86-230`, `src/_6_eval/_9b_ir_eval.rs`)

Per unit, per wave: seeds = macro seeds + `syntax_seeds(graph.rows)` (all rows, `:124-127`);
`IrSession::evaluate` re-lowers the same program (`_9b:208`), maps all terms and builds a
`BTreeSet` of all seeds, diffs against `last_seeds` (`_9b:630-680`), settles the delta (`:683`),
snapshots every visible relation and decodes all terms (`:690-700`); `macro_results` reads the
full closure; `rewrite_active_graph` builds a new full row set; `seen.push(rows.clone())` keeps a
full copy per wave for cycle detection. The rule cone is fixed per expansion (`:108` comment).

## Do

1. Lower + install once per expansion: `IrSession::prepare(program)`, then `apply(changes)` per
   wave. No re-lower per wave.
2. `rewrite_active_graph` returns `{ removed, added }` rows (it knows the claimed subtrees and the
   outputs); the wave sends them as -1/+1 changes. No full seed rebuild, no full-set diff.
3. `apply` returns the engine `Delta`; `macro_results` reads claims/outputs from the changed rows
   of the claim/output relations, not from a snapshot of every relation. Keep `Graph` (Rust) in
   step with the applied diff.
4. Cycle check: `seen` holds a 64-bit hash of each wave's row set (order-independent, e.g. sum or
   xor of per-row hashes), not a clone.
5. Measure first and after: per wave rows sent / rows changed / closure rows read, waves per
   unit, wall per unit, for the `registry.rs` build and the c15 corpus compile; write
   `plans/v8/2026-10-04-macrotime-deltas.findings.md` (before section before changing code).

Out of scope: sharing one engine across units (page 2; IR depends on seed terms,
`_9a_ir_lower.rs:2141`), moving the rewrite into dl7 rules (page 3), comptime (page 4).

## Laws

- One cargo process at a time; `uptime` first; wait if load > 12; `-j 4`; `--test-threads=1`.
- Make all edits, `cargo check -j 4` while editing, then test once, re-run failures by name, final
  full run per engine.
- Byte-identical macrotime output: `_0_read_lower_check::every_macrotime_case_matches_v7` and the
  registry `cmp` stay green. No golden edits except via `oracle/refreeze.py` (expect none).
- No recursion (`_9j_no_recursion`), no new kernel relation (stop and report), no silent
  fallbacks, no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- sqlite_ivm unchanged expected; if needed, clone inside the worktree, branch `perf/macrotime-deltas`.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Validation

```
cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1
DL8_ENGINE=ir:dd cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1
dl8 build std/tsi_registry_emit.dl7 --out <tmp> && cmp <tmp>/target/tsi/registry.rs ~/projects/hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs   (both engines, 3 runs, times)
```

## Receipt

```
status: done | blocked
branch + sha:
commits: <sha subject>
before/after: rows sent per wave, closure rows read per wave, macrotime wall, registry build (both engines)
validation: <command + result line each>
next: <one action>
```

Also add a "How it works" section to the findings (problem with numbers, step trace, file:line,
before/after) so the parent can walk the user through it.
