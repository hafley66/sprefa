# Symbol-dict normalization (`_sym_dict`) — R2/R3 plan

Date 2026-07-21. Branch `v11` (tip 8e38d2a8). Companion to
`plans/2026-07-20-strings-n1-verdict.md` (which built + proved the
`_df_node_dict` surrogate mechanism on the df STORED key and specified this
follow-up). This file is the executable plan for eliminating the `format!`
symbol identity: `mint_sym` (mod.rs:414/415) and `lambda_sym` (mod.rs:433).

## Status at time of writing

- **R1 (df in-memory `format!`): LANDED, green.** The df node identity is now a
  dense `NodeIdx` (index into `DataflowFacts.nodes`), not
  `format!("{file}:{line}:{col}:{kind}")`. `ts/flow.rs` is rail-zero; the df-id
  finding is gone from `mod.rs`. Write seam resolves each node's coordinate
  TUPLE to its `_df_node_dict` surrogate (`resolve_coord_surrogates_tuples`).
- **R2 (mint_sym) / R3 (lambda_sym): NOT landed.** The two `mod.rs` findings
  that remain (mint_sym 414/415, lambda_sym 433) require this arc. It is
  multi-day (per the verdict doc) and could not land verified-green in the R1
  session without risking silent join breakage a green suite would not catch.

## Why R2 is large (the honest blast radius, re-verified 2026-07-21)

The stored sym is ALREADY an interned `StringId` (a hash) held as INTEGER
(`spine.rs` SymSink; `lower.rs` `sym_lit`). So `.dl` pure equijoins are
surrogate-agnostic and survive UNCHANGED. Three things do NOT:

1. **Repo-qualification is a hand-built `format!` seam, ~15 sites.** The stored
   sym is `repo::file::kind::name`, minted by `format!("{repo}::{sym}")` at
   `src/engine/extract/call.rs:200,230,244,257,261,264,279,296` and
   `src/engine/extract/type_rels.rs:117,138,142,146,193,204,232,235,273,290`.
   A surrogate cannot be re-concatenated; repo-qualification must become a
   `(repo_id, sym_id)` pair or a second dict tier. `anchor.rs:101
   split_repo_sym` peels the `repo::` prefix and must change in lockstep.
2. **Format-sensitive `.dl` readers.** `std/flow.dl:22-30` `replace_re`-strips
   `repo::` off the sym IN DATALOG. Any literal full-sym pin (`lower.rs:89
   sym_lit` + pins at `lower.rs:455,505,509,555,916,983`) hashes the raw string
   at lower time; a surrogate needs a lower-time dict lookup or a column-form
   literal.
3. **Cross-family sym joins must all flip together.** `df_node.fn_sym` and the
   closure value node's `var` (== `lam_sym`) join `call_def.sym`. If
   `call_def.sym` becomes a surrogate but `df_node.fn_sym` stays a StringId
   hash, the join SILENTLY returns zero rows. Every sym-valued column across
   type + call + dataflow must resolve through the SAME `_sym_dict`.

## Planning protocol

### 1. Type signatures first

    // src/graph/typegraph/mod.rs — mint returns structured key, not a string.
    #[derive(Clone, PartialEq, Eq, Hash)]
    pub struct SymKey {
        pub file: String,
        pub kind: EntityKind,   // the `kind.tag()` column, NOT folded into a string
        pub name: String,
        pub parent: Option<String>,
    }
    pub fn mint_sym(file: &str, kind: EntityKind, name: &str, parent: Option<&str>) -> SymKey;

    // lambda_sym composes on a sym + a coord (R3, rides R2):
    #[derive(Clone, PartialEq, Eq, Hash)]
    pub struct LamKey { pub enclosing: Box<SymRef>, pub coord: String }
    // where SymRef is either a resolved surrogate or a SymKey pending resolution.
    pub fn lambda_sym(enclosing: &SymRef, coord: &str) -> SymKey; // kind = Lambda

    // src/engine/extract/dataflow.rs (mechanism TWIN of resolve_coord_surrogates_tuples)
    fn resolve_sym_surrogates(
        &self,
        syms: &[(i64 /*repo_id*/, String /*file*/, i64 /*kind*/, String /*name*/, String /*parent, "" = none*/)],
    ) -> Result<HashMap<(i64,i64,i64,i64,i64), i64>>;  // (repo,file,kind,name,parent)->surrogate

### 2. Pseudo-code body (resolve seam, mirrors the df tuple core)

    // for every sym-carrying fact: push (repo_id, StringId(file), StringId(kind.tag()),
    //   StringId(name), StringId(parent||"")) as the dict key tuple
    // batch INSERT OR IGNORE distinct tuples into _sym_dict
    // batch SELECT id per tuple via a TEMP _sym_probe join (one round trip)
    // return tuple->surrogate; every sym column stores the surrogate i64
    // sym_decode(surrogate) SELECTs file/kind/name/parent from _sym_dict and
    //   re-renders `repo::file::kind::name` for display ONLY.

### 3. Instance lifetimes

- `_sym_dict`: persistent meta table, NEVER rev-scoped (created in `ensure_meta`,
  exactly like `_df_node_dict`). Survives cold-chunk append because it is
  content-keyed (INSERT OR IGNORE on the tuple).
