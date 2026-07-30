% fixtures/6_relation_depth.pl : relation values nested more than one level
% deep, constructed in a rule head and read back through a pattern or through
% decode/2.
%
% WHY THIS FILE EXISTS. The spine the file-span redesign is built on
% (plans/2026-07-30-file-span-spine-reconciled.md section 3) is depth-3 to
% depth-4 by construction: file_span -> file -> repo. The corpus reached 175
% fixtures covering relation values only at DEPTH 1, and only through world
% arrivals, which go through normalize_relation_reference_rows/3 -- a
% different and working path. Nothing below was covered, and both doors were
% wrong at depth >= 2, in OPPOSITE directions, silently.
%
% ── RED RECEIPTS, taken at 68d1ca3f, before the fix ─────────────────────────
%
% ORACLE (cd v6/prolog/conformance && swipl -q -l go.pl -g go -g halt):
% 176 pass / 10 fail. Ten of the eleven fixtures below were red; the eleventh
% (relation_depth3_many_rows) passed only because its expectations name no
% relation-valued column, and it is red on the emitter instead. Verbatim:
%
%     MISMATCH final file/2
%       got [file(repo(acme),fpath('src/a.rs'))]
%       want [file(obj([name-acme]),obj([name-'src/a.rs']))]
%   fail  relation_depth2_construct_and_read
%     MISMATCH final span/3
%       got [span(file(repo(acme),fpath('src/a.rs')),10,20)]
%       want [span(obj([at-obj([name-'src/a.rs']),repo-obj([name-acme])]),10,20)]
%   fail  relation_depth2_literal_leaf_selects_zero_and_one
%     MISMATCH final dcoord/3
%       got []
%       want [dcoord('src/a.rs',10,20)]
%   fail  relation_depth2_chained_decode
%     MISMATCH final located/2
%       got [located(span(file(repo(acme),fpath('src/a.rs')),10,20),def)]
%       want [located(obj([end-20,file-obj([at-obj([name-'src/a.rs']),
%                                           repo-obj([name-acme])]),start-10]),def)]
%   fail  relation_depth3_construct_and_read
%   fail  relation_pattern_text_literal_in_ref_column_rejected
%   fail  relation_pattern_wrong_target_rejected
%   fail  relation_pattern_target_arity_rejected
%
% The oracle stored a rule-constructed relation value as a plain Prolog
% compound (`repo(acme)`), never as the canonical obj(...) a world arrival
% produces. Two consequences, both silent:
%   (1) ticklog.pl renders a compound as canonical PROLOG text, so the graded
%       bytes were `"repo(acme)"` where the emitter prints `{"name":"acme"}`.
%       Depth 1 diverged the same way; no fixture ever read those bytes.
%   (2) body.pl json_decode/2 has no clause for a compound, so every decode/2
%       over a rule-constructed value failed. Plan section 3.3's
%       "emitter right, oracle empty".
%
% EMITTER (cd v6/tsv2 && bash scripts/sweep.sh), same commit:
% RUN total=133 identical=124 wrong=7, FINAL final_wrong=9, against a clean
% baseline of wrong=0 / final_wrong=2 (both pre-existing). Every depth-2 and
% depth-3 rel came back EMPTY:
%
%   WRONG relation_depth2_construct_and_read first diff at line 1
%     actual {"tick":1,"deltas":{"file":{"add":[[{"name":"acme"},
%              {"name":"src/a.rs"}]]},"fpath":...,"raw":...,"repo":...}}
%     oracle {"tick":1,"deltas":{"coord":{"add":[["src/a.rs",10,20]]},
%              "file":{"add":[["repo(acme)","fpath(src/a.rs)"]]},...,
%              "span":{"add":[["file(repo(acme),fpath(src/a.rs))",10,20]]}}}
%
% Two separate defects visible in that one line. `span` and `coord` are
% missing entirely from the emitted side: depth-1 construction emitted the
% correct endpoint projection, but depth 2 emitted
% `json_extract(b1."repo", '$.fn') = 'repo'` against b1."repo", which is the
% INTEGER __id the depth-1 statement had just written.
% json_extract(<integer>, '$.fn') is NULL, measured, so the WHERE was never
% true and the rel was permanently empty with no refusal. Plan section 3.1's
% "oracle right, emitter empty". And `file` disagrees at DEPTH 1 on the bytes
% -- `{"name":"acme"}` against `"repo(acme)"` -- which is the oracle-side
% defect above, never before graded because no fixture built a relation value
% in a rule head.
%
% The three refusal fixtures COMPILED CLEAN on the emitter at that commit
% (manifest bucket `compiled`), which is exactly the silence 3.2 describes.
%
% ── WHAT THE FIX PINS ───────────────────────────────────────────────────────
%
% One relation model, both doors (0_type_plane.pl header): a rel column naming
% another rel is a generated relational edge, and a pattern reaching through it
% is a join chain, one hop per level.
%
%   oracle    a relation-shaped term in a rule head or body argument over a
%             ref-typed column is SUGAR for the canonical obj(...) value
%             (0_relation_pattern.pl), so a rule-constructed value and a
%             world-arrived value are the same term at every depth.
%   emitter   the same term lowers to a `__ref_<type>` dictionary atom per
%             level (lower.pl expand_relation_pattern_rules/3), exactly the
%             shape decode/2 already lowered to -- one hop, keyed on __id.
%
% EXPLAIN receipts for the emitted join chain live in
% v6/tsv2/tests/relationDepth.test.ts: every hop is SEARCH, never SCAN.
%
% Owner: depth-2 relation-pattern lane.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ depth 2: build, then read back through a pattern ═══════════════════════
%
% repo/fpath are the leaves, file is one level, span is two. `raw` is the flat
% world row every rule reads; nothing here arrives as a nested object, so
% every relation value below is BUILT by a rule -- the path with no coverage.

