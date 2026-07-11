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
