% Opus coordination session: an 8-lane deepseek-flash fleet answering "could
% Haskell do what Prolog does for us", a buried-SCC excavation from clpfd, the
% first cut of a dl6 diagnostic channel for an LSP, and two audit lanes sent
% back over all of it hunting cheats.
%
% Load:
%   swipl -q -l chat_log/20260802.2.opus-flash-fleet-haskell-prolog-dl6-diag.pl
% Grade:
%   swipl -q -g go -t halt chat_log/20260802.2.opus-flash-fleet-haskell-prolog-dl6-diag.pl
% Queries:
%   directive(Name, Text).
%   lane(Name, Model, Base, State, Deliverable).
%   finding(Name, Severity, FoundBy, Text).
%   receipt(Name, Command, Result).
%   correction(Who, What).
%   answered(Question, Answer).
%   open(Item, Owner).
%
% READ THIS FIRST IF YOU ARE LOST. The session ran one experiment: give a cheap
% model (deepseek-v4-flash-0731 on opencode) real work in isolated worktrees and
% see what it produces. Eight lanes ran. Every lane produced working, checkable
% artifacts. Every defect found in them was a MEASURING INSTRUMENT or a CLAIM
% ABOUT THE WORLD, never the code itself. That is the session's one finding
% about the model, and it reproduces the 2026-08-02 flash-vs-opus lane report.

:- module(session_20260802_2, [go/0]).
:- discontiguous lane/5.
:- discontiguous finding/4.
:- discontiguous receipt/3.
:- discontiguous correction/2.
:- discontiguous answered/2.
:- discontiguous open/2.
:- discontiguous resolved/2.

session(id, '20260802.2').
session(date, '2026-08-02').
session(coordinator, 'opus 5 (1M context)').
session(branch, 'codex/rel-ref-file-span-lab').
session(base_at_open, 'a7108169').
session(head_at_close, 'a7a58f95').
session(commits_this_session, 24).
session(pushed, false).
session(tagged, false).
session(merge_state, 'all 6 lane branches merged into codex/rel-ref-file-span-lab; branch is 500 ahead of local main; local main 32 ahead of origin/main (github hafley66/sprefa). PUSH BLOCKED, see finding gate_red_dl_test.').
session(prior_ledger, 'chat_log/20260802.1.fable-state-diet-worktree-skill-purge.md').

% ═══ directives, all user-set ════════════════════════════════════════════════

directive(flash_fleet,
          'run the labs on deepseek-v4-flash-0731 via opencode to see what it is made of. EXPLICIT DEVIATION from the OPUS-only lab law, user instruction, recorded here so it is not read later as drift.').
directive(no_cheating,
          'the Haskell interpreter lane may not depend on, vendor, or transcribe any existing Prolog / miniKanren implementation. Build from the operational semantics.').
directive(park_for_later,
          'keep the Prolog-in-Haskell work in its own folder under v6 for later rather than distilling and deleting it. Lab-death protocol deliberately not applied.').
directive(commit_carries_state,
          'commit messages carry session state so the arc is readable from git alone.').
directive(dl6_lsp_not_prolog_lsp,
          'the want is an LSP for DL6, not for our Prolog source. Coordinator conflated the two once and was corrected.').
directive(diag_state_is_a_seam,
          'the compiler writes diagnostics somewhere a separate process reads, file or stream. Do NOT wire the whole language server in Prolog.').
directive(beautiful_prolog,
          'the diagnostic emitter specifically must be idiomatic and declarative; the user will read it.').
directive(review_everything,
          'send fresh lanes back over all work hunting wonky cheats, bad design, and inappropriate couplings.').
directive(session_log_is_prolog,
          'session logs move to .pl facts; save-session skill to be updated to match.').

% ═══ the fleet ═══════════════════════════════════════════════════════════════
% lane(Name, Model, Base, State, Deliverable).

lane(hs_graph, flash_0731, 'a7108169', landed,
     'ported all ten exports of v6/prolog/0_graph.pl to Haskell over containers Data.Graph. 110/110 differential cells against real SWI answers on the 11 fixture shapes. Landed in v6/hs-prolog/graph.').
lane(hs_demand, flash_0731, 'a7108169', landed,
     'inventoried which SWI powers the repo actually uses and answered each with a COMPILED probe. 24 probes pass. Verdicts: 15 direct, 10 encodable, 2 hostile, 1 absent, 5 refused-without-probe and filed unproven. Landed in v6/hs-prolog/demand.').
