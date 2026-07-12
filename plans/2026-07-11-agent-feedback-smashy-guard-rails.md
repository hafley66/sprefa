# Agent-session feedback: smashy guard-match + property-registry rails (2026-07-11)

Sonnet subagent authored two rails in ~/projects/smashy (committed there:
`.dl/lint-guarded-match.dl`, `.dl/lint-property-registry.dl`, `docs/property-registry.md`;
smashy commits 4037dad + 8e185f2). Task: join `comment_node` dl-guard markers against
`sg` match-block spans and `agent_edit`/`agent_touch`, plus a jsonp-vs-doc property-name
sync rail. Capture only, untriaged.

## Worked first try

- Bare `scan` seeding extraction; `sg(:rust, "match $E { $$$ARMS }")` span capture.
- `jsonp` dotted-path wildcards over a real nested game.json
  (`layers.*.declarations.*.Property.property`).
- kwargs `diag` heads; `${var}` interpolation in msgs.
- `--parse-only` fast-fail; `dl daemon rows <rel>`; `dl daemon restart` after staleness.

## Gotchas hit (docs/ergonomics candidates)

- **`match()` trailing positional is a match ID, not captured text** — bound a big int
  (`10995035168051492288`); `split()` on it silently produced 0 rows. Fix: named capture
  groups `(?<name>...)` bind dl vars directly. Costliest failure of the session, made
  worse by shipped-rail precedent: `lint-box-word-ban.dl` names that column `hit`, which
  reads as "matched text". Candidates: rename the convention to `mid`/`match_id` in
  embedded examples; show the named-capture form in the op quickref row; or add a real
  capture-text mode.
- **`comment_node.text` strips comment tokens** — `t =~ /^\s*\/\/\s*dl-guard:/` silently
  zero-matched. Fix: `t =~ /^\s*dl-guard:/`. Documented only in a buried
  `dl docs authoring` bullet under the regex `comment` op; absent from the relations
  table agents check first.
- **`sg` needs its `scan(...)` inline in the same rule body.** Feeding `(path, rev)` from
  a derived rel failed two ways across two agents: `source rule ... missing scan` error
  in one shape, silent zero rows in another. Not documented; worth an explicit line in
  the `sg` docs entry. (Repeat of the S-batch finding; still biting.)
- **`!(expr)` is not syntax** — `!(f =~ /.../)` errors `expected identifier, got LParen`
  with no hint that the fix is a hoisted `foo_excluded` rel to negate. Error-message
  candidate: suggest the helper-rel idiom.
- **The "put the expression in the head" note misfires as advice** — the
  `unbound var in constraint` error fired on a plain `=~` constraint whose real bug was
  mixing two independent source ops (comment_node + an unrelated scan) in one rule body.
  The note pointed at the arithmetic-in-head model; the actual fix was one source op per
  rule. Candidate: name the trigger condition precisely.
- **`//` inside a regex literal parses as a comment start** —
  `error: expected , or . in rule body, got Regex(...)`. Fix: escape `\/\/`. A note in
  the regex doc row would save the cycle.
- **`?` query grammar is bare `?rel(vars).` only** — Prolog-style `?head(v) :- filter`
  errors `expected Dot, got Colon`; there is no `==` operator (equality is
  shared-variable joins). The syntax table doesn't say this is the entire grammar.
- **Arithmetic head-only rule tripped again** (`nl = gel + 1` in body). Documented, but
  the second most-hit trap across sessions.

## Daemon

- Unflagged `dl prog.dl` against an attached daemon merges through the full `.dl/*.dl`
  discovery corpus instead of running the single program; `--no-daemon` gives the
  expected isolation. Quick-start doesn't mention the merge. No 137/SIGKILL this session.

## agent_edit / agent_touch (biggest capability gap for this rail class)

- Both returned **0 rows inside a spawned subagent** even after real Edit tool calls on
  tracked files in the same root, same run. Unresolved: session-store keyed to an
  interactive harness session id the subagent doesn't share? needs a flush? TRIAGE.
  Docs candidate: one line on what writes the store (which hook, what triggers a flush)
  so agents can distinguish "genuinely empty" from "wrong session key".
- **No line/span columns** — `agent_edit(harness, session, idx, path)` /
  `agent_touch(harness, session, path)` are file-level only, so a guarded-MATCH rail
  degrades to a guarded-FILE rail. Feature candidate: carry edit spans so
  touched-region x AST-span joins work. This is the difference between "you touched a
  file containing a guarded dispatch" and "you edited the guarded dispatch".
- Workaround used to prove the diag path: fabricated `fake_touch(path)` fact in a
  scratch program, identical joins — fired correctly (1 row, right span, right reason).

## Their verdict

