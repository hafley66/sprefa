# lowerpl-arm-census (issue: lowerpl-arm-census, size:med)

FIRST ACTION: `git merge --ff-only d0e8340dff067453e08eedbefaacbd6625777b8c`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root.

GOAL: a coverage census answering "which lower.pl clause arms and unsupported_construct throw sites does NO corpus program reach". The dd arm already caught one instance from the other side (ARCH.pl:950: mutual_recursion fired on zero fixtures — that gap became the PR #266 silent-wrong). Systematize it.

APPROACH (weigh, then pick, and say why): (a) static — enumerate every `throw(unsupported_construct(...))` site and every multi-clause predicate's arms in v6/prolog/lower.pl, cross with the manifest (v6/prolog/compile/out/manifest.json reasons) and grep of conformance fixtures for which named constructs appear; (b) dynamic — a swipl wrapper asserting a coverage fact per arm entry while the conformance battery runs (prolog has `profile/1` and clause instrumentation; research what SWI offers before building anything bespoke — build-vs-buy law). A hybrid is fine.

FILES YOU OWN: a new script/prolog file under v6/prolog/compile/scripts/ (e.g. arm_census.pl + a runner sh), and a REPORT at plans/2026-08-15-lowerpl-arm-census.REPORT.md. Nothing else — you change NO compiler file.
FORBIDDEN: lower.pl and every other existing .pl (read-only), fixtures, emitters, v6/tsv2/**, v6/sprefa-engine-rs/**.

DELIVERABLE: the report lists (1) every throw site file:line with its construct name and reached/unreached verdict; (2) every unreached arm as a table row with a one-line candidate-fixture sketch; (3) counts: total sites, reached, unreached. Each unreached THROW is a hypothesis per the repo law (a refusal is a hypothesis) — flag any whose construct the manifest claims compiles (contradiction = real finding, file it with issuectl).

VALIDATION: conformance battery still 448/0 after your instrumentation run (`cd v6/prolog/conformance && swipl -g go -t halt go.pl`); the census run itself is reproducible (two runs, same counts — paste both).

COMMIT plain. Close: `issuectl --json close lowerpl-arm-census --commit <sha>:<summary>`. Report: the three counts, top-10 unreached list, contradictions found.
