# Extract tier-2 (schema reification) lab — header (planner-seeded contract)

User direction (2026-07-30): type-system mimicry is extract territory —
"diet typechecking for hundreds repos 1 machine scale fever dream." The
ladder: t0 syntax facts (shipped, 74 f/s), t1 diet module resolution (landed,
1.000/1.000 vs madge), **t2 = this lab**: repos CARRY their public type
surface as schema files (openapi/proto/avro/graphql/json-schema); reify those
into type facts at parse-only cost, never running a real typechecker. t3 SCIP
ingest (shipped 43/43) and t4 real typecheckers stay out of scope.

The internal target algebra (worked out in-session, verify not assume):
primitives / record / enum / union / optional / repeated / map / named-ref.
Claimed mappings: record = struct_as_rows, enum+union = variant rels,
optional = ABSENCE (+ coalesce at use site, per ruling null_design), map =
literally a rel, named-ref = ref(Type). Claimed gaps: bytes, int32/64 width,
float (known hole), their-null-to-our-absence per-format policy, defaults.

## Build-vs-buy FIRST (standing law, no bespoke line before the analysis)

- quicktype's IR is the named candidate (json-schema in, ~20 languages out) —
  can its IR or its schema-inference core be consumed as a library or CLI
  producing JSON we reify? Candidate-by-candidate written analysis.
- Also price: protobuf's own descriptor output (protoc --descriptor_set_out
  = the schema AS a binary/JSON document, no bespoke .proto parser needed),
  avro schemas (already JSON), openapi (already JSON/YAML), graphql SDL
  (needs a parse host or graphql-js introspection JSON).
- The verdict must state per format: consumed NATIVELY by the json plane /
  via one existing tool's JSON output / needs a bespoke parser (avoid).

## Questions

- Q1 the algebra holds? Reify ONE real openapi doc and ONE real .proto
  (via descriptor JSON) into dl6 type facts through the REAL json plane
  (landed this week: braces, spread, key capture, ** descent). Every
  construct in the source doc lands in the algebra or gets a named hole.
- Q2 the reifier is a dl6 PROGRAM, not TS code: schema file -> sh/json host
  -> rels -> derived algebra facts. Grade through both doors where the
  fixtures are hermetic.
- Q3 cross-repo join: with 2+ toy repos (one serving openapi, one calling
  it), join t2 contract facts against t1 import/dep edges into a
  "who-calls-what-shape" rel. This is the fever dream's first receipt.
- Q4 scale price: bytes + rows + wall per schema doc; project to 800 repos
  (the ~/orgs corpus is the reference scale; do NOT run against ~/orgs —
  synthesize or vendor 3-5 real schema files).
- Q5 fidelity: round-trip — algebra facts back out to an openapi fragment
  (the registry-driven openapi emitter is the precedent) and diff against
  the source for the subset the algebra claims.

## Named slots

- slot_bytes_spelling, slot_int_width (one int today), slot_format_null_map
  (per-format null -> absence policy table), slot_defaults_residency
  (schema defaults vs coalesce-at-use-site), slot_graphql_entry (SDL parse
  cost vs introspection JSON).

## Receipts required to land

- The buy analysis table. Q1 reification receipts both formats. Q3 the
  cross-repo join receipt. Q4 the price table. Fixture-promotion candidates.
- Verdict `plans/2026-07-30-extract-t2-verdict.md`; lab files under
  `v6/prolog/labs/extract_t2/`, die on landing.
