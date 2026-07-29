# CODEX BRIEF: json_ticklog canonical-JSON regrade (luna-class; executes the ruling)

Execute ruling json_ticklog_encoding = canonical_json_text
(rulings.pl tail): the tick log renders json VALUES as canonical
JSON text, not prolog cons-term text. ORACLE SIDE ONLY -- emitting
json agg heads stays refused (a later arc lifts them; review C5's
stale registry comment gets updated to cite the ruling and this
regrade rather than the overturned cons-text argument, but the
refusal itself stays).

Scope (fence: v6/prolog/conformance/** + the registry comment +
affected fixture expectations + compile/out oracle logs for affected
fixtures ONLY):
1. Define "json value" precisely from the landed semantics (values
   produced by decode/json_each/braces literals/json_array/
   json_object in oracle evaluation) and state the definition in the
   ticklog encoder's header. Plain compound terms KEEP canonical term
   text (the ruling is json-only).
2. Oracle tick-log + final-state encoders render those values as
   canonical JSON (stable key order = sorted keys, no whitespace;
   state the exact canonicalization, it is now part of the
   cross-target log contract).
3. Regrade: every fixture whose logs change gets its expectations
   updated in the same commit as the encoder change, with the diff
   summarized per fixture in your report. Fixtures whose logs do NOT
   change must be byte-identical before/after (prove with a corpus
   log diff, list the changed set).
4. tsv2 side: any currently-compiled fixture whose oracle log
   changes must still grade identical -- if the emitted runtime's
   rendering diverges from the new canonical form, fix the runtime
   ticklog rendering to match (v6/tsv2/runtime/ticklog.ts is in
   fence for THIS narrow purpose only; a concurrent lane owns other
   runtime files -- touch nothing else there, STOP AND REPORT if
   more seems needed).

Grades: conformance full green post-regrade; sweep both modes -- the
identical count must not DROP (changed-log fixtures re-verified);
TEXT_DOOR; roundtrip; plunit; tsv2 tests.

Laws: codex no-commit flow (git READ-ONLY). FIRST ACTION verify HEAD
= dispatch sha, STOP on mismatch. No em dashes; banned words
provenance, substrate, load-bearing, regime. Summary: the canonical
form definition, changed-fixture table with before/after log lines,
grades, cracks.
