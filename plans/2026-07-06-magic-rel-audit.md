# Magic-rel audit + de-magic plan

## STATUS: LANDED (2026-07-06, local uncommitted) — Option 1 (eliminate, not document)

The pattern is retired, not merely catalogued. The four demand/overlay
conventions are now first-class **builtin SINK relations**, exactly like `diag`
and `repo`: pre-declared, head-written from a rule, reserved against a `rel`
re-declaration, and carried in `rel_catalog` (group `demand`). The engine
reading them by name is now reading a catalogued builtin — no magic.

- **Promotion** `src/engine/mod.rs`: `demand_rel_decls()` (RelDecls for
  `scip_want`/`rev_cmp_want`/`def_target`/`effect_cmd`, group `demand`) +
  `DEMAND_RELS` reserved array + a bail ("head it directly, like diag/repo") +
  chained into `all_builtin_decls` and `declare_builtins`. Mirrors
  `diag_rel_decls`/`DIAG_RELS` one-for-one.
- **Consumers migrated**: every `rel scip_want(...)` / `rel rev_cmp_want(...)` /
  `rel def_target(...)` / `rel effect_cmd(...)` decl dropped (they now head the
  pre-declared builtin) — 4 examples/.dl + 6 test programs.
- **`special_rel` DELETED**: `src/rels/special.rs` removed, `SpecialKind`
  unwired, the read-site consts reverted to plain literals (now clean because
  the names are catalogued). The earlier registry approach (documenting the
  magic) was replaced by eliminating it.
- **Dogfood rail** `.dl/magic-rel-audit.dl`: scans `src/**/*.rs` for
  `rels.get("<name>")` / `FROM rel_<name>`, anti-joins **`rel_catalog`** (one
  known-set now), `magic-rel-unregistered` `--check` exit 2. In CI bare-check +
  the PostToolUse hook. Regression test `tests/it/magic_rel_audit.rs` (3) proves
  it fires on a planted name and stays green on catalogued ones.
- **Skill** `assets/sprefa-v5-no-magic-rels.skill.md` (wired by `dl setup
  --project`) + **subagent** `.claude/agents/magic-rel-auditor.md`: adding a new
  demand convention = a RelDecl in `demand_rel_decls()` + `DEMAND_RELS`, never a
  hidden name.
- **Docs** `docs/reference/magic-rels.md` generated from `rel_catalog` group
  `demand` by `gen-reference.dl`; the four also appear in `relations.md`.

Suites: lib 219, it 530, both green; binary reinstalled (installed `dl` has the
`demand` catalog group; the hook needs it or the read sites + un-declared heads
would break). No `@` qualifier, no new syntax, no registry — the answer to "how
do we not do `@`" was "they were always just builtin sinks we forgot to
declare."

### Design correction vs the inventory below

The M1 note "`scip_want`/`rev_cmp_want` have a `reserved_msg` but no `RelDecl` —
reserved yet undocumented" was WRONG. Those `reserved_msg`s belong to the
OUTPUT rels (`scip_def…`, `rev_behind`). The `*_want`/`def_target`/`effect_cmd`
inputs were never reserved — they were UNRESERVED conventions the user heads.
The fix was NOT a separate registry (my first pass, `special_rel`, since
deleted) but promoting them to pre-declared, head-writable builtin sinks — the
`repo` precedent: `repo` is a catalogued builtin (group `core`) users head to
trigger cloning. The four are the same shape.

## The problem

Some relations aren't "stored facts" — the engine special-cases them **by
literal string name**, and deriving/reading them runs side effects (spawn a
subprocess, clone a repo, load an index). The worst offenders are **not in
`rel_catalog`**, so `dl docs relations` never mentions them. You only learn they
exist by reading source or tripping over them. That is the "insane magic".

`scip_want` is the archetype: you write `scip_want(repo) <- ...` in your program;
post-fixpoint the engine does `eng.rels.get("scip_want")` (`src/rels/scip.rs:155`)
and runs `scip_setup::ensure_index` per row — spawning indexers, writing
`index.scip`, merging, loading. Nothing in the catalog tells you the name
`scip_want` is load-bearing.

### Naming model (why this is possible)

Rel **names are a single flat global namespace** — one name → one rel, no
filepath qualifier. (The `file::kind::name` / `file::line::kind` disambiguation
you're thinking of is at the **row/sym level** — `call_def.sym`, `df_node.id` —
to separate same-named symbols across files. That is orthogonal to rel *names*,
which are global.) So the engine can reserve a bare name like `scip_want` as an
API, and a `reserved_msg` guard bails if you `rel` a name it owns. The magic
names just aren't all registered/documented the same way.

## Inventory — every magic mechanism

### M1 — Demand conventions (INVISIBLE: string-matched, NOT in catalog)

You derive the rel; the engine consumes it by exact name to trigger IO. These are
the ones that surprise.

| rel | trigger site | side effect | in catalog? |
|---|---|---|---|
| `scip_want(repo)` | `src/rels/scip.rs:155` | `ensure_index` per repo — spawn scip indexers, write `index.scip`, merge, load | **NO** |
| `rev_cmp_want(repo, refname, upstream)` | `src/rels/git.rs:265` | `git rev-list` subprocess per row → fills `rev_behind` | **NO** |
| `def_target(...)` | `src/engine/mod.rs:2026` | LSP go-to-def resolution reads a user rel by name | **NO** |
| `effect_cmd(kind, template)` | `src/lib.rs:439`, `src/daemon.rs:367` | dynamic effect-template overlay read at drain (`rel_effect_cmd`) | **NO** |

