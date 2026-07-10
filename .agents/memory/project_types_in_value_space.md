---
name: project_types_in_value_space
description: "types must live in VALUE space as a simple type-IR (quicktype/OpenAPI/TypeSpec/Zig-comptime shape); the ts-import bulk node-types / DotTable.ty-as-name direction was WRONG"
metadata:
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

Ruling 2026-05-19 (user). The "ast type thing" — the bulk
tree-sitter node-types importer (`ts_import.rs`, `b8d72264`) AND
`DotTable.ty` being an `Option<Arc<str>>` name slot — was a
"completely wrong direction." Do NOT regard it.

What the user actually wants: turn "types" into a SIMPLE form using
any standards-based source (tree-sitter is fine as one input) — a
type IR in the spirit of quicktype / OpenAPI / TypeSpec / Zig
`comptime`. Crucially: **types live in VALUE space**, not a separate
type slot. A type is a value you can pass, dot into, reflect on, and
resolve at lower time (comptime == `resolve_dot` on literal args).

This supersedes the "next direction" sketch in
[[project_dots_types_tsimport]] (which already pointed at `ty`→`Value`
+ a meta-`Type` but was still entangled with the node-types importer).
Keep the VALUE-space + type-IR half; drop the bulk-importer framing.

Relation to the cross-file entity graph: ORTHOGONAL. The U1 edge
spine ([[project_cross_file_entity_graph]], def/ref/import rows +
SCIP key, Rust-first) does NOT depend on types-as-values and must not
be blocked on it. ts-import stays parked; the entity graph proceeds
on ast-grep metavar capture, not field-type projection.

**Why:** pattern-matching ethos + one-value/one-callable model; a
type should be addressable data, not a grammar schema dump or a name
string hung off a value.
**How to apply:** when the type work resumes, design it as a
value-space type IR (a `Value` whose dots are {name,fields,kind},
built from a standards source, resolvable at comptime). Not a
declare-every-kind importer. Ask before reviving any `ts_import.rs`
shape.