% ONE row per level. The `coord` rule reaches two levels down (span -> file ->
% fpath) inside a single body atom; `file(_, fpath(PathName))` is the pattern
% the emitter used to json_extract out of an integer.
fixture(relation_depth2_construct_and_read,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
         col_type(raw/4, start, int), col_type(raw/4, end, int),
         col_type(coord/3, path_name, text),
         col_type(coord/3, start, int), col_type(coord/3, end, int) ],
       [ (repo(RepoName)  <- raw(RepoName, _, _, _)),
         (fpath(PathName) <- raw(_, PathName, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             raw(RepoName2, PathName2, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             raw(RepoName3, PathName3, Start, End)),
         (coord(PathName4, Start2, End2) <-
             span(file(_, fpath(PathName4)), Start2, End2)) ]),
  [],
  [ [ +raw(acme, 'src/a.rs', 10, 20) ] ],
  [ final(repo/1,  [ repo(acme) ]),
    final(fpath/1, [ fpath('src/a.rs') ]),
    final(file/2,  [ file(obj([name-acme]), obj([name-'src/a.rs'])) ]),
    final(span/3,  [ span(obj([at-obj([name-'src/a.rs']),
                               repo-obj([name-acme])]), 10, 20) ]),
    final(coord/3, [ coord('src/a.rs', 10, 20) ]),
    ticks(1) ]).

% ZERO rows out of a depth-2 read that is NOT vacuous: every upstream rel
% carries a row and a LITERAL leaf two levels down selects none of them. The
% sibling rule with the matching literal is in the same program, so a lowering
% that answers nothing for both is caught by the `hit` half.
fixture(relation_depth2_literal_leaf_selects_zero_and_one,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
         col_type(raw/4, start, int), col_type(raw/4, end, int),
         col_type(hit/1, start, int),
         col_type(miss/1, start, int) ],
       [ (repo(RepoName)  <- raw(RepoName, _, _, _)),
         (fpath(PathName) <- raw(_, PathName, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             raw(RepoName2, PathName2, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             raw(RepoName3, PathName3, Start, End)),
         (hit(Start2)  <- span(file(repo(acme),   _), Start2, _)),
         (miss(Start3) <- span(file(repo(globex), _), Start3, _)) ]),
  [],
  [ [ +raw(acme, 'src/a.rs', 10, 20) ] ],
  [ final(hit/1,  [ hit(10) ]),
    final(miss/1, []),
    final(span/3, [ span(obj([at-obj([name-'src/a.rs']),
                              repo-obj([name-acme])]), 10, 20) ]),
    ticks(1) ]).

