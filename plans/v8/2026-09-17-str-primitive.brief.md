# str primitive brief

Lane branch `feat/str-primitive`. Base `origin/main` at spawn (coordinator
states the sha). First action `git merge --ff-only <sha>`; failure = stop and
report. Preset sonnet.

Decision: `AGENTS.md` row "The string primitive is `str`, never `text`."
One rename, nothing else. No design, no new rules, no book prose.

## Rename table

| site | today | after |
|---|---|---|
| `src/_3_check/_5_kernel.rs:41` `primitive_name` | `"text"` | `"str"` |
| `src/_3_check/_5_kernel.rs:121` kernel node list | `"text"` | `"str"` |
| `src/_3_check/_5_kernel.rs:149` `primitive_ref(u, "text")` | `text` | `str` |
| `src/_2_lower/_8_express.rs:236` `primitive_type` | `"text"` | `"str"` |
| `src/_2_lower/_8_express.rs:293` `Term::Str(_) =>` | `"text"` | `"str"` |
| `src/_2_lower/_6_partial.rs:244` `Term::Str(_) =>` | `"text"` | `"str"` |
| `src/_6_eval/_6_json.rs:179` `value_node(u, "text", ..)` | `"text"` | `"str"` |
| `src/_6_eval/_6_json.rs:144` doc comment | `(text "x")` | `(str "x")` |
| `src/_4_comptime/_0_load/_9_openapi.rs` schema `string` mapping | `text` | `str` (grep `"text"` in the file) |
| `prelude/*.dl7`, `std/*.dl7`, `macrotime/*.dl7` | `text` as a type or constructor | `str` |
| `fixtures/**/*.dl7` | same | same |
| `book/src/**/*.md` | `text` as a type or constructor in `.dl7` blocks and tables | `str` |
| `oracle/**` | frozen `text` rows | refreeze, never hand-edit |

NOT renamed: `_1_macrotime/_2_protocol.rs:330-344` (`syntax_atom` payload slot
named `text`, a protocol slot not the primitive); `_9_runtime/_1_sqlite.rs:575`
(SQL column name); `_0_load/_3_wire.rs:339-341`, `_5_graph.rs:196,340`,
`_6_source.rs:445` (`text(..)` const wrapper for loaded facts); the English
word "text" in comments and prose. Grep each hit and classify before editing:
a hit is renamed only when it names the primitive type or its constructor.

## Steps

| # | step | receipt |
|---|---|---|
| 1 | `grep -rnE "\btext\b" prelude std macrotime fixtures --include=*.dl7 \| wc -l` on base | count |
| 2 | rs sites from the table; `cargo build` | build tail |
| 3 | dl7 files; `dl8 compile fixtures/hosts/4_fs_json.dl7` rc=0 | rc + row count |
| 4 | `bash oracle/refreeze.sh`; commit the regolden ALONE, message states `git diff --stat oracle \| tail -1` | stat line |
| 5 | book `.dl7` blocks and tables; `mdbook build book` rc=0 | build tail |
| 6 | `grep -rnE "\btext\b" prelude std macrotime fixtures --include=*.dl7` | 0 hits, or each remaining hit listed with why it stays |
| 7 | `grep -rn "\"text\"" src` | only the NOT-renamed rows above |
| 8 | full gate | summary lines |

Commit after every step; subject names the step.

## Files forbidden

Any `src/**` line outside the rename table. Any new rule, any new fixture,
any new book section. `src/_2_lower/_2_declare.rs`, `src/_3_check/_0_api.rs`,
`_2_resolve.rs`, `_7_resolved.rs`, `_6_strata.rs` (lane `feat/keys-userland-2`
owns them; a rename hit there is a stop-and-report line, not an edit).

## Gate

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"
cargo test --test _8_compile_oracle --test _3_lower_oracle --test _22_book 2>&1 | grep -E "^test |test result"
git diff --stat origin/main...HEAD
```

Known red on the base: `.github/CI-KNOWN-RED.md` dl8 battery section
(6 nested-worktree legs). Measure each red leg three times.

## Laws

`plans/v8/2026-09-17-import-arc.brief.md` section 8. Plus: no long-form
markdown; the PR body is the steps table with receipts pasted, under 20 lines
of prose.

## Report

```bash
boop beep --no-wait --as feat-str-primitive sprefa-coordinator "str primitive: PR #<n>, oracle delta <stat>, battery <pass>/<total>, red: <list or none>"
```
