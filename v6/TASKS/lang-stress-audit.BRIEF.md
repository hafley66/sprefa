# lang-stress-audit

## Goal
Answer, with receipts: how hard is the dl6 language stress-tested today, where
are the soft spots, and what would a real stress battery add. AUDIT + one small
probe battery. No language design changes (Chris in the room for those). No
compiler fixes; file findings as rows.

## Read first
- CLAUDE.md standing laws (comments are not the language; grep the manifest).
- `v6/prolog/compile/out/manifest.json` (bucket+reason per fixture, 452 rows:
  342 compiled / 110 unsupported at time of writing). Classify every
  `unsupported` reason: intended negative fixture (typo/collision probes) vs
  unbuilt construct vs real impossibility. Cite the throw site in
  `v6/prolog/0_unsupported_messages.pl` / `lower.pl` / `0_*.pl` per class.
- `v6/prolog/compile/{SYNTAX,CONSTRUCT-REFERENCE,SCOREBOARD,PIPELINE}.md`
  vs `parse_dl_dcg.pl` + `registry.pl` (surface/5): every construct in the
  docs must exist in the parser; every parser production must appear in a doc.
  Table the mismatches.
- Existing stress: `v6/prolog/compile/scripts/{metamorphic_rename,roundtrip,
  arm_census,golden_coverage,text_door_receipt}.*`, `v6/dl/fixtures/golden-flex.dl6`,
  `v6/tsv2/goldens/scip_combo` (both doors byte-diffed), conformance
  (`v6/prolog/conformance/go.pl`, 64 fixtures), plunit
  (`compile/test/plunit_tests.pl`). Open cards: fuzz-grammar-threedoor,
  construct-pair-matrix, naive-selfdiff-random, schedule-permutation,
  kill9-midtick (`issues/<slug>/item.md`).

## Probe battery (small, one at a time, each under 10s)
Write 12-20 hand `.dl6` programs under
`v6/prolog/conformance/probes/2026-08-17-stress/` that combine constructs
pairwise where no fixture does today (pick pairs from the construct list x
construct list minus what golden-flex + conformance already cover; show the
coverage matrix). Compile each on the text door
(`v6/prolog/compile/scripts/compile_dl6.sh` or the sweep's per-fixture
command; read the script for the exact form). Record: compiled / unsupported
(reason, throw site) / crash (stack). A crash or a wrong-oracle row is a
finding. Do NOT run whole gates or the sweep; another lane is measuring legs.

## Deliverables (commit on branch audit/lang-stress, open PR, do not merge)
1. `docs/audits/2026-08-17-lang-stress.md`: TOC; the manifest reason
   classification table; docs-vs-parser mismatch table; existing stress
   inventory table (tool / what it varies / what it judges / last green);
   probe matrix + results table; findings ranked; what a real battery adds,
   as cards to file (slug, one line, owner) not as prose.
2. `docs/audits/2026-08-17-lang-stress.visual.human.unga.md`: plain words,
   mermaid, zero citations, for Chris.
Style: no em dashes, banned words (provenance substrate load-bearing regime
refusal honest*), no "support" (say refCount or "compiles"), tables over prose.
