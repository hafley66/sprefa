---
created: 2026-08-20
updated: 2026-08-20
type: bug
status: testing
priority: high
labels: [ci]
---

# comment-budget rail: golden .dl6 fails ir_version load at HEAD, rail red on empty index

## Description

## Comments

### 2026-08-20T16:05:19Z · @jsonschema-rail-fix

Fixed on fix/jsonschema-loop-and-rail, PR https://github.com/hafley66/sprefa/pull/385. Commit 484f8fb7f, base 3993e44aa.

Re-emitting the rail's golden would not have fixed it: the rail POSTs a .dl6 SOURCE and serve/0_compile.ts compiles it on demand, so the version-none program was minted at request time. 65607a8d5 had deleted ir_version/1 and both emission sites from emit_ts.pl and emit_rust.pl, and IR_VERSION plus try_from_json from sprefa-engine-rs/src/program.rs, while leaving every consumer (irVersion.ts, dl6_build.rs, build_template/main.rs) standing.

Restored in both emitters and the Rust runtime. Guard added: plunit incremental_mode:both_doors_stamp_the_ir_version_the_runtimes_interpret drives BOTH emitters and reads the stamp out of the emitted text (the existing tests pin only the checker and the number's agreement).

Probes: `git commit --allow-empty` with the hook on returns rc=0 (`graded files 0`); a staged 4-line comment block reds it rc=2 with `probeViolation.ts:1-4 (4 comment lines)`. Both probe artifacts removed.
