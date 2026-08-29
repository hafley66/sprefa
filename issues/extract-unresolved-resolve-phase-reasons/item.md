---
created: 2026-08-29
updated: 2026-08-29
type: chore
status: open
priority: low
---

## Description

`UnresolvedReason` (`v6/sprefa-extract/src/types.rs`) carried the closed v5
vocabulary of three phase-1 slugs, with a doc comment requiring an issue row
for any addition. Two were added for the rust crawl's kink 7
(`plans/extract-crawl-2026-08-29/rust.REPORT.md` section 7):

| slug | decided by | meaning |
|---|---|---|
| `no_corpus_def` | a `Resolve<CallF>` arm | no corpus def bears the callee's name: std, a dependency, a macro, or a builtin. No edge is the right answer. |
| `ambiguous` | a `Resolve<CallF>` arm | the corpus defines the name and this tier does not settle which one is meant. |

Both are corpus-wide facts. A per-file phase-1 walk cannot decide either, so
they reach the wire through the `ResolveArm.drops` seat
(`v6/sprefa-extract/src/project.rs`) rather than `CallFAux.unresolved`. The
`unresolved` record gained an optional `path` field for the same reason: a
`--resolve` run spans files and a bare span does not say which one.

Emitted by the rust arm only (`RustSource::call_drops`). Every other arm sets
`drops: None` and its output is byte-identical to the era before the channel.

## Acceptance Criteria

- [x] The two variants carry `as_str` tags `no_corpus_def` and `ambiguous`.
- [x] `src/schema.rs` documents both under `unresolved reason`.
- [x] `tests/52_rust_crawl_kinks.rs` pins the reason per site and the COUNT
      rail `rows == sites - edges`.
- [ ] A second language arm adopts the channel (go is the obvious next).
