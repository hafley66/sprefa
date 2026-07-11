---
name: sprf-write-plan
description: How to author a plan doc in ~/projects/sprefa/plans/ — naming, required sections, the <!-- todo(category) --> comment convention, and the PLANS.md regen command. Load before writing or editing a plan doc.
---

# Writing a plan doc

## Naming

`plans/YYYY-MM-DD-<slug>.md` — date the plan was written, kebab-case slug.
One plan per arc; a follow-on arc gets its own dated file, not an edit.

## Required sections

```markdown
# <Title>

## Context
Why now, what exists, what broke or is missing. Cite files/lines/commits.

## Decisions
The choices made, with the rejected alternatives named (one line each).

## Verification
How the arc proves itself: tests to add, gates to run, measured numbers.

## Staffing
Who implements (agent model, worktree y/n), base SHA, suite budget.
```

Extra sections (design, sequencing, research facts) are fine between Context
and Verification.

## Open-item convention: todo comments

One markdown comment per open item, anywhere in the doc:

```markdown
<!-- todo(category): text -->
```

- `category` is an open set; starter vocabulary: `perf | bug | feature |
  docs | triage | decision`.
- Multi-line comment bodies are allowed; the index flattens whitespace.
- Text must be a faithful statement of the item (quote the plan's own words),
  because PLANS.md renders it verbatim.
- Remove or reword the comment when the item closes, then regen.

These are extracted by sprefa itself (`comment_node` over the markdown
grammar), so a fenced code sample showing the convention never becomes a row.

Rust source comments use the same convention for code debt:

```rust
// todo(category): text
```

The category is one of `perf | bug | feature | docs | triage | decision`.
`examples/gen-plans-index.dl` scans `src/**/*.rs` and renders these into the
PLANS.md “By code file” zone as `category src/path.rs:LINE — text`. Bare
uppercase `TODO`/`FIXME` comments are counted in one untriaged-debt row.

## Regenerating PLANS.md

```sh
dl examples/gen-plans-index.dl            # rewrites the PLANS.md zones
dl examples/gen-plans-index.dl --check    # drift rail; exit 2 when stale
```

PLANS.md (repo root) is the reverse index: by-category and by-plan zones
between `BEGIN:/END:` markers, hand-owned prose outside them. Never hand-edit
inside the markers. Adding/removing a todo comment without a regen fails the
`--check` rail (`plans-index-drift` / `plans-index-orphan`).
