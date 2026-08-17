# render-rust-dl6 (issue: render-rust-dl6, size:med)

FIRST ACTION: `git merge --ff-only d0e8340dff067453e08eedbefaacbd6625777b8c`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Language vocabulary law applies to every .dl6 line you write: rxjs/prolog/SQL words only, descriptive variable names.

GOAL: the dl6 door renders Rust types. v6/dl/typegen/render_ts.dl6 renders TS interfaces from type_row/7 JSONL and holds 12 goldens via v6/prolog/compile/test/typegen_golden.sh, judged against 7_emit_ts_types.pl. Build the twin: v6/dl/typegen/render_rust.dl6 rendering what 8_emit_rust_types.pl renders (structs, Vec<T>, Option<T>, module-prefix collision names), judged the same double way in typegen_golden.sh.

READ FIRST: render_ts.dl6 whole (the strata: list_depth_* unrolling, module prefix casing via split+spread+upper landed in PR #262), 8_emit_rust_types.pl (the target bytes), typegen_golden.sh (how one golden is judged twice). PR #266 made the mutual list_type<->element_type shape close correctly — you may restructure to the mutual spelling if it reads better, cite the fixture 24_mutual_recursion.pl pattern.

FILES YOU OWN: v6/dl/typegen/render_rust.dl6 (new), typegen_golden.sh (additive: the rust leg), new golden pairs under v6/prolog/compile/test/typegen_golden/ (*.types.rs beside the existing *.types.ts, reusing the SAME .type_rows.jsonl inputs — do not fork the row fixtures).
FORBIDDEN: render_ts.dl6, 7_/8_emit_*_types.pl and every other .pl (read-only), v6/tsv2/**, v6/sprefa-engine-rs/**.

VALIDATION: `bash v6/prolog/compile/test/typegen_golden.sh` HOLDS with the rust legs added (12 ts goldens untouched + your rust goldens, every one judged dl6-vs-prolog byte-identical); `cd v6/prolog/conformance && swipl -g go -t halt go.pl` stays 448/0. A shape the dl6 door cannot express is NOT a stopping point: name the construct, cite the manifest reason, degrade that golden to a named absence in the script, and report it.

COMMIT in slices. Close: `issuectl --json close render-rust-dl6 --commit <sha>:<summary>`. Report: golden count before/after, byte-parity table per shape, any named absence with its citation.