% MANY rows per level, with sharing: two files under one repo, three spans,
% three distinct paths. Identity is content-addressed, so the shared `acme`
% leaf must be ONE row that two file rows point at -- a per-parent copy would
% show up here as two repo rows.
fixture(relation_depth2_many_rows_share_one_leaf,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
         col_type(raw/4, start, int), col_type(raw/4, end, int),
         col_type(coord/3, path_name, text),
         col_type(coord/3, start, int), col_type(coord/3, end, int) ],
       [ (repo(RepoName)  <- raw(RepoName, _, _, _)),
         (fpath(PathName) <- raw(_, PathName, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             raw(RepoName2, PathName2, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             raw(RepoName3, PathName3, Start, End)),
         (coord(PathName4, Start2, End2) <-
             span(file(_, fpath(PathName4)), Start2, End2)) ]),
  [],
  [ [ +raw(acme, 'a.rs', 1, 2), +raw(acme, 'b.rs', 3, 4),
      +raw(globex, 'c.rs', 5, 6) ] ],
  [ final(repo/1,  [ repo(acme), repo(globex) ]),
    final(fpath/1, [ fpath('a.rs'), fpath('b.rs'), fpath('c.rs') ]),
    final(file/2,  [ file(obj([name-acme]),   obj([name-'a.rs'])),
                     file(obj([name-acme]),   obj([name-'b.rs'])),
                     file(obj([name-globex]), obj([name-'c.rs'])) ]),
    final(coord/3, [ coord('a.rs', 1, 2), coord('b.rs', 3, 4),
                     coord('c.rs', 5, 6) ]),
    ticks(1) ]).

% ═══ depth 2: read back through decode/2 ════════════════════════════════════

% Plan section 3.3, the inverted divergence. Same construction, the other
% dereference spelling: two decode/2 goals chained through the intermediate
% value. The emitter already produced the correct two-hop join here; the
% oracle derived nothing, because the value it stored was a compound term
% json_decode/2 has no clause for.
fixture(relation_depth2_chained_decode,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
         col_type(raw/4, start, int), col_type(raw/4, end, int),
         col_type(dcoord/3, path_name, text),
         col_type(dcoord/3, start, int), col_type(dcoord/3, end, int) ],
       [ (repo(RepoName)  <- raw(RepoName, _, _, _)),
         (fpath(PathName) <- raw(_, PathName, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             raw(RepoName2, PathName2, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             raw(RepoName3, PathName3, Start, End)),
         (dcoord(PathName4, Start2, End2) <-
             span(File, Start2, End2),
             decode(File, {at: At}),
             decode(At, {name: PathName4})) ]),
  [],
  [ [ +raw(acme, 'src/a.rs', 10, 20) ] ],
  [ final(dcoord/3, [ dcoord('src/a.rs', 10, 20) ]),
    ticks(1) ]).

% The same read written as ONE decode with a NESTED object pattern.
% decode_slot/6 already recursed on the emitted side; the oracle half had
% never been reached over a rule-constructed value. Both spellings answer the
% same rows, and the zero-arrival tick beside it is the non-vacuous
% zero-cardinality case for the decode spelling: the tick runs, the join
% executes, no rows come back.
fixture(relation_depth2_nested_decode_pattern,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
         col_type(raw/4, start, int), col_type(raw/4, end, int),
         col_type(dcoord/3, path_name, text),
         col_type(dcoord/3, start, int), col_type(dcoord/3, end, int) ],
       [ (repo(RepoName)  <- raw(RepoName, _, _, _)),
         (fpath(PathName) <- raw(_, PathName, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             raw(RepoName2, PathName2, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             raw(RepoName3, PathName3, Start, End)),
         (dcoord(PathName4, Start2, End2) <-
             span(File, Start2, End2),
             decode(File, {at: {name: PathName4}})) ]),
  [],
  [ [],
    [ +raw(acme, 'src/a.rs', 10, 20) ] ],
  [ deltas(dcoord/3, [ [], [ +dcoord('src/a.rs', 10, 20) ] ]),
    final(dcoord/3, [ dcoord('src/a.rs', 10, 20) ]),
    ticks(2) ]).