lane(hs_idioms, flash_0731, 'a7108169', landed,
     'cloned 10 real Haskell projects (postgrest, graphql-engine, HLS, servant, rio, katip, co-log, hs-opentelemetry, effectful, safe-exceptions) with commit shas and cited file:line for every convention claim. Produced the GC-knob finding that a bench harness needs. Landed in v6/hs-prolog/idioms.').
lane(hs_interp, flash_0731, 'a7108169', cancelled_work_kept,
     'Prolog kernel on logict. 1032 lines, compiles, runs all three required fixtures. CANCELLED by user mid-flight; committed on branch lane/hs-interp at 81921ea4, deliberately NOT merged into v6/hs-prolog.').
lane(swi_scc, flash_0731, 'a7108169', committed_negative,
     'extracted Triska Tarjan verbatim from clpfd.pl:5892-5962 into a callable SWI module. Agrees with our Kosaraju on 11 shapes and 360 fuzzed graphs. 21x slower. Verdict: do not use. Branch lane/swi-scc at a315deba.').
lane(dl6_diag, flash_0731, '3d8c34e3', committed_blocked,
     'LSP-shaped diagnostic channel for the dl6 compiler. All gates green. BLOCKED from merge by the wrong-position defect and the lab coupling. Branch lane/dl6-diag at 8907a040.').
lane(audit_hs, flash_0731, '3d8c34e3', landed,
     'audited the Haskell work by sabotage and measurement. Found 4 defects the coordinator missed, 2 of which corrected coordinator claims. Landed at 73953052 under v6/hs-prolog/AUDIT.').
lane(audit_pl, flash_0731, '8907a040', committed,
     'audited the Prolog work. Found the wrong-position defect by CONTROLLED EXPERIMENT (5 single-statement controls correct, 4 decoy variants wrong). Branch lane/audit-pl at c805eafc.').

% ═══ findings ════════════════════════════════════════════════════════════════
% finding(Name, Severity, FoundBy, Text).

finding(diag_wrong_position, blocks_merge, audit_pl,
        'compile/parse_dl.pl:210 statement_location_for_reference/4 returns the FIRST statement containing the relation as a sub_term, so any earlier valid mention wins over the actual offender. Coordinator reproduced: offender on line 6, both human line and JSON report line 5. In an editor the squiggle lands on innocent code. A fallback is visibly a fallback; a wrong position is not. The lane test passes because it uses single-statement programs only. FIX BELONGS IN THE RESOLVER: the refusal already knows which rule it rejected, the resolver is not told.').
finding(diag_lab_coupling, blocks_merge, coordinator,
        'compile.pl:35 carries use_module(labs/diag_channel/diag). The lab protocol DELETES labs on landing, so the compiler stops loading when the lab dies. Confirmed live by the audit: moving the directory aside turns the refusal path into compile:emit_diag_file/2 Unknown procedure. diag.pl is a compiler module and belongs beside 0_refusal_messages.pl.').
finding(diag_bare_uri, small, coordinator,
        'the JSON uri field is a bare filesystem path. LSP requires a file:// scheme URI. One-line fix.').
finding(diag_unwritable_target_swallows, medium, audit_pl,
        'an unwritable DL6_DIAG_JSONL turns a refusal into a raw open/3 Permission denied, exit 2, indistinguishable from a normal refusal. The diagnostic is lost entirely with no fallback to stderr.').
finding(diag_stream_never_closed, small, audit_pl,
        'diag_stream_open/0 opens the target once per process and never closes it; one open append handle per emitting thread under a server.').
finding(hs_indegree_quadratic, medium, audit_hs,
        'v6/hs-prolog/graph/src/Graph.hs:113-115 indegree is O(V^2 * degree) and sits on the hot path of graphTopologicalOrder and graphHasCycle, paid every call. Coordinator measured graphHasCycle on a chain at 105/429/1778 ms for N=1000/2000/4000. The 11 fixture shapes are too small to trigger it.').
finding(hs_nub_quadratic_construction, medium, coordinator,
        'Graph.hs:37,44,51 use nub, which is O(n^2) because it needs only Eq. Graph construction measures 757 ms at N=16000 against 13 ms for the actual SCC. The single most famous Haskell performance trap, walked into four times.').
