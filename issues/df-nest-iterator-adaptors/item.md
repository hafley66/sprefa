---
created: 2026-09-08
updated: 2026-09-08
type: improvement
status: open
priority: normal
related: ['@extract-reach-entry-sink']
labels:
- pkg:extract
- size:med
---

# df_nest: closures passed to iterator adaptors count as loop bodies

## Description

## Ask

`df_nest` only recognises syntactic loops (`for_expression`, `loop_expression`, `while_expression`). A call inside a closure passed to `map`, `for_each`, `filter_map`, `flat_map`, `try_for_each`, `fold`, `retain` (Rust), `.forEach`/`.map`/`.flatMap` (ts), `range`/`for` (go) runs per element too and is invisible to the loop-reach join.

## Receipt

Same store read, `boop-store/src/_0_session_graph.rs`: the `sessions.iter().map(|session| ...)` chain at 384-388 is not a loop to `df_nest`. Today it does no SQL, so nothing is missed yet; the first `.map(|x| store.f(x))` would be.

## Shape

- Add `df_nest` rows for sites inside a `lambda` node that is an argument to a site whose callee is in a per-language iterator vocabulary. `collection` = the receiver expression text, `depth` counts nested adaptors.
- Vocabulary lives beside `kind_role` rows so a language without it emits nothing rather than guessing.

## Acceptance Criteria

- [ ] rust: `xs.iter().map(|x| f(x)).collect()` yields `df_nest` for the `f` site with `collection = "xs.iter()"`
- [ ] ts: `xs.map(x => f(x))` same
- [ ] a closure stored in a `let` and called once does not yield a row
