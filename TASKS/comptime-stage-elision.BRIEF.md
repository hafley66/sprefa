# Mixed-stage relations and comptime elision

Deferred follow-on to `plans/2026-08-19-applicative-type-annotations.md`.

Research and specify only after applicative annotations land:

- explicit per-parameter compile-time staging for a relation that also consumes runtime rows;
- inference/elision rules for type-valued inputs and outputs;
- type values as ordinary first-class relation values where the selected execution stage permits them;
- specialization identity and storage erasure;
- diagnostics for runtime values flowing into compile-time-required positions.

This task does not block the annotation arc. The initial annotation rule is
`return: type` selects a compile-time invocation and requires every supplied
argument value to be compile-time known.
