# userland-typegen-ugliness

## Goal
Make the userland type story fully legible and grade its ugliness with
receipts: what a dl6 author writes, what the compiler derives, what comes out
in TS, Rust, DDL, JSON Schema, OpenAPI, and how a userland program consumes it.
AUDIT ONLY. No implementation, no design changes (generics and type-plane
design are "Chris in the room"; wrapper-composition, type-plane-design cards).

## Read first
- CLAUDE.md (standing laws, style laws, user decisions on types: no coercions,
  one set-rel DDL shape, module-prefix collisions, generics need written
  inspection first).
- `docs/generics-wrapper-inspection.md` (+ .visual.human.unga.md): the prior
  inspection. Re-verify each claim; mark stale ones.
- `plans/2026-08-16-typespec-parity-typegen.PLAN.md`, `docs/type-comptime-roadmap.md`,
  `docs/rust-type-system-tooling-research.md`, `docs/bootstrap-typegen-lab-vs-typespec.md`.
- Type plane sources: `v6/prolog/0_type_plane.pl`, `0_type_ids.pl`,
  `0_option_expand.pl`, `0_generic_expand.pl`, `0_enum_expand.pl`,
  `lower.pl` (comparison_type_mismatch, join_column_type_mismatch),
  `compile/4_emit_jsonschema.pl`, `5_emit_openapi.pl`, `7_emit_ts_types.pl`,
  `8_emit_rust_types.pl`, `9_emit_type_artifact.pl`, `typegen_export.pl`.
- Emitted artifacts: `v6/prolog/compile/out/**` (types.ts / types.rs / schema
  / ddl per fixture; find the exact layout).
- Userland consumers: `v6/tsv2` (how a TS app imports the emitted types and
  binds hosts), `v6/sprefa-engine-rs` (Rust side), typed hosts
  (`plans/2026-08-16-typegen-host-report.pro4.md`, `bytes-type-system.dl6`).
  Note: local main has unlanded typed-host/list-persistence commits from a
  peer session; audit origin/main only, and name that fork in the doc.

## Method
Pick 6 representative userland programs (existing fixtures; name them): scalar
rels, enum, option, list, typed host in+out, a rel-referencing rel. For EACH:
authored decl -> derived type ids -> DDL -> types.ts -> types.rs -> jsonschema
-> what an app writes to consume it (real snippet from tsv2 / engine-rs tests).
Compile only single fixtures, under 10s each. Do not run gates.

## Ugliness scorecard
Per axis, a 0-3 score with the receipt (path:line + emitted text excerpt):
naming (module prefix, casing, `__id`, companion tables), wrapper leakage
(option/list/enum companions visible to the app), duplication across the five
emitters (same fact derived N times), consumer ergonomics (lines an app needs
to read one typed row), error quality (the message a userland author sees on a
type mistake; quote 5 real ones from the manifest reasons), doc coverage (is
this spelled anywhere a user would find it). Then a ranked findings table:
finding / where / cost to fix (S/M/L) / needs Chris (y/n).

## Deliverables (branch audit/userland-typegen, open PR, do not merge)
1. `docs/audits/2026-08-17-userland-typegen.md`: TOC, the six walkthroughs
   (tables + fenced excerpts, each excerpt under 15 lines), scorecard,
   findings, stale claims found in prior docs.
2. `docs/audits/2026-08-17-userland-typegen.visual.human.unga.md`: plain
   words + mermaid, zero citations, one page.
Style: no em dashes; banned words provenance substrate load-bearing regime
refusal honest*; "support" banned; tables over prose.