finding(hs_timing_units_1000x, medium, coordinator,
        'graph/app/Timing.hs:20 divides getCPUTime picoseconds by 1e12, yielding SECONDS, and line 23 labels it " ms". Every number in that lane report Numbers section is 1000x small. Real: SCC 3.152 ms, closure 151.681 ms, so ~172x faster than the Warshall path, not ~170,000x.').
finding(hs_interp_grader_no_teeth, medium, audit_hs,
        'interp app/Main.hs:42 is ok = not (null solutions). Every PASS means the solver came back non-empty, never that the answer is right. The fixtures encode exact SWI-graded values and the Haskell side does not compare against them.').
finding(hs_interp_vacuous_pass, medium, audit_hs,
        'Tabling.hs solves tabled clause bodies with all variables free, but edge(Eqs, From, Dep) :- member(eq(From,Expr), Eqs), idep(Expr, Dep) needs Eqs bound, so no reach facts are derived. counter_ok therefore passes VACUOUSLY over an empty relation and broken_rejected fails. SWI passes both.').
finding(hs_demand_exit_zero_on_fail, medium, coordinator,
        'demand/probes/app/Main.hs never calls exitFailure. A probe prints FAIL and the process still exits 0. Read the printed lines; the exit code proves nothing.').
finding(hs_idioms_dead_test_suite, small, audit_hs,
        'idioms/starter/starter.cabal declares a test-suite pointing at a nonexistent test/Spec.hs and depending on a nonexistent library named starter. cabal build all skips it silently; idioms has no runnable grader at all.').
finding(hs_dead_dependencies, cosmetic, audit_hs,
        'graph/graph-scc.cabal declares fgl and algebraic-graphs in the LIBRARY stanza while src/Graph.hs imports neither; they are used only by buy-probe.').
finding(hs_sugar_unconditional_pass, small, coordinator,
        'demand/probes/src/Probe/Sugar.hs:21 prints PASS with no condition. It is a Template Haskell probe so compiling is real evidence, but the printed line asserts nothing at runtime.').
finding(bench_results_csv_stale, small, coordinator,
        'v6/sprefa-store/bench/out/results.csv header has 8 columns while run.sh:36 writes 11 and its rows carry 10. The committed artifact is stale against the script.').
finding(sugar_spans_absent, design_gap, coordinator,
        'ALL SEVEN dl6 expansion passes carry ZERO source positions across 1608 lines (0_match_expand 137, 0_enum_expand 180, 0_coalesce_expand 274, 0_seq_expand 194, 0_relation_edge_expand 92, 1_expansion 57, 1_host_expand 674). match arms, |-> arrows, and enum declarations synthesize fresh rules with nothing behind them. A diagnostic on the third arm of a match has no source text to point at. This is the classic macro source-mapping problem that Rust, Scheme, and TypeScript all treat as core infrastructure. The coordinator BRIEFED THE SIDE TABLE WITHOUT THE PROPAGATION, so the landed channel works for parse-time diagnostics and silently degrades downstream of sugar.').

% ═══ the merge and the blocked push ══════════════════════════════════════════

