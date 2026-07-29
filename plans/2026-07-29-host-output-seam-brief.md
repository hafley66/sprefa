# host output seam brief (codex sol): struct-typed host outputs end to end

ARCH row struct_host_output_seam, escalated to BLOCKER: terra's flow-parity
value-plane rewrite is complete and HELD on branch codex/flow-parity behind
exactly this. Two halves, both named by the struct arc's landing report:

1. COMPILER: a host/probe OUTPUT column whose declared type is a struct
   (`sh df_node_at(...) -> (span: span, ...)`) refuses as
   `unsupported_surface(column_type_wrapper(Host, Column, none))`. The
   oracle door already accepts declared type names in host columns
   (1_host_expand.pl validate_columns/2, struct arc). The compiler must
   accept them too and plan the column as a struct ref column — the struct
   plane (dictionaries, intern-at-arrival, canonical JSON render) already
   owns storage; this is plumbing the host arrival into it, not new
   storage design.
2. SERVE RUNTIME: `serve/1_hosts.ts` coerce currently JSON-stringifies any
   nested object before it reaches the arrival row, and
   `IHostColumnPlan.type` is the closed union "int" | "text" | "json".
   Widen the plan union for struct-ref columns and deliver the value so
   StructPlane interns it (the struct arc's own suggested shape: StructPlane
   parses canonical JSON text for a ref column — arrivals over HTTP/hosts
   are JSON text already; canonicalization at intern is the runtime's
   standing behavior). The A4 law: oracle and emitter change in the SAME
   arc with fixture coverage in both modes.

## Deliverables
1. Compiler acceptance + lowering for declared-struct host output columns;
   the refusal stays for genuinely unknown type names (fail-first fixture
   for BOTH: the acceptance case red->green, the unknown-name case still
   refusing by name).
2. Serve half: plan union + coerce pass-through + StructPlane intern for
   ref columns; a serve-level test in the injected-seam style
   (tests/serveHost.test.ts is the precedent) proving a host answering a
   struct-typed output lands as ONE dictionary row and the tick log renders
   canonical JSON values, never ids.
3. At least one oracle-graded conformance fixture with a schedule-fed host
   answer carrying a struct value (the canned-rows law), identical BOTH
   modes.

## Laws
- Files: v6/prolog/** (compile + host expand + fixtures), v6/tsv2/serve/**,
  v6/tsv2/runtime/structPlane.ts + types as needed, tests. Do NOT touch
  v6/dl/fixtures/flagship-flow.dl6 or v6/tsv2/scripts/flagship-flow* — the
  held branch owns those and the coordinator merges it after you land.
- Smallest correct: no new storage concepts, no GC, no digest changes —
  intern the canonical text exactly as arrival interning does today.
- Fail-first receipts in test/fixture headers. No new deps. Descriptive
  names; vocabulary law.

## Validation (report exact counts)
- conformance (just-conformance form) — currently 158 PASS + yours.
- plunit (currently 138), sweep BOTH modes (currently 98/96/0-wrong;
  identical growth only), TEXT_DOOR (98/98/0 + yours), roundtrip.
- tsv2 suite green (currently 68 pass/1 skip + yours); extraction-live.sh
  HOLDS; serve leak-soak still green.

## Final summary shape
Base sha; per-half outcome; the fail-first receipts verbatim; exact battery
counts; any named stop.
