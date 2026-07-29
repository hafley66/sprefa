# HOSTS WIRING ARC (planner contract; the ghcacher-on-tsv2 path, phase 1 of 2)

Wire the hosts+extraction lab's SELECTED term forms
(plans/2026-07-29-hosts-extraction-verdict.md is the design record;
its Lowering law table rows RX-H1/H2/B1/Q1 are the semantics) into
the oracle + compiler front. PHASE 1 = deterministic compilation and
grading with world answers fed BY THE SCHEDULE (the conformance
pattern). Phase 2 (live execution in the tsv2 runtime) is a separate
arc; do not start it.

## Scope

1. TERM FORMS into the oracle (engine.pl + registry.pl):
   - `sh_decl(Name, Inputs, Outputs, template(Text))` decl; refusal
     shapes exactly as the verdict's Q1 table (template_mismatch/
     column_mismatch/probe_mismatch families).
   - `probe(Name, InputValues, OutputValues, SaltColumns)` as a body
     item: derives a demand row (identity digest = host+inputs,
     witness digest = +salts); the world's ANSWER arrives via the
     schedule as an ordinary EDB arrival on the response rel
     (inputs+outputs columns, keyed by witness). Late/duplicate
     answers follow keyed-set semantics.
   - `bind_decl(Name, Columns)`: EDB by declaration; bind + rule head
     = refusal bind_and_rule_head(Name). Schedule feeds bind rows in
     fixtures (no live timers in phase 1).
   - `query(RelAtom)` as the program's read surface term.
2. SURFACE SPELLINGS into parse_dl.pl + print_dl.pl + SYNTAX.md +
   the registry-generated grammar:
   - `sh name(in: t, ...) -> (out: t, ...) = ` + backtick template.
   - `? name(args)` probe in bodies with `@ salt(col: Val)` or the
     verdict's salt-column spelling (pick ONE, record it in
     SYNTAX.md, roundtrip-exact).
   - `bind name(col: t, ...).`
   - `? name(args).` query line.
   - SOURCE DEFAULT LAW (user 2026-07-29): worktree is the UNMARKED
     default for any file/content host; a pinned rev is the marked
     case (rev argument or sibling host). No mandatory source atom.
3. COMPILER (analyze/lower/emit): host/bind/query plans compile;
   emitted module carries the plans as DATA (host_plan rows in the
   generated TS) + the demand-rel derivation SQL; execution wiring
   refused with a NAMED unsupported (phase 2 boundary), EXCEPT the
   demand-row derivation and response-rel joins which are ordinary
   rel machinery and must grade byte-identically.
4. FIXTURES: convert the lab's five candidates
   (ghcacher_host_program_term, extraction_fork_callgraph,
   extraction_fork_span_line, native_ts_query_term promoted as a
   compile-check fixture, ghcacher_json_normalization already
   promotable) into conformance fixture/5 entries graded by the
   oracle. ghcacher.dl6's G2 named gaps (host_decl/probe/query had
   no term form) must CLOSE: G2 reparse shows those findings gone.
5. KWARGS PARTIAL APPLICATION rides this lane (you own parse_dl.pl):
   body atoms may omit columns when using named args (each omitted
   slot = fresh anonymous variable); HEAD atoms stay total (named
   refusal partial_head(Rel)). fill_free_slots (parse_dl.pl:590) is
   the current exact-fill gate. Fixture pair + SYNTAX.md row.

## Grades (all re-run by you; coordinator re-runs after)

conformance (max 3 full runs) grows by exactly the new fixtures, 0
fail; sweep both modes: existing 66-fixture buckets ZERO movement;
new host fixtures compile + grade identical where the phase-1 subset
allows, named refusals otherwise; TEXT_DOOR all-compiled pass;
roundtrip ALL GRADES PASS incl the new spellings; plunit grows;
tsv2 tests + import gate; tsgo clean; G2 ghcacher.dl6 gap count
strictly decreases with the remaining gaps named.

## Laws

Worktree agent, codex no-commit flow (git is READ-ONLY: no commits,
no workarounds; tree stays dirty, coordinator commits). FIRST ACTION
verify HEAD equals the dispatch-stated sha; STOP AND REPORT on
mismatch or missing v6/. Descriptive identifiers; no em dashes;
banned words provenance, substrate, load-bearing, regime; refCount
vocabulary in new names. Every new spelling in SYNTAX.md carries its
rx lowering row (cite the verdict's RX ids where they apply). If a
verdict term form cannot wire cleanly, KEEP ITS REFUSAL and name the
crack; never improvise a different spelling.

## Final summary shape

Term-by-term wiring table (term -> oracle/parse/print/compile
status), the G2 gap delta on ghcacher.dl6, new fixture list with
grades, the salt spelling chosen with its SYNTAX.md line, kwargs
receipts, all grades, cracks.
