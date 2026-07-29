% fixtures.pl -- the fixture/5 CANDIDATES distilled from the two receipt
% programs, graded here by the REAL oracle engine (grade_fixtures.pl runs the
% same engine:fixture_expectations_hold/2 that conformance/go.pl runs), and
% recorded in the verdict for promotion rather than promoted here. Promoting
% them inside the lab would move the conformance count in a lab commit that is
% about to be deleted; the lab protocol's precedent (consumption-arms,
% update-arm) is candidates + a recoverable hash.
%
% Each one pins a capability the receipt programs exercised live, reduced to
% rows so it grades without a host, a watcher or a subprocess. The HOST half is
% deliberately absent: what these fixtures own is the LANGUAGE-side shape of
% the seven comment techniques, and every one of those shapes is an ordinary
% join, antijoin or int expression.
%
% SABOTAGE RECEIPTS, run then reverted, one per fixture, so none of the four is
% a fixture that cannot fail:
%   F1  drop `comment_node(Path, Line)` from the body
%       -> got 2 rows including the string-literal false positive, want 1. RED.
%   F2  `Line + 1` -> `Line + 0`   -> got line 2, want line 3. RED.
%       (NOTE, disclosed: dropping F2's witness atom instead leaves it GREEN --
%        F2 pins the ARITHMETIC and nothing else, and F1 is the fixture that
%        owns the witness. Neither is a substitute for the other.)
%   F3  `not(rail_finding(...))` -> `true` -> got 2 rows, want 1. RED.
%   F4  `not(arch_url(Parent))` -> `true`  -> got 3 roots, want 1. RED.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ F1: the grammar witness ════════════════════════════════════════════════
%
% THE fixture of this lab. A scanner hit becomes a fact only where the parser
% put a comment on the same line. Row 2 is the real false positive
% string-safety.sh witnesses in this repository's own sources
% (`v6/tsv2/cli/bop.ts:172`, a `//` inside `"http://127.0.0.1:..."`), reduced
% to two rows.
%
%   .dl6, with its rx lowering (the snippet law):
%     rel arch_node(path: text, line: int, url: text).
%     arch_node(path, line, url) <- marker_hit(path, line, url),
%                                   comment_node(path, line).
%
%     const archNode$ = combineLatest([markerHit$, commentNode$]).pipe(
%       map(([hits, comments]) => {
%         const onLine = new Set(comments.map((row) => `${row.path}:${row.line}`));
%         return hits.filter((hit) => onLine.has(`${hit.path}:${hit.line}`));
%       }),
%       distinctUntilChanged(sameRowSet));
fixture(comment_witness_gates_a_scanner_hit,
  prog([ col_type(marker_hit/3, path, text),
         col_type(marker_hit/3, line, int),
         col_type(marker_hit/3, url, text),
         col_type(comment_node/2, path, text),
         col_type(comment_node/2, line, int),
         col_type(arch_node/3, path, text),
         col_type(arch_node/3, line, int),
         col_type(arch_node/3, url, text) ],
       [ (arch_node(Path, Line, Url) <-
            (marker_hit(Path, Line, Url), comment_node(Path, Line))) ]),
  [],
  [ [ +marker_hit('bop.ts', 4, 'real/one'),
      +marker_hit('bop.ts', 172, 'inside/a/string'),
      +comment_node('bop.ts', 4) ] ],
  [ final(arch_node/3, [arch_node('bop.ts', 4, 'real/one')]) ]).

