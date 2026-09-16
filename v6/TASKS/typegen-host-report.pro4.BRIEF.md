# Lane brief: typegen-for-host-engines investigation (pro4 arm)

First action: `git merge --ff-only 320464bf`. Failure = STOP AND REPORT.

Investigation lane. You edit NOTHING except creating ONE new file:
`plans/2026-08-16-typegen-host-report.pro4.md`, committed with
`COMMENT_RAIL_IDLE_MS=3000 git commit ...` (never pipe a commit). The rest of
the tree is read-only to you.

QUESTION (from Chris): can the generated per-program types for TS and Rust be
made to work FOR the host engines themselves — v6/tsv2 (the TS
runtime/serve/cli) and v6/sprefa-engine-rs (the Rust runtime)? Today typegen
emits per-program artifacts (v6/prolog/compile/out/*.types.ts and *.types.rs,
~780 of them untracked; emitters at v6/prolog/compile/7_emit_ts_types.pl and
8_emit_rust_types.pl, dl6 doors at v6/dl/typegen/render_ts.dl6 and
render_rust.dl6 reading type_row/7 JSONL from
v6/prolog/compile/typegen_export.pl). The hosts meanwhile shuttle rows untyped
or hand-typed.

Answer these, each with path:line receipts:
1. How does tsv2 represent a rel row today at its runtime seams (its types.ts
   files, the serve endpoints, the emitted-template execution path)? Where
   exactly would a generated interface plug in?
2. Same for sprefa-engine-rs: how do Row/Value/Arrival (src/types.rs) and the
   source_bind/dep_resolve/hosts.rs relation declarations represent columns?
   Where would a generated Rust struct plug in (e.g. typed views over Row,
   serde into generated structs at the SqlRunner or arrivals seam)?
3. What already exists that is halfway there: does anything in tsv2 or
   engine-rs consume a *.types.ts / *.types.rs artifact today? Grep for
   imports of generated type files, or codegen include! / d.ts references.
4. The shape question: per-PROGRAM types vs the hosts being program-generic.
   The hosts run arbitrary dl6 programs, so compile-time program types cannot
   type the host core. Name the seams where per-program types CAN bind:
   emitted harness programs (emit_rust_harness generated program.rs), served
   endpoint payloads in tsv2, golden gate scripts, client-side consumers. For
   each seam: what generation step would wire it, what stays untyped, roughly
   small/med/large.
5. Forks needing Chris, one line each, only if real, with the citation that
   proves the fork exists.

Style laws for the report: tables and lists over prose; every claim carries
path:line; banned words: provenance, substrate, load-bearing, regime, refusal,
support. No recommendations dressed as decisions — findings and sized options
only.
