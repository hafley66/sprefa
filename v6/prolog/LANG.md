# LANG snapshot for labs (2026-07-27) — the candidate language, unbanked

MISSION: a lazy, durable, checkable rxjs-lowering language with highly
efficient RAM usage, for online static analysis of human and AI code alike.
Wanted: good types + matching, good effect model, good time model, good async
model, good relational model, good analysis model. Events come in, state gets
saved, outside observers read sqlite directly (the db IS the API).

STATUS: user verdict "mid, boiling". Labs exist to stress this spec, find
ambiguities/redundancies, and report. Deviating is fine IF the lab's verdict
.md states the deviation and why.

## Surface

- `regexp(text_operand, "pattern")` is a positive body condition. The shared
  subset is literals, character classes, `.`, anchors, `* + ? {n,m}`, groups,
  alternation, and `\d \w \s \b`; Rust-regex and PCRE may differ outside it.
  Rx lowering: `filter(row => /pattern/.test(row.textColumn))`.
- `ast(path, digest, lang, query)` runs one full tree-sitter query host. Known
  languages are `rust`, `ts`, `tsx`, `js`, `go`, and `kotlin`; named captures,
  `line`, and `end_line` are host output columns.

- Keywords: `enum`, `struct`, `rel`, `bind`. Nothing else. (`source`, `fact`,
  `rule`, `external`, `register` all died: inference or unbundling.)
- Column types required: `rel watch(endpoint: Url);`
- Keys: `Key(Type)` wrapper in the column type position, optional 2nd arg =
  compound order: `rel cache(input: Key(Url), entry: Entry);`
  A keyed rel holds at most one row per key; new derivation REPLACES (emits
  -old/+new). Latest-wins is the key's semantics, not a construct.
- Effects = one rel; signature arrow splits program-bound from world-bound
  columns: `rel fetch(endpoint: Url, prev: Tag) -> FetchResult;`
  Demand rows = requests (content-addressed dedup), world fills responses.
  An effect is a lazy rel whose oracle is the world. Envelope enums make the
  fill det: `enum FetchResult { Fresh { tag, body }, Unchanged, Error { status } }`
  Failure is a value; only success carries a body.
- Rules, two arrows (bare clauses, no keyword):
  - `head <- body;`  LEVEL: maintained view over current membership; IVM
    retracts consequences when membership goes away.
  - `head <+ body;`  EDGE: fires on body-atom ARRIVALS this tick, appends;
    consequences never retract (occurrences cannot un-happen).
  A body is one time cut (all atoms at the same instant). Edge rules with
  several atoms fire on ANY atom's arrival joined against others' current
  sets (semi-naive shape) — known consequence: late-subscriber backlog replay.
- Operators: `pre(atom)` = previous tick's row (rare once keys exist);
  `x.field.sub` = nested pattern sugar; `Entry { tag, .. }` struct patterns;
  `name in listexpr` = per-element fan-out (candidate sugar, not sold).
- Facts = bodiless clauses: `watch("repos/cli/cli");`
- Protocols bind at LINK time, separate section/file:
  `bind fetch = shell { \`...\` -> (status, tag, body); match status { 200 => Fresh{tag,body}, 304 => Unchanged, s => Error{status:s} } };`
  Program text never names a transport. Exactly one bind per effect.

## Semantics

- Tick = one sqlite transaction: deltas in -> rules join -> writes -> commit.
  Only deltas cross the sqlite/rx boundary. rx hosts only the yield residue.
- Two implicit time coordinates, both erased unless observed:
  global tick T (phantom column; observed via clock rels / written-at
  fields; delta = d/dT; pre = T-1) and subscription-relative time (lives in
  the sub forest, never in rows, because rows are shared across subscribers).
- Mode type per ask/stream: (cardinality, lifetime); cardinality =
  det/semidet/multi (Mercury), lifetime = finite < until(S) < never;
  lifetime(inner) = min(own, enclosing scope); switch_map is a scope
  constructor. See plans/2026-07-27-mode-dominance.md.
- Keyed-rel conflict law: rules heading a keyed rel must be jointly semidet
  per key per tick (checker discharges; yield points separate seeds from
  transitions).
- Mixed heads are sound under count-IVM (per-row origin support).
- SWR is the cache semantics: key = request INPUT; serve current row
  immediately (level read); staleness triggers revalidation (demand rule);
  fresh response replaces (edge). TTL = a level validity view; invalidation
  = IVM retraction or an edge reset from a world rel.
- Unsubscribe/teardown/laziness are one mechanism: demand rows under sub
  paths; scope exit = range-DELETE of the path prefix.

## Known open questions (find more; do not silently resolve)

- Key(Type) vs `->`-as-FD: BOTH currently exist in notes. Redundant?
- Edge-derived demand rows need arrival-tick salt or repeated identical
  requests dedup into silence.
- Streaming effects (many response rows per demand row) vs det-per-request.
- `in` fan-out sugar; rel blocks (deferred); un-watch cleanup semantics.

## Context files (read before working)

plans/2026-07-27-surface-boil.md, plans/2026-07-27-mode-dominance.md,
v6/prolog/ARCH.pl (callouts), v6/prolog/examples/ghcacher.pl (kernel-4 era
example, PRE-dates this snapshot), v6/prolog/src/{kernel,checks,grader}.pl,
books/v6/enum_match.pl + books/v6/algos/*.pl (lab style exemplars),
examples/gh-cache.dl (the v5 original).

## Lab style laws (non-negotiable)

- Every lab = one self-loading .pl: `swipl -q -l <lab>.pl -g go -g halt`
  exits 0 printing only `PASS <name>` lines (grader.pl's run/1 or the same
  forall shape inline). A lab with any `fail` line is not done.
- Descriptive prolog variables (Endpoint, not E). No single-letter vars.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. No em dashes in .md files.
- Reference semantics IN prolog (unification/fixpoint over terms); sqlite/rx
  lowering is described in the verdict .md, not mocked in code, unless the
  lab is specifically about emission.
- Each lab ships <lab>.md: verdict, deviations from this spec, ambiguities
  found (numbered), and what the finding means for the tier order.
