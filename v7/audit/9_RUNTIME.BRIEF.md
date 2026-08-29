# Slice 9: runtime and temporal semantics

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/conformance/engine.pl`
- `v6/prolog/conformance/level_eval.pl`
- `v6/prolog/conformance/body.pl`
- `v6/prolog/conformance/ticklog.pl`
- `v6/prolog/2_subscribe.pl`
- `v6/prolog/1_host_expand.pl`
- temporal, recursion, state-machine, and subscription fixtures

Trace one tick from input facts through IDB closure to queued next-tick facts.
Identify the runtime evaluator laws that can also execute compile-time rules.

Write `v7/1_AUDIT/results/9_RUNTIME.md`.
