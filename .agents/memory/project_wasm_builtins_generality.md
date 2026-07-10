---
name: project-wasm-builtins-generality
description: "Design constraint: built-ins are placeholder registrations for future wasm-loaded user built-ins; engine ships generic seams, all policy strings live in .dl"
metadata: 
  node_type: memory
  type: project
  originSessionId: 4e6d11b9-5d44-4352-bd43-76b4f9c29ee7
---

Chris (2026-07-04, during the hook-event/chat-marks discussion): sprefa's built-ins
are what ships today, but the direction is wasm loading of user-written Rust
built-ins, "like custom webcomponent registration" (`customElements.define()`).

**Why:** generality. A feature like chat-log bookmarking must NOT bake its
trigger phrase or policy into the engine; the engine ships the generic event
seam (e.g. `hook_event` rel) and the phrase/sectioning logic is a plain .dl
program the user owns.

**How to apply:** when adding a built-in, shape it as if a third party could
register it: generic columns, no special-cased strings in Rust, policy via
facts/rules. The registration seams that would host wasm plugins already exist:
`TypeLang` trait + `type_langs()`, RelKind + `rel_kinds()`, the operator
registry. Related: [[feedback-dsl-functional-no-statements]].
