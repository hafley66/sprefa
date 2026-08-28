# Slice 10: compiler pipeline, storage projection, and emitters

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/compile.pl`
- `v6/prolog/compile/0_storage_projection.pl`
- `v6/prolog/compile/registry.pl`
- numbered emitter files under `v6/prolog/compile/`
- `v6/prolog/emit_rust.pl`
- `v6/prolog/emit_ts.pl`

Trace the canonical program fields consumed by every emitter. Mark parser-free
predicates and target-specific policy. Record the exact contract required to
keep `sprefa-engine-rs` unchanged.

Write `v7/1_AUDIT/results/10_EMITTERS.md`.