% ═══ depth 3: the spine's real shape ════════════════════════════════════════
% located -> span -> file -> repo|fpath. The head builds three levels in one
% term; the body reads three levels back out of one atom.

fixture(relation_depth3_construct_and_read,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         type_decl(located, [col(span, span), col(kind, text)]),
         col_type(located/2, span, span), col_type(located/2, kind, text),
         col_type(rawk/5, repo_name, text), col_type(rawk/5, path_name, text),
         col_type(rawk/5, start, int), col_type(rawk/5, end, int),
         col_type(rawk/5, kind, text),
         col_type(found/2, path_name, text), col_type(found/2, kind, text) ],
       [ (repo(RepoName)  <- rawk(RepoName, _, _, _, _)),
         (fpath(PathName) <- rawk(_, PathName, _, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             rawk(RepoName2, PathName2, _, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             rawk(RepoName3, PathName3, Start, End, _)),
         (located(span(file(repo(RepoName4), fpath(PathName4)), Start2, End2),
                  Kind) <-
             rawk(RepoName4, PathName4, Start2, End2, Kind)),
         (found(PathName5, Kind2) <-
             located(span(file(_, fpath(PathName5)), _, _), Kind2)) ]),
  [],
  [ [ +rawk(acme, 'src/a.rs', 10, 20, def) ] ],
  [ final(located/2,
          [ located(obj([end-20,
                         file-obj([at-obj([name-'src/a.rs']),
                                   repo-obj([name-acme])]),
                         start-10]), def) ]),
    final(found/2, [ found('src/a.rs', def) ]),
    ticks(1) ]).

% Depth 3 through a three-hop decode chain, and through the equivalent single
% nested pattern, in one program. `located(Span, Kind)` binds the span-typed
% column, so the chain is span -> file -> fpath. Both rules answer the same
% row.
fixture(relation_depth3_chained_decode,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         type_decl(located, [col(span, span), col(kind, text)]),
         col_type(located/2, span, span), col_type(located/2, kind, text),
         col_type(rawk/5, repo_name, text), col_type(rawk/5, path_name, text),
         col_type(rawk/5, start, int), col_type(rawk/5, end, int),
         col_type(rawk/5, kind, text),
         col_type(dfound/2, path_name, text), col_type(dfound/2, kind, text),
         col_type(nfound/2, path_name, text), col_type(nfound/2, kind, text) ],
       [ (repo(RepoName)  <- rawk(RepoName, _, _, _, _)),
         (fpath(PathName) <- rawk(_, PathName, _, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             rawk(RepoName2, PathName2, _, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             rawk(RepoName3, PathName3, Start, End, _)),
         (located(span(file(repo(RepoName4), fpath(PathName4)), Start2, End2),
                  Kind) <-
             rawk(RepoName4, PathName4, Start2, End2, Kind)),
         (dfound(PathName5, Kind2) <-
             located(Span, Kind2),
             decode(Span, {file: File}),
             decode(File, {at: At}),
             decode(At, {name: PathName5})),
         (nfound(PathName6, Kind3) <-
             located(Span2, Kind3),
             decode(Span2, {file: {at: {name: PathName6}}})) ]),
  [],
  [ [ +rawk(acme, 'src/a.rs', 10, 20, def) ] ],
  [ final(dfound/2, [ dfound('src/a.rs', def) ]),
    final(nfound/2, [ nfound('src/a.rs', def) ]),
    ticks(1) ]).

