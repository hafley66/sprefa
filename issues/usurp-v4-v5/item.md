---
created: 2026-08-21
updated: 2026-08-21
type: epic
owner: chris
status: open
priority: high
---

# Usurp v4 and v5: extraction parity, rail census, retirement

## Description

User ask, 2026-08-21: "i just want all that v4/v5 to be usurped and its taking
ages. this extract shit should be at parity with all of v5 and below's
extraction capabilities." Standing decision (CLAUDE.md): "I DO NOT WANT TO RUN
V5 ANYTHING ANYMORE."

This epic carries the census that says what is left, and the retirement order.

## Deliverables in this epic

| doc | what it answers |
|---|---|
| `docs/v5-extraction-parity.md` | one row per v5 capability, v6 equivalent or the named gap |
| `docs/v5-rail-census.md` | every v5 `.dl` file bucketed: ported / portable / blocked / dead |
| `plans/2026-08-21-v5-retirement.PLAN.md` | what must exist before `src/` moves to an archive, in order |
| `plans/2026-08-21-v5-retirement.PLAN.visual.human.unga.md` | the same, drawn, no citations |

## Relationship to @extract-port-closeout

`@extract-port-closeout` is the 16-row RECORD-LEVEL census of the extractor
crate (does v6 emit v5's facts). It is nearly finished: 13 of 16 rows closed.

This epic is the DOOR-LEVEL census (can a dl6 program on the Rust door reach
those facts) plus the rail census and the retirement order. The two do not
overlap: every record-level row can be green while the door still cannot ask
for it, and that is exactly what the matrix found.

## Children

Filed against this epic, one per door-level gap:

- `@dl6-no-text-extraction-door` — the headline. No dl6 spelling for v5's
  `match_line` or `match_ast`.
- `@dl6-cfg-family-unlinked` — `--family cfg` is CLI-reachable, in-process
  unreachable.
- `@dl6-scip-facts-door` — v5's ten scip rels are eight on the dl6 door and the
  passthrough that answers the other two is not in-process either.
- `@dl6-deps-package-door` — `--deps`, `--scip-deps`, `--package-deps` are
  CLI-only; v5's module/crate edge rels have no dl6 spelling.
- `@v5-rail-eprintln-blocked` — the `no-new-eprintln` rail port, blocked on the
  first child.
