# CODEX BRIEF: the dl6 door (luna-class, three mechanical pieces)

User go 2026-07-29: (1) wire parse_dl as a compile entry, (2) the text
surface is named .dl6, (3) vscode grammar emitted from the registry.
The DCG (parse_dl.pl) is canonical and G1-exact; this arc adds no
construct and changes no semantics. Centralize: every piece is a new
projection of machinery that already exists.

## Piece 1: text becomes a door

- compile.pl gains a text entry (name it compile_dl6/2 or similar):
  read a .dl6 file, parse_dl to prog(Decls, Rules), feed the EXISTING
  compile path (analyze/lower/emit_ts). No fixture-format change; the
  conformance corpus stays term-form.
- A small runner script beside the existing ones (compile/scripts/),
  usage: file in, gen .ts out, refusals print exactly as
  compile_fixture prints them.
- THE WIRING GATE: for every fixture the sweep currently compiles
  (34), print_dl the fixture's prog to text, re-enter through the text
  door, and byte-compare the generated TS against the term-door
  output. 34/34 identical is the receipt. Plus ONE hand-written .dl6
  file (write a small program yourself using colon types + an enum +
  latest + log) compiled through the door and its tick log
  byte-checked against the oracle via the existing harness.

## Piece 2: .dl6 everywhere the NEW surface lives

- dl_view/ emits *.dl6 (regenerate; the .dl files disappear in the
  same change).
- roundtrip.sh, SYNTAX.md references, and the two real-file G2 inputs:
  rename v6/dl/fixtures/*.dl to *.dl6 (they are old-surface; G2 keeps
  parsing them with the same findings count).
- Do NOT touch v5's .dl anything (root justfile, examples/, .dl/
  rails, editors grammar for v5).

## Piece 3: vscode grammar as a registry projection

- 1_emit_registry_docs.pl (or a sibling emitter consulting the same
  registry.pl) gains a target that emits
  editors/vscode-dl/syntaxes/dl6.tmLanguage.json: keywords from
  surface/5 rows (live words highlighted, reserved words flagged),
  decl words (rel/log/keep/key/enum shapes), comment/string/number/
  atom rules per SYNTAX.md's lexical table, the two rule arrows, the
  semicolon separator.
- editors/vscode-dl/package.json contributes a second language id
  (dl6, extensions [".dl6"]) pointing at the generated grammar. The
  generated file is COMMITTED and marked generated (the SYNTAX.md
  precedent).
- Verify: cd editors/vscode-dl && npm run compile passes. No vsix
  packaging in this arc.

## Grades (all re-run by you)

conformance 115 (max 3 runs); roundtrip ALL GRADES PASS over the
renamed .dl6 views; sweep buckets unchanged (34/31/0 modulo nothing);
plunit 28/28; tsv2 6/6 + import gate; the 34/34 text-door byte
receipt; the hand-written .dl6 oracle-match receipt; vscode extension
compile pass.

## Laws

READ-ONLY git only (sandbox limit; tree stays dirty, coordinator
commits). Descriptive variables; no em dashes; banned words
provenance, substrate, load-bearing, regime. If a piece cannot land
within the laws, STOP it and name why. Final summary: file list, the
entry predicate name, the 34/34 receipt, the hand-written program and
its tick-log receipt, grammar emitter target name, all grades, cracks.
