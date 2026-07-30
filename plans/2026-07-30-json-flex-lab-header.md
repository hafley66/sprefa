# JSON flex lab — header (planner-seeded contract)

User direction (2026-07-30): "have we flexed the fuck outta our json stuff in
a lab yet, like i mean really make it covering or check that it has decent
coverage bc json is large statemachine."

Standing facts: the json-wiring lane landed the surface (bare `{...}`, `$`
key holes, `**` descent, array spread, `list(text)`, json as its own storage
kind) with 23 fixture entries and a 20/27 archive parse acceptance. That is
parse acceptance, not state-machine coverage. Measured holes at seeding:
zero fixtures touch json null, float/exponent numbers, string escapes, or
unicode; two LIVE known defects (top-level scalar renders as a JSON string
via encodeValue first-char sniffing; the shipped `json_extract(...) IS NOT
NULL` presence read collapses present-null with absent — the option lab
measured one document silently lost).

## Scope: grade, then harden

This lab GRADES the json plane against the real json state machine on BOTH
doors, files fail-first fixtures for every hole it proves, and fixes only
what is mechanical (encoder/predicate-level). Anything structural comes back
as a named card. Rulings not relitigated: `json5_subset = unquoted_keys_only`,
`json_ticklog_encoding = canonical_json_text` (sorted keys, no whitespace),
`json_key_hole_marker = dollar`, bare-brace spelling, json agg heads stay an
arc, `null_design = get_else_use_site_never_storage` (the LANGUAGE has no
null — but json DOCUMENTS carry null as a value; the boundary between those
two facts is exactly Q2).

## Question families (each = fixtures on both doors, byte-diffed)

- Q1 VALUES: every json value kind through storage, destructure, and tick-log
  render: null, true/false, integers at/over 2^53 (the @libsql bigint class
  bit twice already), -0, floats and exponent forms (float is a KNOWN language
  hole — grade what happens today: refusal? corruption?), strings with every
  escape class (\" \\ \/ \b \f \n \r \t, \uXXXX incl surrogate pairs, control
  chars), empty string, empty object, empty array, deep nesting (both wide and
  100+ deep — sqlite json1 has depth limits, find them), arrays of objects of
  arrays.
- Q2 NULL-IN-DOCUMENTS vs no-null-language: what does `{key: null}` do in a
  pattern match today? What SHOULD it do under the ruling (absence and
  present-null are distinct: json_type preserves, ->> collapses — the wiring
  lane's own trap note)? Every read path classified collapse-vs-preserve, with
  the option lab's three-variant read as the reference semantics.
- Q3 KEYS: duplicate keys in an input document (last-wins? first? refusal?),
  unicode keys, key sort order in canonical output for non-ASCII (byte sort vs
  codepoint sort — pick becomes a cross-target contract), empty-string key,
  keys colliding with `$` hole spelling, quoted-vs-unquoted key equivalence.
- Q4 CANONICALIZATION: the cross-target log contract. Round-trip property:
  parse(canon(x)) == parse(x) for a generated corpus; canon idempotent; canon
  agreement between oracle encoder (ticklog.pl), tsv2 encoder (ticklog.ts),
  and sqlite json1's own normalization where used. Property-test shape: a
  generator over the json grammar, hundreds of documents, both doors.
- Q5 MALFORMED: every truncation/corruption class at the arrival boundary and
  in stored columns (the guard-first-in-WHERE claim: prove malformed JSON in a
  stored row is a named refusal/zero-row, never a runtime error, in EVERY
  statement family incl the delta arm). The `#`-comment-inside-braces superset
  accepted today: grade whether it round-trips or silently rewrites.
- Q6 SCALE: json_each/json_tree statement counts flat in document count
  (count-test law), EXPLAIN SEARCH-not-SCAN on delta arms, one large-document
  receipt (1MB+ nested doc through storage, destructure, and render) with
  bytes and ms recorded.
- Q7 EXTERNAL ORACLE: grade our canon + destructure against an external
  reference on the same corpus — sqlite json1 itself, jq, and (research per
  build-vs-buy before any bespoke generator) an existing JSON test-suite
  corpus: JSONTestSuite (seriot.ch, the de-facto parser state-machine corpus,
  y_/n_/i_ files) is the obvious buy. Report pass/fail/impl-defined tallies
  the way that suite intends.

## Named slots

- slot_json_float_fate: floats in json values vs the no-float language: store
  as REAL? refuse? text-preserve? Hands back priced options if not decidable.
- slot_json_bignum: integers beyond 2^53 and beyond i64: text-preserve vs
  refuse vs lossy — sqlite, JS, and prolog disagree; the pick is a contract.
- slot_key_collation: canonical key sort for non-ASCII keys.
- slot_dup_key_fate: duplicate keys in input documents.
- slot_null_pattern_spelling: how a pattern matches present-null explicitly
  (if at all) given the language has no null value.

## Receipts required to land

- Fixture wave on both doors, sweep both modes zero wrong, prior buckets
  unmoved.
- The JSONTestSuite tally (or the researched equivalent) with every n_ file
  that crashes rather than refuses named as a defect.
- Canon agreement receipt across the three encoders on the generated corpus.
- Fail-first receipts for every fixed defect; named cards for every
  structural hole.
- Verdict doc `plans/2026-07-30-json-flex-verdict.md`; lab files under
  `v6/prolog/labs/json_flex/`, die on landing.
