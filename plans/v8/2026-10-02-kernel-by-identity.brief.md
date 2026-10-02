# Brief: compiler-generated goals reference kernel and prelude relations by identity

Defect (verified 2026-10-02): a user product named `tuple` silently kills generic builds (0
instances, no diagnostic). 24 sites build generated goals as `name(owner, "<label>")`, which resolve
in the user's module, so user declarations capture compiler-generated code:
`git grep -n 'compound("name", vec!\[owner' -- src` (24: `_5_derived.rs` 2, `_5a_generic.rs` 4,
`_6_partial.rs` 2, `_7_goals.rs` 6, `_8_express.rs` 8, `_4_comptime/_0_load/_5_graph.rs` 1,
`_6_source.rs` 1). Also prelude relations read by literal name: `prelude_call`
(`_2_lower/_7_goals.rs`), `prelude_relation` (`_4_comptime/_5_finish.rs`, which returns `vec![]`
silently when one is missing).

## Goal

- Generated goals use `ref(kernel(<name>))` for kernel relations and a prelude identity resolved
  once when the prelude loads; a missing prelude relation is a loud diagnostic.
- Code that recognises generated goals by label text (`== Some("tuple")`,
  `"intern" | "intern_snapshot"`, 232 string-name matches in `src/`) matches the identity.
- A test: every user product named after a kernel or prelude relation (`tuple`, `intern`, `:`,
  `node`, `edge_snapshot`, `edge_ref`, `obligation`, `discharged`, `Extends`, ...) next to a generic
  build leaves the build's rows identical to the same program without that declaration.
- A test that fails when `src/` gains a `compound("name", vec![owner` site.

Method, laws, validation and receipt: as `plans/v8/2026-10-02-strings.brief.md` (all edits first;
one cargo process; both engines; byte-identical registry; no merge/push). No sqlite_ivm change
expected.