`scip_want` and `rev_cmp_want` have a `reserved_msg` (so `rel scip_want` bails)
but still no `RelDecl` — reserved yet undocumented. `def_target`/`effect_cmd`
aren't even reserved.

### M2 — Writable sinks, not rule-headed (written out-of-band; bail if headed)

| rel | writer | note |
|---|---|---|
| `diag_mute(code)` | LSP `dl.toggleDiagCode` (`mod.rs:3186`) | in catalog, guarded |
| `hook_event(...)` | `dl --hook` feed (`tick.rs:365`) | in catalog |
| `repo(slug, root, url)` | rule-headed SINK, triggers `ensure_cloned` (clone) | in catalog |
| `mcp_request` / `mcp_retire` | MCP port pump | port rels |

### M3 — Nondeterministic / wall-clock (present every tick, not fact-triggered)

`clock(secs, bucket)`, `every(secs)` — read `SystemTime::now`. In catalog (group
`clock`) but their per-tick, time-varying nature is easy to miss.

### M4 — Git subprocess on refresh (shell out each relevant tick)

`changed`, `changed_line`, `created`, `head`, `git_ref` — `git` subprocess in
`src/rels/git.rs`. In catalog.

### M5 — Corpus-parse extraction families (run syn/oxc/tree-sitter; digest-gated)

~40 rels: `type_*`, `call_*`, `df_*`, `module_*`, `crate_edge`, `doc_*`,
`comment_node`, `node`/`child`, `scip_def`/`scip_ref`/`scip_edge`. In catalog.
Magic = a bare `scan` silently triggers a full parse of the corpus to fill these.

### M6 — Self-describing meta (query the engine itself)

`rel_catalog`, `fn_catalog`, `op_catalog`, `dl_diag`, `rel_count`, `stmt_ms`. In
catalog.

**Verdict:** M3–M6 are discoverable today (catalogued, grouped). **M1 is the
real problem** (invisible), and M2 is a secondary one (write-only rels that look
derivable). The fix targets M1/M2 and makes the whole set self-declaring.

## The fix — one registry, catalogued, rail-enforced

Goal: **no rel the engine special-cases by name is invisible.** Every one is in
`rel_catalog`, carries a machine-readable nature tag, and a `--check` rail fails
CI if a new string-matched rel is added without registering.

### Step 1 — `SpecialRel` registry (`src/rels/special.rs`, new)

One table of every rel matched by literal string OR written out-of-band:

```rust
pub enum RelNature { Plain, Demand, Sink, Clock, GitIo, Effect, Extract, Meta }
pub struct SpecialRel {
    pub name: &'static str,
    pub cols: &'static [(&'static str, &'static str)], // (name, brand)
    pub nature: RelNature,
    pub trigger: &'static str,  // "you derive it; engine runs ensure_index per row"
    pub doc: &'static str,
}
pub fn special_rels() -> &'static [SpecialRel];  // scip_want, rev_cmp_want, def_target, effect_cmd, ...
```

### Step 2 — route the 4 literal lookups through the registry

Replace each raw `eng.rels.get("scip_want")` / `"rev_cmp_want"` / `"def_target"`
/ `FROM rel_effect_cmd` with a typed accessor `eng.demand_rel("scip_want")` that
validates the columns against the registry (one place, not scattered strings).

### Step 3 — put the demand conventions in `rel_catalog`

Give `scip_want`, `rev_cmp_want`, `def_target`, `effect_cmd` `RelDecl`s (group
`demand`) sourced from the registry, so `dl docs relations` lists them. Extend
the reserved-name guard to cover all four (today only scip_want/rev_cmp_want).

### Step 4 — a `nature` column on `rel_catalog`

Add `nature` (plain | demand | sink | clock | git-io | effect | extract | meta)
to `rel_catalog(name, group, cols, nature, doc)`, populated from `RelDecl` +
registry. `dl docs relations` shows it; `dl docs magic` (new topic) filters to
the non-plain rels — the permanent, discoverable audit. You never find one by
surprise again; you `dl docs magic`.

### Step 5 — drift rail (`.dl/magic-rel-audit.dl`, `--check` exit 2)

Dogfood: `scan` `src/**/*.rs`, `match` every `rels.get("<name>")` /
`FROM rel_<name>` literal, anti-join the registry; `diag` if any orphan. A new
string-matched rel that isn't registered fails CI. This is the guarantee that the
magic set can only shrink or be documented, never silently grow.

### Step 6 — generate `docs/reference/magic-rels.md`

From the registry (like `gen-reference.dl` does for `rel_catalog`), with its own
drift rail. The prose home for the taxonomy above.

## Effort / risk

M (registry + 4 lookup migrations + 4 catalog decls + `nature` column + rail +
doc-gen). **Low risk**: additive — the refresh logic is unchanged, only routed
through a table and made self-describing. Sequence: Step 1 → 2 → 3 → 4 (the
user-visible win) → 5 → 6.

The single highest-value slice if you want it small: **Steps 3 + 4** — put the
four demand conventions in the catalog with a `nature` tag. That alone makes
`scip_want` and friends show up in `dl docs relations` and kills the "invisible"
problem; the registry + rail (Steps 1, 2, 5) are the durability layer.
