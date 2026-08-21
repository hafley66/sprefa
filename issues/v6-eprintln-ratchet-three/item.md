---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

Three `eprintln!` statements in `v6/sprefa-*/src/*.rs` survive the waiver rule.
CLAUDE.md: "eprintln never comes back. No `eprintln!` in `src/**`; `tracing`
only. Rare CLI-UX lines carry `@eprintln-ok`."

Measured by the ported rail (`just no-new-eprintln`, `@v5-rail-eprintln-ported`),
not by grep. The title says five because a grep with v5's line rule says five;
the rail says three and the rail is right, for the reason below.

## The three, measured 2026-08-21

```
== rail_eprintln_counted (unwaived sites, against the baseline) ==
  v6/sprefa-extract/src/bin/extract.rs  @10175
  v6/sprefa-extract/src/bin/extract.rs  @20088
  v6/sprefa-store/src/engine.rs  @3861
hits=17 waived=14 new=0 exceeded=0
```

| site | what it prints | verdict |
|---|---|---|
| `v6/sprefa-extract/src/bin/extract.rs:281` | top-level CLI error before exit | waive: CLI-UX contract, add the comment |
| `v6/sprefa-extract/src/bin/extract.rs:592` | top-level CLI error before exit | waive: CLI-UX contract, add the comment |
| `v6/sprefa-store/src/engine.rs:75` | `[cascade] {ms} {head}` timing line | **convert to `tracing`**: machinery narration in a library, not a CLI-UX contract |

Both `extract.rs` sites are inside the baseline row
(`eprintln_baseline('v6/sprefa-extract/src/bin/extract.rs', 2)`), so the rail is
green today; lowering that row to 0 after the comments land is the ratchet step.

## Why grep says five and the rail says three

Two `emit_rust_harness.rs` sites are MULTI-LINE `eprintln!(` calls whose
`@eprintln-ok` marker sits on the closing `);` line, four lines below the
`eprintln!` token:

```rust
eprintln!(
    "--arrive {rel} carries {} values and the rel declares {} columns",
    cells.len(),
    types.len()
); // @eprintln-ok CLI usage
```

v5's window is `[line-1, line]` against the token line, so v5's own rail would
have reported these too. The v6 rail reads the statement's next sibling and
waives them correctly. `v6/dl/rails/fixtures/eprintln/multiline_waiver.rs` pins
the case.

## Gate

```bash
cd v6 && timeout 600 just no-new-eprintln          # NO-NEW-EPRINTLN OK findings=4
bash v6/dl/rails/no-new-eprintln-rail.sh . 'v6/sprefa-*/src/*.rs'
cd v6/sprefa-store && cargo test --release
```

After the fix the baseline rows in `v6/dl/rails/no-new-eprintln-rail.dl6` both
go to 0 and get deleted, per the rail's own law.