% MANY rows at depth 3 with leaf sharing across branches: one repo, two paths,
% three located rows (two kinds over one span). The depth-3 read returns one
% row per located row and no cross product.
fixture(relation_depth3_many_rows,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         type_decl(span,  [col(file, file), col(start, int), col(end, int)]),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         type_decl(located, [col(span, span), col(kind, text)]),
         col_type(located/2, span, span), col_type(located/2, kind, text),
         col_type(rawk/5, repo_name, text), col_type(rawk/5, path_name, text),
         col_type(rawk/5, start, int), col_type(rawk/5, end, int),
         col_type(rawk/5, kind, text),
         col_type(found/2, path_name, text), col_type(found/2, kind, text) ],
       [ (repo(RepoName)  <- rawk(RepoName, _, _, _, _)),
         (fpath(PathName) <- rawk(_, PathName, _, _, _)),
         (file(repo(RepoName2), fpath(PathName2)) <-
             rawk(RepoName2, PathName2, _, _, _)),
         (span(file(repo(RepoName3), fpath(PathName3)), Start, End) <-
             rawk(RepoName3, PathName3, Start, End, _)),
         (located(span(file(repo(RepoName4), fpath(PathName4)), Start2, End2),
                  Kind) <-
             rawk(RepoName4, PathName4, Start2, End2, Kind)),
         (found(PathName5, Kind2) <-
             located(span(file(_, fpath(PathName5)), _, _), Kind2)) ]),
  [],
  [ [ +rawk(acme, 'a.rs', 1, 2, def),
      +rawk(acme, 'a.rs', 1, 2, ref),
      +rawk(acme, 'b.rs', 3, 4, def) ] ],
  [ final(repo/1,  [ repo(acme) ]),
    final(fpath/1, [ fpath('a.rs'), fpath('b.rs') ]),
    final(found/2, [ found('a.rs', def), found('a.rs', ref),
                     found('b.rs', def) ]),
    ticks(1) ]).

% ═══ the guard that was an accident ═════════════════════════════════════════
%
% Plan section 3.2. A ref-typed column read with a non-relation argument used
% to compile into `json_extract(<integer>, ...)`. It was caught ONLY when the
% phantom text expression happened to land against an INT column
% (join_column_type_mismatch), a coincidence of the OTHER operand's typing and
% not a check on ref columns at all. Against a text column it compiled clean
% and answered nothing.
%
% All three shapes below are now the same named refusal on both doors, and the
% refusal names the ref column rather than whatever the phantom collided with.

% A bare text literal where a relation value belongs.
fixture(relation_pattern_text_literal_in_ref_column_rejected,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(seen/1, start, int) ],
       [ (seen(Start) <- span('src/a.rs', Start, _)) ]),
  [],
  [],
  [ throws(relation_pattern_not_a_relation_value(
             span/3, file, file, 'src/a.rs')) ]).

% The right shape under the WRONG relation name. `fpath('a.rs')` is a declared
% relation value, just not this column's.
fixture(relation_pattern_wrong_target_rejected,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(seen/1, start, int) ],
       [ (seen(Start) <- span(fpath('a.rs'), Start, _)) ]),
  [],
  [],
  [ throws(relation_pattern_not_a_relation_value(
             span/3, file, file, fpath('a.rs'))) ]).

% The right name at the wrong arity. A pattern naming fewer columns than the
% target declares is not a partial read: the column set is what the relation
% IS, and a one-column `file` is a different shape, not a prefix of this one.
fixture(relation_pattern_target_arity_rejected,
  prog([ type_decl(repo,  [col(name, text)]),
         col_type(repo/1, name, text),
         type_decl(fpath, [col(name, text)]),
         col_type(fpath/1, name, text),
         type_decl(file,  [col(repo, repo), col(at, fpath)]),
         col_type(file/2, repo, repo), col_type(file/2, at, fpath),
         col_type(span/3, file, file),
         col_type(span/3, start, int), col_type(span/3, end, int),
         col_type(seen/1, start, int) ],
       [ (seen(Start) <- span(file(repo(acme)), Start, _)) ]),
  [],
  [],
  [ throws(relation_pattern_not_a_relation_value(
             span/3, file, file, file(repo(acme)))) ]).
