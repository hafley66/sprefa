---
name: sprefa-v5-no-magic-rels
description: The banned "magic rel" pattern in the v5 dl engine (~/projects/sprefa) and how to avoid it. Read before making the engine read any relation by a literal string name.
---

# No magic rels

A **magic rel** is a relation the engine special-cases by a **literal string
name** in its own Rust source — `eng.rels.get("scip_want")`,
`FROM rel_effect_cmd` — where nothing in `rel_catalog` tells a program author the
name is critical. That is a real API surface (deriving `scip_want(repo)` spawns
SCIP indexers) but an invisible one: you learn it exists only by reading engine
source or tripping over it.

**The pattern is banned.** Every name the engine reads by literal string must be
a **catalogued relation** (in `rel_catalog`). The rail `.dl/magic-rel-audit.dl`
enforces it: `dl --check` exits 2 on any un-catalogued literal name, `dl --lsp`
squiggles the source line.

## The key idea: demand/overlay conventions are just builtin SINKS

The engine already has a first-class category for "a user heads a relation from a
rule, and the rows drive engine behavior": the **builtin sink**, e.g. `diag`
(rows become editor squiggles) and `repo` (rows trigger a clone). They are:

- **pre-declared** by the engine (`declare_builtins` → a `RelDecl`), so they show
  in `rel_catalog` / `dl docs relations`;
- **head-written** by rules — you write `diag(...) <- ...`, you do NOT
  `rel`-declare them;
- **reserved** against a `rel` re-declaration (heading is the only way in).

The four demand/overlay conventions are exactly this shape and now live here:
`scip_want`, `rev_cmp_want`, `def_target`, `effect_cmd` — declared in
`demand_rel_decls()` and reserved via `DEMAND_RELS` in `src/engine/mod.rs`,
group `demand`. The engine reading them by name is reading a catalogued builtin,
not magic. See `docs/reference/magic-rels.md`.

## Adding a new demand/overlay convention

Do NOT invent a hidden name the engine matches by string. Make it a builtin sink,
mirroring the existing four:

1. Add a `RelDecl` to `demand_rel_decls()` (`src/engine/mod.rs`), group `demand`,
   with a one-line `doc` describing what heading it does.
2. Add the name to the `DEMAND_RELS` reserved array (so a user `rel <name>` decl
   bails with "head it directly, like diag/repo").
3. Read it in the engine where the behavior fires (a literal
   `eng.rels.get("<name>")` is now fine — it names a catalogued builtin).
4. In programs, HEAD it (`<name>(...) <- ...`); never `rel`-declare it.

That is the whole ceremony. The name is visible in the catalog, the rail is
green, and there is no side channel.

## Fixing a rail finding

`dl .dl/magic-rel-audit.dl --root . --check` (or bare `dl --check --root .`)
reports `magic-rel-unregistered` at the offending `file:line`. To clear it, give
the name a `RelDecl` so it is catalogued:

- an engine-owned, engine-filled relation → a normal builtin (`sprefa-v5-new-builtin-rel`);
- a user-headed demand/overlay sink → the `demand_rel_decls()` + `DEMAND_RELS`
  steps above.

Never silence the finding by narrowing the rail's regex or muting the code. The
magic set may only shrink or become catalogued.

## Anchors

- The four sinks: `demand_rel_decls()` + `DEMAND_RELS` in `src/engine/mod.rs`
  (mirrors `diag_rel_decls` / `DIAG_RELS`).
- The rail: `.dl/magic-rel-audit.dl` (scans `src/**/*.rs` for `rels.get("...")`
  / `FROM rel_...`, anti-joins `rel_catalog`).
- The doc: `docs/reference/magic-rels.md` (generated from `rel_catalog` group
  `demand` by `examples/gen-reference.dl`).
- Read sites: `src/rels/scip.rs` (scip_want), `src/rels/git.rs` (rev_cmp_want),
  `src/engine/mod.rs` (def_target), `src/lib.rs` + `src/daemon.rs` (effect_cmd).
- Adding a plain builtin instead: `sprefa-v5-new-builtin-rel`.
