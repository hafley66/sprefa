# effect_proof

Smallest-possible validation that v3's plugin authoring surface is one
file per `(effect + batcher)`. Pretend / minimal; framework
scaffolding at ~90 LoC.

**Full teaching document with measurements and design-doc join:
[FINDINGS.md](./FINDINGS.md).** Read that after this README for the
whats / whys / hows across the perf probe, the topology taxonomy, the
plugin arch affirmation, the sqlite write-leg baseline, the git
blob-walk baseline (git2 vs shell-out), and the cross-reference with
`v3/docs/*.md`.

## What this proves

- `ctx.put(Effect).await -> Response` with typed responses. No
  `Any`/downcast visible to the op author.
- Adding a new effect kind touches exactly one new file in
  `src/effects/` and one line in `src/effects/mod.rs`. `src/lib.rs`
  never changes.
- Two effect kinds coexist in one registry, monomorphized per kind.
- Unregistered effects fail at `put` time with a clear message.

## What this deliberately skips

- Cursors, pipeline grammar, LSP, diagnostics, mutations.
- Opportunistic batching (passthrough only here; policy layer is a
  followup that lives inside `impl Batcher::run`).
- Full `Op` / `CaptureKind` / `DiagnosticKind` traits. Those are
  scaffolding built atop this foundation; effect dispatch is the
  novel v3 thing and the one worth proving first.
- Cancellation, tracing, stores.

See `../../docs/appendix/v3-plugin-author-surface.md` for the full
authoring surface map this prototype is a slice of.

## Run

```
cd v3/experiments/effect_proof
cargo test
```

Four tests green = associated-type + TypeId registry surface holds.

## Bash drill helpers

Two sourceable files live alongside the crate:

- `plugins.sh` — development drill under `_.sprfv2.expr.plugins.*`
  (test, build, check, clean, watch, list, count, audit, add).
- `helpers.bash` — bench runner + harness-comparison commands under
  `_.sprfv3.bench.*`. Reproduces every measurement in
  `v3/docs/FINDINGS.md` and every comparison table in
  `chat_log/20260420.0.v3-perf-plugin-synthesis-and-library-split.md`.

```
source v3/experiments/effect_proof/helpers.bash
_.sprfv3.bench.help
```

Common drill:

```
_.sprfv3.bench.build                        # three v3 release bins
_.sprfv3.bench.build-probe                  # v2 throughput probe
_.sprfv3.bench.probe-no-prefilter 8 3       # demonstrate the 6x lever
_.sprfv3.bench.head-to-head-ast-grep 8      # probe vs ctx.put parity
_.sprfv3.bench.three-domains                # ast-grep + sqlite + git
```

---

## Plugin drill helper (legacy naming)

`plugins.sh` exposes the drill under `_.sprfv2.expr.plugins.*`.

```
source v3/experiments/effect_proof/plugins.sh
```

Available:

| command | what it does |
|---|---|
| `_.sprfv2.expr.plugins.test`       | `cargo test` |
| `_.sprfv2.expr.plugins.build`      | `cargo build` |
| `_.sprfv2.expr.plugins.check`      | `cargo check` |
| `_.sprfv2.expr.plugins.clean`      | `cargo clean` |
| `_.sprfv2.expr.plugins.watch`      | `cargo watch -x test` (falls back to one-shot) |
| `_.sprfv2.expr.plugins.list`       | list effect files + tests |
| `_.sprfv2.expr.plugins.count`      | LoC per file |
| `_.sprfv2.expr.plugins.audit`      | grep `Any`/`downcast`/`TypeId` in effects+tests (should be clean) |
| `_.sprfv2.expr.plugins.add <name>` | scaffold new effect file + register in `mod.rs` |
| `_.sprfv2.expr.plugins.help`       | this list |

Typical drill after sourcing:

```
_.sprfv2.expr.plugins.add uppercase        # scaffolds src/effects/uppercase.rs
# ... edit uppercase.rs, add a test ...
_.sprfv2.expr.plugins.test
_.sprfv2.expr.plugins.audit                 # src/lib.rs untouched, effects clean
_.sprfv2.expr.plugins.count
```

If the audit stays clean after `.add`, the authoring-surface thesis
holds for that effect.

## File inventory

| file | role |
|---|---|
| `src/lib.rs` | core: `EffectKind`, `Batcher`, `RtCtx`, `put`, registry |
| `src/batchers/passthrough.rs` | no-queue direct compute (rxjs concatMap) |
| `src/batchers/work_steal.rs` | rayon per-item spawn, no queue |
| `src/batchers/bounded_work_steal.rs` | bounded tokio mpsc → rayon pool — the v3 default for CPU effects |
| `src/batchers/bounded_batched.rs` | crossbeam bounded + W workers + max_batch coalesce — for amortizing I/O (sqlite, git writes) |
| `src/effects/read_bytes.rs` | original surface test: path → `Vec<u8>` |
| `src/effects/count_lines.rs` | original surface test: bytes → usize |
| `src/effects/scan_one.rs` | toy effect used by topology tests |
| `tests/surface.rs` | original four surface tests |
| `tests/topology_choice.rs` | six tests including a 2000-concurrent-submitter burst through `BoundedWorkSteal` with cap=16 |
| `src/bin/ast_grep_v3_bench.rs` | plugin-arch perf affirmation against walk-parallel baseline (per-file vs batch emission) |
| `src/bin/sqlite_v3_bench.rs` | sqlite write-leg baseline (extract + batched insert pipeline; `--scan-only`, `--skip-insert` isolate stages) |
| `src/bin/git_tree_bench.rs` | git2 vs shell-out baseline for HEAD blob walk |
| `FINDINGS.md` | teaching document (lives in `v3/docs/`) |
| `PRIOR_ART.md` | survey of 10 typed-effect-dispatcher projects (lives in `v3/docs/`) |
| `helpers.bash` | bench + harness-comparison functions under `_.sprfv3.bench.*` |
| `plugins.sh` | development drill functions under `_.sprfv2.expr.plugins.*` |

Framework : ops LoC ratio ≈ 90 : 60. Each new effect adds ~30 LoC
and zero framework edits. The ratio is the thesis.

## To extend and re-validate

Add a third effect, e.g. `Uppercase { content: Vec<u8> } -> Vec<u8>`:

1. `touch src/effects/uppercase.rs` and implement `EffectKind` +
   `Batcher` in that one file.
2. Add `pub mod uppercase;` to `src/effects/mod.rs`.
3. Add a test in `tests/surface.rs` that calls
   `ctx.put(Uppercase { content }).await` and asserts the result.
4. Run `cargo test`.

Verify `src/lib.rs` was never opened. If step 4 green and the audit
holds, v3's authoring surface is validated at the smallest possible
scale and you can pull the trigger.

## Remaining centralizations this prototype does not address

See `v3-plugin-author-surface.md` Tier 1 (unavoidable), Tier 2
(collapsible — the real v3 work), Tier 3 (structural invariants),
Tier 4 (policy-deferred). This prototype is the Tier 2 foundation.
Tier 1 (the one dyn cell inside the registry, captures/slots
heterogeneous storage) is inherent and present here at its minimum.