% ═══ F2: dl-disable-next-line is + 1, in the language ═══════════════════════
%
% v5 writes `suppress_line(p, l1, c) <- scoped(p, l, "next", c), l1 = l + 1`.
% v6 writes `effect_line := line + 1`. The point of pinning it is that the
% offset is the LANGUAGE'S arithmetic and not something a host computed: a
% marker host that emitted the effect line itself would pass the receipt and
% fail this fixture.
fixture(disable_next_line_shifts_the_effect_by_one,
  prog([ col_type(directive_next/3, path, text),
         col_type(directive_next/3, line, int),
         col_type(directive_next/3, code, text),
         col_type(comment_node/2, path, text),
         col_type(comment_node/2, line, int),
         col_type(suppressed/3, path, text),
         col_type(suppressed/3, line, int),
         col_type(suppressed/3, code, text) ],
       [ (suppressed(Path, Effect, Code) <-
            (directive_next(Path, Line, Code),
             comment_node(Path, Line),
             Effect := Line + 1)) ]),
  [],
  [ [ +directive_next('a.ts', 2, 'no-eval'),
      +comment_node('a.ts', 2) ] ],
  [ final(suppressed/3, [suppressed('a.ts', 3, 'no-eval')]) ]).

% ═══ F3: the unused-suppression antijoin ════════════════════════════════════
%
% eslint's reportUnusedDisableDirectives, which std/suppress.dl exports as
% `suppress_unused`. Two directives, one guarding a real finding and one
% guarding nothing; only the second warns.
fixture(unused_suppression_antijoins_the_finding,
  prog([ col_type(suppressed/3, path, text),
         col_type(suppressed/3, line, int),
         col_type(suppressed/3, code, text),
         col_type(rail_finding/3, path, text),
         col_type(rail_finding/3, line, int),
         col_type(rail_finding/3, code, text),
         col_type(suppress_unused/3, path, text),
         col_type(suppress_unused/3, line, int),
         col_type(suppress_unused/3, code, text) ],
       [ (suppress_unused(Path, Line, Code) <-
            (suppressed(Path, Line, Code),
             not(rail_finding(Path, Line, Code)))) ]),
  [],
  [ [ +suppressed('a.ts', 2, 'no-eval'),
      +suppressed('b.ts', 2, 'no-eval'),
      +rail_finding('a.ts', 2, 'no-eval') ] ],
  [ final(suppress_unused/3, [suppress_unused('b.ts', 2, 'no-eval')]) ]).

% ═══ F4: the ARCH hierarchy is joins, once the url is decomposed ════════════
%
% std/arch.dl derives `arch_parent` with `replace_re(url, r"/[^/]*$", "")`.
% v6 has no text operation, so `parent` arrives as a column and everything
% BUILT ON IT -- edges, roots, per-parent child counts -- is ordinary datalog.
% Pinning that split is the honest statement of where the port stands: the
% hierarchy ported, the string surgery did not.
fixture(arch_hierarchy_from_decomposed_marker_rows,
  prog([ col_type(arch_node/3, path, text),
         col_type(arch_node/3, url, text),
         col_type(arch_node/3, parent, text),
         col_type(arch_url/1, url, text),
         col_type(arch_edge/2, parent, text),
         col_type(arch_edge/2, child, text),
         col_type(arch_root/1, url, text),
         col_type(arch_child_count/2, parent, text),
         col_type(arch_child_count/2, children, int) ],
       [ (arch_url(Url) <- arch_node(_, Url, _)),
         (arch_edge(Parent, Url) <-
            (arch_node(_, Url, Parent), arch_url(Parent))),
         (arch_root(Url) <-
            (arch_node(_, Url, Parent), not(arch_url(Parent)))),
         (arch_child_count(Parent, count(Child)) <- arch_edge(Parent, Child)) ]),
  [],
  [ [ +arch_node('demo.pl', 'sprefa/compile/01-lower', 'sprefa/compile'),
      +arch_node('demo.pl', 'sprefa/compile/01-lower/00-entry', 'sprefa/compile/01-lower'),
      +arch_node('demo.pl', 'sprefa/compile/01-lower/01-emit', 'sprefa/compile/01-lower') ] ],
  [ final(arch_edge/2,
          [ arch_edge('sprefa/compile/01-lower', 'sprefa/compile/01-lower/00-entry'),
            arch_edge('sprefa/compile/01-lower', 'sprefa/compile/01-lower/01-emit') ]),
    final(arch_root/1, [arch_root('sprefa/compile/01-lower')]),
    final(arch_child_count/2, [arch_child_count('sprefa/compile/01-lower', 2)]) ]).
