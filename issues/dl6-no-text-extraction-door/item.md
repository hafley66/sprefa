---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

A `.dl6` program on the Rust door cannot read source TEXT. It can read node
kinds and byte spans, and it can read the few fields that carry text as their
own column (`callee`, `name`, `doc.text`, `unresolved.detail`), but there is no
way to ask "what are the bytes at this span" and no way to run a pattern.

This is the single highest-value row in `docs/v5-extraction-parity.md`: it
blocks 68 of the 195 v5 rails.

## Receipts

| fact | receipt |
|---|---|
| the host command parser accepts two flags and nothing else | `v6/sprefa-engine-rs/src/hosts.rs:896-914` |
| an unrecognised flag is a named stop | `hosts.rs:907-910` "flag `{token}` is not linked in-process" |
| the ast-grep pattern door exists on the CLI | `v6/sprefa-extract/src/bin/extract.rs:144-171` (`--ast-pattern`, `--ast-selector`, `--ast-capture`) |
| its record already exists on the wire | `v6/sprefa-extract/src/schema.rs:46` `record=capture` with `text`, `start`, `end`, `match_start`, `match_end` |
| it is already CLI-tested | `v6/sprefa-extract/tests/3_ast_pattern_cli.rs`, `tests/9_query_cli.rs` |
| cst nodes carry no text | probed 2026-08-21: `extract --family cst probe.rs` returns `{"kind":"identifier","name":null}` |
| `ts_query/1` compiles but has no executor | `v6/prolog/compile/registry.pl:198` is `live`; `hosts.rs:41-59` `executor_for` has no `tree_sitter` arm |
| `sg_pattern/3` is refused | `registry.pl:199` `value(refuse(slot_sg_metavariable_semantics))` |
| v5's two ops | `src/engine/decls.rs:225` `match_line`, `:228` `match_ast` |

## The three arms, cheapest first

**A. link `--ast-pattern` (answers `match_ast` / `sg`, 28 rails).** Three
branches in `SprefaExtractExecutor::run`: collect `--ast-pattern`,
`--ast-selector` and `--ast-capture` into the `query_patterns` call the CLI
already makes, and emit `record=capture` rows. A host name and an input
contract in `registry.pl` beside `extract`.

**B. link `ts_query/1` (answers `ast`, 13 rails).** An `executor_for` arm for
the `tree_sitter` execution name. The compile path is already live and its
demand/response tables already emit (`plunit_tests.pl:3611-3613`).

**C. a line/text plane (answers `match_line` / `match` / `comment`, 67 rails).**
The one that needs a design call, because it is a new record shape. Sketch:

```
record=line   family=text   path=<string>  line=<u32>  start=<u32>  end=<u32>  text=<string>
```

emitted only for lines matching a caller-supplied pattern, behind
`--family text --line-pattern ID=RE`, so a whole-file text dump is never on the
wire. `--occurrence-text`'s existing shape is the precedent: text rides a flag,
never the default.

`ast_yaml` (5 rails) needs the `sg_pattern/3` metavariable-slot semantics
decided first. LANG DESIGN, Chris in the room.

## Gate

```bash
cd v6/sprefa-extract && timeout 900 cargo test --release --features cli
bash v6/dl/rails/recompute-guard-rail.sh
# plus: one new dl6 fixture per arm that declares the host and reads a row
```