Span joins, sg, jsonp, and the diag sink are solid; the two rails shipped and gate via
`dl --check`. The rail class "escalate when an agent edits a marked region" is currently
capped by agent_edit's file granularity and subagent invisibility — both fixable on the
sprefa side, and the highest-value items in this batch.

## Addendum: gen-rail session, same day (TODOS.md aggregation rail)

Second Sonnet session, `.dl/gen-todos.dl` (smashy commit 9531a9d). New items only:

- **Named captures `(?<name>...)` bind only via the `match` source op.** In a plain `=~`
  body constraint on an already-bound var they silently don't bind: `--parse-only` accepts,
  full run errors `unbound variable <name>`. Workaround: `replace_re(text, ..., "$1")` in
  the rule head. Candidate: either bind captures from `=~` too, or reject them at parse.
- **`gen` row order is the rendered output text**, not any sort-key column bound in the
  body (tested: bound or `_`, zero effect). Ordering requires baking the sort prefix into
  the rendered string. Candidate: document, or accept an order-by column.
- **`gen` interpolation is `{var}`, not `${var}`** — the diag/string-head sigil in a gen
  template fails at RUNTIME (`gen expects a template string here, got Interp(...)`), not
  at parse. Candidate: parse-time check.
- **Version drift broke two shipped rails on this repo**: `file_lines` became a reserved
  builtin name (rail's own rel had to rename), and a source rule mixing `scan(...)` with a
  negated relation atom now fails "source rules cannot join relations" (split into
  bare-scan rel + derived filter). Both fixed repo-side; flagging because reserved-name
  additions and source-rule tightening are silent breaking changes for existing rails —
  a `dl --check` upgrade note or migration lint would catch these.

## Triage todos (added 2026-07-11, from the capture above)

<!-- todo(bug): agent_edit/agent_touch return 0 rows inside a spawned subagent — session-store keying or flush; document what writes the store -->
<!-- todo(feature): agent_edit carries line/span columns so touched-region x AST-span joins work (guarded-MATCH vs guarded-FILE) -->
<!-- todo(bug): sg source rule fed (path, rev) from a derived rel fails two inconsistent ways (error in one shape, silent zero rows in another) — make it one loud error or support it; repeat of the S-batch finding -->
<!-- todo(docs): match() trailing positional is a match ID not captured text — rename convention to match_id in shipped examples, show named-capture form in the op quickref -->
<!-- todo(docs): comment_node.text strips comment tokens — state it in the relations table row, not only the buried authoring bullet -->
<!-- todo(feature): error-message for !(expr) — suggest the hoisted helper-rel negation idiom -->
<!-- todo(docs): unbound-var-in-constraint note names the wrong fix when the real bug is two source ops in one rule body — name the trigger condition precisely -->
<!-- todo(docs): // inside a regex literal parses as comment start — note the \/\/ escape in the regex doc row -->
<!-- todo(docs): query grammar is bare ?rel(vars). only — syntax table should say so (no :- , no ==) -->
<!-- todo(docs): quick-start states that an unflagged run against an attached daemon merges the full .dl/ discovery corpus; --no-daemon isolates -->

## Addendum 2: ban-word rail session (2026-07-11 evening, struct-field sweep)

Rail found 9 struct-field hits, zero comment/doc/string false positives. New
gotchas surfaced (none in the skill yet):

- `ast_yaml` `inside:` defaults to the IMMEDIATE parent, not any ancestor —
  verified empirically; this is exactly what makes
  `kind: type_identifier, inside: {kind: struct_item}` precise.
- `ast_yaml` RuleCore atomics: pattern, kind, regex, nthChild, range, inside,
  has, precedes, follows, all, any, not, matches — NO `field:` selector. "The
  name field of a function_item" must be reached as `kind: identifier,
  inside: {kind: function_item}`.
- Under `(?i)` a character class folds case too (`[A-Z0-9]` becomes
  `[A-Za-z0-9]`), silently defeating an uppercase-boundary check — produced a
  REAL false positive (`jumpstart_unrelated`). Fix shape: three case-exact
  branches unioned instead of one `(?i)` regex.
- The ban-word skill's "\b works for snake_case" claim did not hold under
  test (documented in the rail header) — backprop to that skill's
  word-boundary bullet.

<!-- todo(docs): skill/authoring — ast_yaml inside: is immediate-parent only; state it in the ast_yaml doc row -->
<!-- todo(docs): skill/authoring — ast_yaml RuleCore has no field: selector; document the kind+inside idiom -->
<!-- todo(docs): skill/authoring — (?i) folds character classes too; uppercase-boundary checks need case-exact branches -->
<!-- todo(backprop): ban-word skill word-boundary bullet overclaims \b for snake_case; correct with the case-exact-branch pattern -->
