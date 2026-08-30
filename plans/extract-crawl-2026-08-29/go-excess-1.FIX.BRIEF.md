# Brief: go excess fix 1 (lane `bench-extract-go-parity` -> next fix lane)

Read `plans/extract-bench-2026-08-29/GO-PARITY.REPORT.md` sections 3 and 5
and `plans/extract-crawl-2026-08-29/go.GAPS.md`. After the projection, the
residual excess (`ours - oracle`, full projection) is:

| set | rows | top class (300-sample, seed 7) | verdict |
|---|---|---|---|
| ours - codeql2 | 5,141 | concrete-one-hop-receiver 76 (~1,299) | oracle-side: spot-checks (`checker.go:30104` -> `getTypeArguments`, both defs named in the same file) show both endpoints exist and the binding is concrete; codeql2 simply lacks the edge (its caller has 3 other edges in `go.codeql2.call.tsv`) |
| ours - vta | 10,525 | interface-dispatch 78 (~2,738) | oracle-side: vta seeds implementers from program roots; mock implementers (test fakes) are never reached (go.GAPS.md ours-only classes: "mostly oracle-side: mock implementers vta did not seed") |
| ours - vta | 10,525 | package-qualified-call 78 (~2,738) | needs verification: our import-qualified bindings may be right while vta bare prunes cross-package static calls, or may be name-collisions we resolve too eagerly |

No class in the table is an extractor defect fixable in under 100 lines:
the top classes are rows WE emit with a concrete binding that an oracle
lacks. The work that would move precision further is verification, not
extraction.

## Next lane's job (fail-first, one class at a time)

1. package-qualified-call vs vta: take the 78-sample rows, load each callee
   with `go/types` (reuse `go_gap_classify`'s packages.Load scaffold) and
   split into "callee is the unique package-level func with that name in the
   imported package" (our row correct, vta-side gap) vs "two or more
   candidates in scope" (possible wrong-target, an extractor bug). Fix the
   wrong-target side in `src/lang/go.rs` under 100 lines only if the split
   says so; otherwise close the class as oracle-side.
2. concrete-one-hop-receiver vs codeql2: same scaffold, assert the callee
   method exists on the receiver's type; count the assert-passes. If >= 90%
   pass, the class is codeql-side and this lane writes it into
   ORACLES.REPORT.md section 12 instead of touching the extractor.
3. generated-file excess (471 codeql2 / 714 vta dst rows in
   `*_generated.go`): verify the callee symbols exist in the generated
   source; if they do, the oracles' own generated-file coverage is the gap,
   not our rows.

## Receipt

Per class: count split table, 5 file:line examples, and either a fix PR
(`tests/7N_go_<name>.rs`, fixture under `tests/fixtures/go_findings/`, HEAD
failure in the header) or a one-paragraph oracle-side note appended to
GO-PARITY.REPORT.md section 5. Go wall stays under 10 s (3-run median).