- `SymKey`/`SymRef`: transient per-extraction; resolved to a surrogate at the
  single serial write connection, never persisted as a struct.
- `_sym_probe`: TEMP, per-connection, DELETE-then-fill each resolve pass.

### 4. Storage layout, read/write sequence, uniqueness

    CREATE TABLE IF NOT EXISTS _sym_dict (
      id     INTEGER PRIMARY KEY AUTOINCREMENT,  -- dense sym surrogate
      repo   INTEGER NOT NULL,   -- StringId(repo)   (repo-qualification, NOT concatenated)
      file   INTEGER NOT NULL,   -- StringId(path)
      kind   INTEGER NOT NULL,   -- StringId(kind.tag())
      name   INTEGER NOT NULL,   -- StringId(name)
      parent INTEGER NOT NULL,   -- StringId(parent) or StringId("") when None
      UNIQUE(repo, file, kind, name, parent)
    );

- Write: extraction mints `SymKey`; the family writer resolves the batch to
  surrogates (one INSERT OR IGNORE + one SELECT), then every sym column
  (`type_entity.sym/parent`, `call_def.sym`, `call_edge.caller/callee`,
  `type_edge.from/to`, `type_link.src/dst`, `type_sig.sym/ref`,
  `call_site.caller/callee`, `call_name.sym`, `call_kind.fn`, `doc_comment.sym`,
  `doc_tag.sym`, `const_value.sym`, `df_node.fn_sym`, closure `df_node.var`, and
  ALL `_rev` twins) stores the surrogate i64.
- Read (display only): `sym_decode` branch reads `_sym_dict`, re-renders
  `repo::file::kind::name`; the `_txt` views (`declare.rs`) gain a `_sym_dict`
  reconstruction arm exactly like the `_df_node_dict` arm at declare.rs:125-144.
- Uniqueness: the tuple UNIQUE is the real key; `id` is the dense surrogate.
  Zero hash collision by construction. rev is NEVER folded in (its own column).

### Where the 4 layers disagree

- Signature layer says `mint_sym -> SymKey`; storage layer says the column is a
  single i64; they meet at the resolve seam (SymKey -> surrogate).
- Repo-qualification: signature/pseudocode want `repo` as a first-class dict
  column; the legacy `.dl` (`std/flow.dl` replace_re) wants `repo::` peelable
  from a string. Reconcile by making `sym_decode` re-render the prefixed form
  and rewriting `std/flow.dl` to read a `repo` column (or a `sym_repo(sym)` rel)
  instead of `replace_re`.

## Codemod inventory (every reader that must change)

Producers (~79 mint sites): rust/mod.rs(14), kotlin.rs(10), go.rs(17),
python.rs(13), ts/mod.rs(23), ts/flow.rs(nested-closure prefix), plus
`mod.rs mint_sym/lambda_sym`. Each stores `SymKey` into its `*Facts` struct
field instead of a `String`.

Repo-qualification seam (~15 sites): call.rs + type_rels.rs `format!("{repo}::{sym}")`
-> carry `(repo_id, SymKey)`; `anchor.rs:101 split_repo_sym` -> read the
`repo` column.

Storage (decls.rs): sym-typed columns keep their NAMES and arity (still one
INTEGER column) — only the VALUE domain changes (StringId hash -> `_sym_dict`
surrogate). `rebuild_legacy_type_rels`, the call router
`flip_call_rels_via_router`, and the `_rev` writers change in lockstep. Built-in
name guards `declare.rs:831,855` unaffected.

`.dl` ecosystem (~83 files reference a sym; ~25 do real joins): PURE EQUIJOINS
SURVIVE unchanged (surrogate is consistent). MUST codemod only the
format-sensitive: `std/flow.dl:22-30` (`replace_re` on `repo::`), and any literal
full-sym pin. Heavy joiners to spot-check post-change: `.dl/flow-panel.dl`,
`std/measures.dl`, `std/callgraph.dl`, `std/flow.dl`, `examples/type_profile.dl`,
`goto-flows.dl`, `callable-coverage.dl`.

Display: `lower.rs:26-56 sym_decode` gains a `_sym_dict` branch (format-independent
once the dict reconstructs text). `coord_reconstruct` is the exact template.

Tests (~22 assert the `::`-string form — MUST be rewritten to the new identity,
NEVER gutted): `call_golden.rs`, `resolver_*`, `go.rs:1127-1216`, `mod.rs:582`
(`assert_lambda_lifted` checks `clo.var.contains("::closure::")` — becomes a
`_sym_dict` kind==Lambda check), `ts/mod.rs`, the typegraph extractor unit tests.
`composite_key_precision::rail_flags_the_real_mint_sym_site_red` INVERTS: after
R2 it must assert mint_sym NO LONGER fires and the surrogate path is used.

## Acceptance for R2/R3

`dl .dl/composite-key-string.dl` reports ZERO findings for `mod.rs` (mint_sym +
lambda_sym gone). Suite green. `dl .dl/ --parse-only` exit 0. No orphan sym
joins (measure: every `df_node.fn_sym` resolves to a `call_def.sym` where the
legacy build did — a join-parity probe, the sym twin of the df orphan check).
