# v6 surface language — boiling pot (NOT banked)

Status 2026-07-27: user verdict "mid, needs more boiling time." Nothing here
is decided unless ARCH.pl says so. This file exists so no candidate is lost.

## Converged-so-far (leading candidates, still unbanked)

- Keywords: `enum`, `struct`, `rel`, `bind`. `source`/`fact`/`rule`/`external`/
  `register` all died (inference or unbundling).
- Arrows: `<-` level rule (maintained view, IVM retracts), `<+` edge rule
  (fires on body arrivals, append-only, never retracts). `delta()` wrapper
  dead. `pre` a body operator, rarely needed once keys exist.
- Effects = one rel; signature arrow splits program-bound / world-bound
  columns (adornment = prolog modes = magic demand; effect = lazy rel whose
  oracle is the world). Envelope enum makes fill det.
- Scan/register unbundled: event rel + keyed rel + transition rules.
  Exhaustiveness inverts to a lint (no rule consumes Error(_) - deliberate?).
- Keyed-rel conflict rule: rules must be jointly semidet per key per tick
  (mode analysis discharges; yield points separate seed from transitions).
- Protocols bind at link time (banked in ARCH f91b9dbb).
- Mixed heads sound under count-IVM (banked in ARCH f91b9dbb).

## Open — syntax

- KEY MARKER: `=>` rejected (JS flow connotation). USER CALL 2026-07-27:
  `Key(Type)` wrapper in the column's type position, optional 2nd arg =
  compound key order: `rel cache(input: Key(Url), entry: Entry)`,
  `rel edge(a: Key(Str, 1), b: Key(Int, 2))`. Keys are types, which fits
  "types and rels are the same thing". Still open: multi-valued right side,
  key-change-over-time semantics.
- ITERATION: `{k, v} in [...]` fan-out not sold. Fallback = one plain edge
  rule per field (v5 style, costs a duplicated line). `in` stays candidate
  sugar only.
- REL BLOCK: user-deferred to LATER (2026-07-27). Sketch kept:
  `rel(endpoint: Url) { rel watch(); rel fetch(prev: Tag) -> FetchResult; rel cache(input: Entry); }`
  Inner rels get block columns prepended; first-order instantiation by
  linking twice with different binds.
- Edge-derived demand rows must salt with arrival tick or repeated identical
  requests dedup into silence (the bust(endpoint) refetch case).

## Open — semantics

- SWR is the cache semantics wanted: key = request INPUT (not endpoint per
  se), serve current row immediately (level read), staleness triggers
  background revalidation (demand rule), fresh response replaces (edge).
  TTL hard-expiry and serve-stale windows = two level views over the same
  written-at field; invalidation = IVM retraction of a validity view, or an
  edge reset from a world rel (bust via http bind).
- Edge rule with multiple body atoms fires on ANY atom's arrival joining the
  others' current sets (semi-naive shape). Consequence: subscriber-joins
  replay backlog on connect. Sometimes wanted (SSE catch-up); must be known.
- Un-watch teardown: level consequences retract free; edge-derived history
  survives by design; real cleanup = scope teardown (switch_map/range-DELETE).

## Noted for later (user-flagged)

- TYPE->SQLITE LOWERING, 1-1: every type = a table; reference-typed column =
  surrogate int into the referenced type's table (auto junction where
  multiplicity demands); EVERY row gets a dense integer rowid/surrogate for
  dense graph-algorithm storage. Hash kosher-ness = interning: canonical term
  -> content hash -> surrogate via mapping table; equality = int compare;
  same term never stored twice. Same move as: node(hash) in the sub/node
  design, v5 storage-diet dense dictionary ids, support keys tag*stride+rowid.
- Quoted DSLs (sg/shell/sql...): each owes parse (DCG or SWI quasiquotation),
  check (against imported schema facts, e.g. node-types.json cons), lower
  (native driver query || unification reference semantics, babel two-path).
  Patterns are terms, so rules can derive patterns (codemod route).
- TypeSpec-replacement direction (user, 2026-07-27, "one day"): types-as-facts
  + envelope effects + bind already mirror TypeSpec's model/operation/protocol
  split; OpenAPI and JSON Schema become additional emitters over the same
  facts, each emit_ts.pl-sized. Far tier, no dependencies beyond T0 types.
- PRIOR ART, user's own (verified 2026-07-27): this is iteration THREE.
  ~/projects/hafley-tsp = TypeSpec source-of-truth app gen (ghcacher as 14
  entity .tsp files; React+Rust targets; config+CLI from one model via typed
  annotations). ~/projects/hafley-rxjs/packages/json-rx = rx-shaped TypeSpec
  -> checked portable Rx program -> TS(signals/rxjs) + Rust(tokio/streams);
  marble fixtures as the cross-target compatibility record. Both lacked the
  semantic core (time/relational/IVM/durability) that v6 supplies. INHERIT:
  (1) _auto/manual emitted-file split, never overwrite manual; (2) fixture
  corpus as the babel two-path agreement mechanism; (3) bind vocabulary must
  cover config/env/CLI sources with @secret redaction, not just
  shell/every/sse; (4) path-file concordance as a compile check; (5) inline
  operators get derived source locations, not reusable identities (matches
  this session's naming law independently); (6) the tsp ghcacher entity list
  (incl. call_log, poll_state) is the richer v6 ghcacher target spec.

## Kernel candidate if the shape survives boiling

{ground_terms, rule(level|edge), keyed_rel, world_rel}; register leaves;
pre/delta demoted to operators; sugar table rewrite pending.