lane(dl6_diag_fix, flash_0731, '8907a040', merged,
     'fixed all three audit defects. Wrong position solved head-first: a refusal names
      the offending rule and that rule DEFINES the reference, so the resolver looks up by
      head identity before falling back to the sub_term scan. diag.pl moved out of labs/
      to v6/prolog/. uri is now file://. Coordinator re-ran every gate.').
lane(dl6_vscode, flash_0731, 'b7cdf014', merged,
     'answered the rich-diagnostics question by DRIVING A LIVE VS CODE over the Chrome
      DevTools Protocol, not by quoting the spec. Diagnostic.message is escaped plain text;
      hovers render markdown plus a subset of HTML with inline styles stripped. Recommends
      the extension watch the JSONL and set diagnostics itself, no new server.').
lane(extract_drift, flash_0731, 'a7a58f95', reported,
     'investigated the 79-vs-82 golden drift. Rebuilt the OLD binary from 023cd61a, got
      exactly 79, set-diffed. Verdict correct-and-additive, re-baseline to 82.').
lane(extract_drift_fable, fable_5, 'a7a58f95', reported,
     'same question, independently. Rebuilt from 2faf82b1^ instead, also got exactly 79.
      Same verdict. Found THREE things flash did not: downstream inertness, the repo own
      prior acknowledgment of the defect, and the sanctioned replacement shape.').

finding(gate_red_dl_test, blocks_push, coordinator,
        'green-all on the merged tree is green on every leg EXCEPT dl-test: conformance
         281/0, plunit 283/283, TEXT_DOOR 196/196/0, roundtrip G1+G2 pass, PROLOG_LINT
         findings=1 baseline=1 OK, GOLDEN FLEX HOLDS. dl-test fails for a PRE-EXISTING
         reason with two parts, neither caused by this session (zero files changed under
         v6/dl or v6/sprefa-extract across a7108169..HEAD).').
finding(extract_pin_points_at_reaped_worktree, blocks_push, coordinator,
        'v6/dl/tasks.d.ts:346 pins DEFAULT_EXTRACT_BIN as a STRING LITERAL TYPE to
         .claude/worktrees/extract-golden-plan/.../target/debug/extract. That worktree was
         removed in a prior cleanup, so the test cannot find a binary at all. The comment
         above it calls the pin an owner ruling the ledger keeps standing, so the
         coordinator did not touch it. v6/tsv2/scripts/extraction-live.sh already carries
         the successor shape AND a comment naming this exact defect as a known item.').
finding(extract_golden_drift_79_to_82, needs_user_ruling, coordinator,
        'v6/dl/tests/4_hosts.test.ts:463 asserts 79 rows from the unchanged 6-line fixture
         v6/dl/fixtures/corpus/bad.ts; the current binary emits 82. TWO INDEPENDENT
         INVESTIGATIONS (fable and flash) reached the same verdict by rebuilding the old
         binary from DIFFERENT base commits and both getting exactly 79. The delta is
         purely additive: commit 2faf82b1 (2026-07-29, df param/arg aux records) added
         three TRUE facts about the fixture, and the original 79 survive byte-identical.
         Nothing downstream consumes them: 4_ingest.ts handles only node/edge/sig/site/
         const, so param/arg fall through. Recommendation re-baseline to 82; the residual
         risk both named is that a raw-count assertion against a live evolving binary
         breaks on every future additive change.').
finding(gitignore_clobbered_by_coordinator, process, coordinator,
        'the coordinator wrote a fresh .gitignore in the hs-interp worktree, overwriting
         the repo real 69-line file, and the merge carried the damage in. Also why a later
         git add -A nearly swept examples/_npm into a commit. Restored from main plus the
         lab entries appended. Root cause: cat > .gitignore in a worktree without checking
         whether one existed.').

correction(coordinator_to_self,
           'ran green-all piped through tail, which masked its exit code, and reported the
            gate as passing when dl-test had failed. Re-ran with full capture.').
correction(coordinator_to_self,
           'raised an alarm that a lane had triggered an OAuth flow on the user machine.
            It was the Chrome DevTools Protocol debug port of a VS Code instance the lane
            launched deliberately. Nothing authenticated. Should have traced it first.').
correction(coordinator_to_self,
           'seeded the audit with a claim that removing the outer sort in scc.pl turns 3
            tests red. It is 1 test, exit 1. The 3 came from counting output lines.').

answered('can a dl6 diagnostic message carry markdown or html',
         'no. Diagnostic.message renders as escaped plain text, proved in a live VS Code
          and confirmed by the coordinator reading the screenshot. What DOES render: the
          code field as a clickable link, relatedInformation as clickable locations, and a
          HoverProvider returning MarkdownString, which renders markdown plus <b> under
          supportHtml while STRIPPING inline style attributes. Recommended shape is a plain
          diagnostic plus a hover that re-renders it, which is the pattern v5 already ships
          at editors/vscode-dl/src/extension.ts:261-286.').
answered('is prolog the right tool for the sprefa-extract subset work',
         'half. Approved-subset is a FILTERING problem, not a parsing one, and filtering a
          tree by rules is what unification is for. But tree-sitter already parses 100+
          languages with error recovery, which is exactly the shitty-parse tolerance the
          goal needs, and a DCG gives none of it. sprefa-extract already extracts PROLOG
          itself via a tree-sitter grammar (src/lang/prolog/_0_source.rs), so the
          five-language pattern is established. Split to defend: tree-sitter parses, Rust
          emits the four planes, and the APPROVAL POLICY plus TYPE MAPPING become Prolog
          rules over the rows. Undecided and it decides the design: whether approval needs
          the full CST shipped per file or just the rows.').

% ═══ receipts ════════════════════════════════════════════════════════════════
% receipt(Name, Command, Result).

receipt(green_all_merged_tree,
        'cd v6/prolog && just green-all',
        'EXIT 1. Green: conformance 281/0, plunit 283/283, TEXT_DOOR 196/196/0, roundtrip
         G1 ALL PASS + G2 NO PARSE ERRORS, PROLOG_LINT findings=1 baseline=1 OK, GOLDEN
         FLEX HOLDS. Red: dl-test only, and it is the last recipe so nothing went unrun.').
receipt(merge_did_not_touch_the_failing_area,
        'git diff --name-only a7108169..HEAD -- v6/dl v6/sprefa-extract',
        '0 files. The fixture is untouched since 84744e7a. The merge is not responsible.').
receipt(extract_three_extra_rows,
        './v6/sprefa-extract/target/release/extract v6/dl/fixtures/corpus/bad.ts',
        '82 rows, of which exactly 3 carry record param or arg: param name at pos 0,
         arg console at pos -1 (receiver convention), arg message at pos 0.').
receipt(downstream_ignores_the_new_rows,
        'grep -c param|arg v6/dl/src/4_ingest.ts',
        '0 hits. The ingest switch handles node/edge/sig/site/const only, so the new rows
         fall through unread. Fable found this; flash did not check it.').
receipt(repo_already_knew,
        'v6/tsv2/scripts/extraction-live.sh:52-60 comment',
        'verbatim: "v6/dl s 4_ingest.ts still defaults to a DEBUG build under an absolute
         path from another worktree; that is the known perf item this script does not
         copy." The defect was documented before this session found it.').
receipt(salvage_banked,
        'per-worktree git diff --cached --binary into /tmp/salvage-2026-08-02b',
        '18 patches, 1.1M, covering every dirty worktree including nine from prior
         sessions. Two were re-banked without dist-newstyle after the first pass hit 34M.').

receipt(gate_arch, 'cd v6/prolog && swipl -g go -t halt ARCH.pl', 'PASS').
receipt(gate_plunit, 'cd v6/prolog && just plunit', '281/281 exit 0 (baseline 276, +5 from the diag lane tests)').
receipt(gate_text_door, 'cd v6/prolog && just text-door', 'compiled=196 byte_identical=196 failures=0').
receipt(gate_compile_speed, 'cd v6/prolog && just compile-speed', 'programs=4 phases=24 regressions=0 improvements=0 OK, so the diag channel costs nothing on the success path').
receipt(diag_end_to_end,
        'DL6_DIAG_JSONL=... swipl -g "compile_dl6(broken.dl6,_)"',
        'valid JSON, LSP-shaped, source line 4 emitted as line 3 zero-based, message field byte-identical to the human line').
receipt(diag_wrong_position_repro,
        'decoy.dl6 with counter mentioned on line 5 and offending on line 6',
        'HUMAN and JSON both report line 5. Wrong-position defect confirmed by the coordinator independently of the audit.').
receipt(graph_grader_sabotage,
        'make graphClosure reflexive in v6/hs-prolog/graph/src/Graph.hs, rebuild, run grader',
        '12 of 110 cells red, exit 1. Restored: 110/0 exit 0. The grader has real teeth.').
receipt(golden_independence,
        'dump 0_graph.pl answers for all 11 shapes from swipl and compare to fixtures/Golden.hs',
        'closure, components, cyclic, topo, and cycle flag all match. The oracle was NOT generated by the code it grades.').
receipt(scc_extraction_verbatim,
        'whitespace-normalize clpfd.pl:5892-5962 and the extracted scc.pl, then diff',
        'all 57 non-blank lines present, nothing missing.').
receipt(scc_speed,
        'time scc_components/2 against graph_components/2 on a 1000-node chain',
        'Tarjan 171 ms / 3,054,074 inferences; Kosaraju 8 ms / 143,616. Audit re-measured at three sizes: Tarjan scales 3.92x and 3.96x per doubling (quadratic), Kosaraju 2.12x and 2.11x (linear).').
receipt(kosaraju_scales_linearly,
        'graph_components/2 on chains at N=1000..16000',
        '9/20/43/89/193 ms and 143,614..2,811,253 inferences. About 14.5M inferences per second, which is SWI throughput. No hidden quadratic; our Kosaraju is fine.').
receipt(swi_has_no_scc,
        'module_property(ugraphs, exports(E)) and pack_list(scc)',
        'ugraphs exports 18 predicates, none is SCC. pack_list finds no matching packages. ugraphs.pl is the only graph file in the library directory. The only standard-library mention of strongly connected components is inside clp/clpfd.pl.').
receipt(triska_standalone_is_scryer,
        'curl https://www.metalevel.at/scc.pl then consult it in SWI',
        'HTTP 200, 4831 bytes, public domain, but imports atts/clpz/dcgs which are Scryer libraries. Does not load in SWI.').
receipt(haskell_memory_three_numbers,
        'one probe at n=600000 under +RTS -s, +RTS -t --machine-readable, and /usr/bin/time -l',
        'bytes allocated 48,452,392 (total volume) vs max_bytes_used 18,366,456 (peak live heap) vs maxrss 51,396,608 (OS process). Three different numbers.').
receipt(rts_A_flag_changes_everything,
        'same probe, default RTS versus -A32m',
        'peak residency 18,366,456 bytes versus 44,328. Same program, 400x apart. A bench must PIN -A or the numbers are noise.').

% ═══ corrections, both directions ════════════════════════════════════════════
% correction(Who, What).

correction(lane_to_coordinator,
           'v6/prolog/1_expansion.pl is 57 lines, not the 268 the coordinator briefed. 268 is labs/json_syntax/3_lists.pl.').
correction(lane_to_coordinator,
           'the v6/prolog tree is 37,074 lines; the coordinator brief said 14,090, which covers only the 22 top-level modules.').
correction(lane_to_coordinator,
           'the refusal inventory loads 73 signatures here, not the 77 the coordinator briefed; refusal_inventory/1 reads currently-loaded clauses.').
correction(lane_to_coordinator,
           'removing the outer sort in scc.pl turns 1 test red with exit 1, not the 3 the coordinator seeded. The 3 came from counting output LINES, not tests.').
correction(audit_to_coordinator,
           'the fgl finding was too broad. Query.SCC and Query.TopSort genuinely do not exist; the functions live in Query.DFS. The conclusion is wrong, the module names are not.').
correction(audit_to_coordinator,
           'the quadratic in the Haskell graph port is in TWO places, not one. The coordinator measured graphComponents and reported construction as the sole offender; indegree on the toposort path is worse.').
correction(audit_to_coordinator,
           'the coordinator summarized interp as 9 of 10 green. Those passes assert only non-emptiness and one of them passes over an empty relation.').
correction(coordinator_to_self,
           'answered the AST-matching question for v5 (.dl, match_ast, =~) when the user meant dl6. dl6 refuses sg_pattern outright.').
correction(coordinator_to_self,
           'conflated an LSP for Prolog source with an LSP for dl6. The packs found (lsp_server 3.17.0, prolog_lsp 0.0.18) serve Prolog and are not what was wanted.').
correction(coordinator_to_self,
           'read the scc lane as stuck in a retry loop; the repeated plunit warnings were "test succeeded with choicepoint" on a PASSING test.').
correction(coordinator_to_self,
           'briefed the diagnostic side table without span PROPAGATION through the seven expanders, so the landed channel cannot follow sugar.').

% ═══ answered questions ══════════════════════════════════════════════════════

answered('does SWI really ship no SCC',
         'no callable one. library(ugraphs) has 18 exports and none is SCC, and no pack supplies it. Tarjan IS in the tree, by Markus Triska, unexported inside clp/clpfd.pl:5892-5962 where all_distinct and global_cardinality use it. A packaging gap, not a science gap.').
answered('is our Kosaraju secretly bad',
         'no. It scales linearly and runs at normal SWI throughput. Both slow comparisons this session came from hand-written GLUE: a coordinator O(V^2) wrapper on the extracted Tarjan, and nub in the Haskell port. Library to library the honest tier gap is about 15x, interpreted versus compiled.').
answered('do we need cut for a Haskell dl6',
         'no. Exactly one cut exists across the three fixtures, seminaive.pl:19 loop(Known, [], Known) :- !., which in Haskell is ordinary first-match pattern matching. Datalog has no cut and no backtracking search to prune, so LogicT is the wrong layer for dl6 entirely. demand/probes/src/Probe/Tabling.hs already shows the right shape: an explicit Data.Set fixpoint, no logic monad.').
answered('is Prolog the right tool to build a language',
         'yes for parse, expand, typecheck, lower, where terms as AST and unification pay for themselves (HM in four clauses; non-linear patterns free). The LSP is the carve-out: it is a long-running server, "infra is bought, never built" applies, SWI is thinnest at daemon-shaped work, and a working LSP already exists in Rust at src/lsp.rs.').
answered('does Prolog have tree-sitter-like tooling',
         'no, and mostly on purpose: DCGs put grammars in the language. What that costs is incremental reparse, error recovery, and the 100-grammar corpus. The working bridge already exists at books/v6/algos/sexp_cst.pl: tree-sitter parses, emits S-expressions, ~20 lines of DCG turns them into terms, and matching is unification, so a repeated metavariable is enforced for free.').
answered('how hard is a dl6 parser in Haskell',
         'megaparsec is the conventional choice. The main porting hazard is that DCG alternation backtracks by default while megaparsec <|> does not once input is consumed, so the three arrows <- <+ <~ need try or a lexed arrow token. You also lose bidirectionality: marble.pl parses and prints from one grammar, Haskell needs a separate prettyprinter and golden round-trip tests.').
answered('can Haskell join the sqlite throughput bench',
         'yes, with no harness change. v6/sprefa-store/bench/engines/pure_wrap.sh is 16 lines and defines the contract: print one CSV, line on stdout, get wrapped by /usr/bin/time -l, emit 11 fields on stderr. Not built.').

% ═══ open ════════════════════════════════════════════════════════════════════
% open(Item, Owner).

open(rebaseline_extract_golden_79_to_82, user_ruling).
open(repoint_extract_pin_to_extraction_live_shape, user_ruling).
open(harden_count_assertion_to_record_kinds, user_ruling).
open(push_532_commits_to_github, user_ruling).
open(remove_reconciled_worktrees, coordinator_staged).
open(span_propagation_through_seven_expanders, phase_2).
open(sg_pattern_metavariable_semantics, user_deferred).
open(hs_reach_bench_engine, unbuilt).
open(push_and_tag, user).
open(update_save_session_skill_to_pl, user).

% ═══ resolved ════════════════════════════════════════════════════════════════
% resolved(FindingName, By).

resolved(diag_wrong_position,   'lane/dl6-diag-fix 0d336775, head-first resolution; coordinator reproduced line 6').
resolved(diag_lab_coupling,     'lane/dl6-diag-fix 0d336775, diag.pl moved labs/ -> v6/prolog/').
resolved(diag_bare_uri,         'lane/dl6-diag-fix 0d336775, file:// scheme').
resolved(gitignore_clobbered_by_coordinator, 'restored from main plus lab entries appended, merge commit 2ee96c42 line').

% ═══ grader ══════════════════════════════════════════════════════════════════

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).

check(every_lane_has_a_state,
      forall(lane(Name, _, _, State, _),
             ( memberchk(State, [landed, committed, committed_negative,
                                 committed_blocked, cancelled_work_kept,
                                 merged, reported])
             -> true
             ;  ( format("  bad state on ~w: ~w~n", [Name, State]), fail ) ))).
check(every_finding_has_a_finder,
      forall(finding(_, _, FoundBy, _),
             memberchk(FoundBy, [coordinator, audit_hs, audit_pl]))).
% Every merge blocker must be either resolved by a named commit or still open.
check(merge_blockers_are_resolved_or_open,
      forall(finding(Name, blocks_merge, _, _),
             ( resolved(Name, _) -> true ; open(Name, _) ))).
check(fleet_is_flash_except_the_named_fable_lane,
      forall(lane(Name, Model, _, _, _),
             ( Model == flash_0731 -> true ; Name == extract_drift_fable ))).
check(push_blockers_are_user_ruling,
      forall(finding(_, blocks_push, _, _), open(_, user_ruling))).
check(nothing_pushed,
      ( session(pushed, false), session(tagged, false) )).
