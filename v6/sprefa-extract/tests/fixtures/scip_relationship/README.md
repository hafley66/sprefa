The scip.proto `Relationship` worked example, verbatim from the spec's own
doc comment: `class Dog implements Animal`, where `Dog#` relates to `Animal#`
with is_implementation, and `Dog#sound()` relates to `Animal#sound()` with
both is_reference and is_implementation.

It lives in its own directory rather than under `tests/fixtures/ts/` because
the TS SCIP ratchet in golden_parity.rs walks every `.ts` under that root and
would absorb these files into its corpus counts.

The diet used to DROP relationships, which made v5's `scip_impl` and the
`scip_edge` family inexpressible from a v6 index. This fixture is what pins
that they survive the decode.
