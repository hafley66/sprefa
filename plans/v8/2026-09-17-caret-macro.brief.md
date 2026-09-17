# Caret macro brief

Lane branch `feat/caret-macro`. Base: `origin/main` at spawn (coordinator states
the sha). Runs after `feat/import-std-alias` merges; do not start before.
First action `git merge --ff-only <sha>`; failure = stop and report.

## Contents

1. Goal
2. Decisions (read `AGENTS.md:76-107` first)
3. Before / after
4. Reader fact
5. Files owned
6. Files forbidden
7. Steps and receipts
8. Gate
9. Laws
10. Report

## 1. Goal

`^` is a macrotime rewrite in `macrotime/1_caret.dl7`. It reaches lower as
plain `:` rows. Rust changes: one token class in the reader, nothing else.

## 2. Decisions

| row | decision |
|---|---|
| `AGENTS.md:103` | `^x` marks the subterm a form evaluates to; macrotime rewrite; one `^` per level, second is `ambiguous_return` |
| `:105` | `(^Name: T): (body)` reparents every body edge under `Name`'s own node id; no minted node, no curry, no `return`. `(: ^b str)` inside a product collects into one `(: return b)` member edge |
| `:107` | one `^` per level for now; call-site selection is the later end state (plan section 8) |
| `:104` | keys are userland; this lane does not touch key sets |

## 3. Before / after

Reparent. Before:

```
(^User: (* (: a text))): (
  (: greet (<- (greet ?User ?Salutation) (User ?User ?Name) (str.cons "hi " ?Name ?Salutation)))
)
```

After `dl8 expand`:

```
(: User (* (: a text)))
(: User greet (<- (greet ?User ?Salutation) (User ?User ?Name) (str.cons "hi " ?Name ?Salutation)))
```

Return collect. Before:

```
(: Pick (* (: source type) (: names any) (: ^result type)))
```

After:

```
(: Pick (* (: source type) (: names any) (: result type) (: return result)))
```

Two carets at one level. Before:

```
(: Bad (* (: ^a int) (: ^b int)))
```

After: no expansion; one macrotime diagnostic row `ambiguous_return(<form>)`.

## 4. Reader fact

`(User: X)` already reads as `(: User X)`: `src/_0_read/_1_tokens.rs:32-36`
accepts a `name:` suffix token. `^User` is a reader diagnostic today
(`valid_atom`, `:28-38`, no `^` arm). The one Rust change: `valid_atom` accepts
a `^` prefix on an identifier, with or without the `:` suffix. The macro then
matches `(syntax_atom ?Node ?Text)` with `(str.cons "^" ?Name ?Text)`.

The colon-after-edge form `(edge): (body)` is a plain 3-item form for the
reader; verify with `dl8 read` on the before-form above and paste the output.
If it does not read, stop and report the reader line.

## 5. Files owned

| file | change |
|---|---|
| `src/_0_read/_1_tokens.rs:28-38` | `^` prefix arm in `valid_atom` |
| `macrotime/1_caret.dl7` | new: claim, reparent, return collect, `ambiguous_return` |
| `src/_8_driver/_0_read.rs:18` | `MACROTIME` gains the new file |
| `oracle/macrotime/` | new expansion cases for the three before/after pairs, frozen through `oracle/refreeze.py` |
| `tests/_2_macrotime_oracle.rs` | picks up the new cases |
| `plans/v8/probes/2026-09-17-caret-*.dl7` | the three probes |
| `book/src/8_macros.md` | one section: `^`, before/after via `mdbook-cmdrun` of `dl8 expand` |

## 6. Files forbidden

`src/_2_lower/**`, `src/_3_check/**`, `src/_4_comptime/**`, `src/_6_eval/**`,
`prelude/**`, `std/**`. A `^` that needs lower to change is a stop-and-report.

## 7. Steps

| # | step | receipt |
|---|---|---|
| 1 | `dl8 read` the three before-forms on the base sha; paste diagnostics | reader output |
| 2 | `valid_atom` arm; `dl8 read` rc=0 on all three | pasted forms |
| 3 | claim + reparent rules; `dl8 expand` on probe 1 prints the after-form | pasted expansion |
| 4 | return collect; probe 2 | pasted expansion |
| 5 | `ambiguous_return`; probe 3 | pasted diagnostic row |
| 6 | `dl8 compile` probe 1 and a goal `(User.greet ?U ?S)` resolves rc=0 | `compiler_rows` count |
| 7 | oracle cases frozen; `cargo test --test _2_macrotime_oracle` | PASS lines |
| 8 | book section; `mdbook build book` rc=0 | build tail |
| 9 | full gate | summary lines |

Commit after every step; message names the step.

## 8. Gate

```bash
cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
cargo test --test _2_macrotime_oracle --test _22_book 2>&1 | grep -E "^test |test result"
grep -rn "\^" src/_2_lower src/_3_check | wc -l    # expect 0 new hits versus base
git diff --stat origin/main...HEAD
```

Known red on the base outside this lane: `.github/CI-KNOWN-RED.md`, dl8
battery section. Measure each red leg three times.

## 9. Laws

Same as `plans/v8/2026-09-17-import-arc.brief.md` section 8. Plus: every
macro rule in `1_caret.dl7` carries a TypeScript-shaped row signature
comment like `macrotime/0_standard.dl7:4-16`; dl variables descriptive.

## 10. Report

```bash
boop beep --no-wait --as <lane> sprefa-coordinator "caret macro: PR #<n>, macrotime oracle <pass>/<total>, battery <pass>/<total>, red: <list or none>"
```
