# AUDIT: adversarial read of the v6 surface language (2026-07-27)

Target: `v6/prolog/labs/LANG.md`. Supporting reads: `plans/2026-07-27-surface-boil.md`,
`plans/2026-07-27-mode-dominance.md`, `v6/prolog/ARCH.pl`, `v6/prolog/src/{kernel,checks}.pl`,
`v6/prolog/examples/ghcacher.pl`, and 13 v5 programs sampled from the 173-file corpus
(`.dl/*.dl` + `examples/**/*.dl` + `std/*.dl`).

No code was written. Every constructed program below is in the candidate syntax of
LANG.md and is presented as a counterexample, not a proposal.

Corpus feature census (173 files, regex-counted, script in the session scratchpad):

| feature | files | % |
|---|---|---|
| comparison / arithmetic in body or head | 166 | 95 |
| `scan(...)` file-set seeding | 139 | 80 |
| `? query` | 130 | 75 |
| negation `!rel(...)` | 86 | 49 |
| aggregate in head (`count`/`max`/`sum`/`collect`) | 76 | 43 |
| string interpolation (`{x}` or `${x}`) | 69+47 | 40 / 27 |
| `diag(...)` head | 55 | 31 |
| `comment(...)` extraction | 31 | 17 |
| `gen(...)` head (file generation) | 30 | 17 |
| `closure`/`scc`/`node2vec` | 29 | 16 |
| scalar fns (`strip_prefix`, `replace_re`, ...) | 28 | 16 |
| `json`/`jsonp` extraction | 21 | 12 |
| `ast`/`ast_yaml`/`sg` extraction | 18 | 10 |
| `use "..."` module import | 16 | 9 |
| `@async` effect | 12 | 6 |
| `@next` carry | 6 | 3 |

The candidate language, as written in LANG.md, has surface syntax for none of the top ten.

---

## 1. The self-check that is supposed to stop non-compositional features cannot see the current surface

**Severity: blocker**

Evidence. `ARCH.pl:9-11` claims `go` machine-checks that "every surface feature desugars,
transitively, into the 5-element kernel (no non-compositional core features can sneak in past
this check)". The check is `ARCH.pl:236`:

```prolog
check(sugar_grounds_out, ( forall(sugar(Feature, _), grounds(Feature)) )).
```

The quantifier ranges over `sugar/2`, not over the surface. A surface feature with no `sugar/2`
fact is not examined. Verified: `grep -nE "sugar\((key|edge_rule|level_rule|effect_arrow|bind|struct)"
v6/prolog/src/kernel.pl` returns nothing. `Key(Type)`, `<+`, the `->` effect split, `bind`, and
`struct` have zero registry entries.

Worse, the registry describes a dead surface. `kernel.pl:47-53`:

```prolog
surface_form(fact,     [ground_terms]).
surface_form(source,   [external_rel]).
surface_form(external, [external_rel, ground_terms]).
surface_form(register, [register, ground_terms]).
```

`LANG.md:15-16` says `source`, `fact`, `rule`, `external`, `register` all died. There is no
`surface_form` for `rel`, `struct`, or `bind`. `kernel.pl:15` still lists `register` as one of the
four kernel primitives while `LANG.md` replaces it with `Key(Type)` and demotes `pre` to a rare
operator (`LANG.md:36`). Ran `swipl -q -l v6/prolog/ARCH.pl -g go -g halt`: all four checks PASS
against a surface that no longer exists.

Resolution options:
- Invert the quantifier: enumerate `surface_form/2` for the LIVE keywords and require
  `forall(surface_form(F,_), grounds(F))` plus a `sugar/2` entry for every non-primitive.
- Make `surface_dcg` (ARCH.pl:205, unbuilt) the source of surface names, and have `go` fail on any
  parsed construct with no sugar chain.
- Accept that `go` is a lint over the registry only, and delete the ARCH.pl:9-11 claim.

---

## 2. The shipped conformance check forbids transitive closure

**Severity: blocker**

Evidence. `checks.pl:32-36`:

```prolog
no_self_union(Rules) :-
    \+ ( member(Head <- Body, Rules),
         functor(Head, Name, Arity),
         body_member(Ref, Body),
         functor(Ref, Name, Arity) ).
```

This is name/arity self-reference, which is the definition of a recursive datalog rule, not the
`change_log_next` carry twin it was aimed at. Verified by running it against
`.dl/rails.dl:84-88` transcribed to prolog terms: `no_self_union: FAILS on plain transitive
closure`.

The corpus depends on this: `.dl/rails.dl:84-88` (`loop_reachable_fn`),
`examples/db-seam-callgraph-audit.dl:51-53` (`reaches_sym`), `.dl/flow-panel.dl:49-51`
(`mod_reach`), `.dl/rusqlite-coupling.dl` (`raw_sql_reaches`), plus 40 other files carrying a
self-recursive rule head.

`ghcacher.pl:89-92` wires this check into the graded example, so the flagship v6 program passes
only because it contains no recursion at all, which is itself a warning sign about coverage.

Resolution options:
- Retarget the check at the actual smell: head rel H with a rule `H <- H, ...` whose only purpose
  is carry across ticks, i.e. the body references `pre(H)` or a `_next` twin, not plain recursion.
- Replace with a stratification check (recursion legal, recursion through negation illegal), which
  is the standard law and matches `ARCH.pl:133 algorithm(stratification, ...)`.
- Delete `no_self_union` and keep `no_twin_names` only.

---

## 3. `->` is Key on the demand columns, except exactly where it is not

**Severity: design-gap**

Evidence. `LANG.md:18-21` (Key = at most one row per key, new derivation replaces) and
`LANG.md:22-27` (`rel fetch(endpoint: Url, prev: Tag) -> FetchResult;` with "demand rows =
requests, content-addressed dedup"). Content-addressed dedup on `(endpoint, prev)` IS a uniqueness
constraint on `(endpoint, prev)`. `mode-dominance.md:19` closes the loop: `fetch` is `det`, exactly
one envelope per request. For a det effect, `->` and `Key` on the left columns are the same
statement made twice.

Three places they come apart, and all three are unstated:

- **Multiplicity.** `LANG.md:72` lists "streaming effects (many response rows per demand row) vs
  det-per-request" as open. A streaming effect's left columns are not a key. So `->` cannot be
  defined as Key, and there is no second marker for the streaming case.
- **Write discipline.** Key means replace with `-old/+new` (`LANG.md:20-21`). `->` plus
  `LANG.md:62` ("fresh response replaces (edge)") means the fill arrives as an edge. Replace-by-key
  and append-by-edge are different disciplines on the same rel; the spec applies both words to the
  same row.
- **Direction.** `->` also carries the adornment (which columns are input to the demand transform).
  `Key(Str, 1)` / `Key(Int, 2)` (`surface-boil.md:28-29`) carries an ORDER but no direction, so
  `rel edge(a: Key(Str,1), b: Key(Int,2))` says nothing about mode.

Constructed case where the two disagree on the same rel:

```
rel fetch(endpoint: Url, prev: Tag) -> FetchResult;
rel cache(input: Key(Url), entry: Entry);
cache(endpoint, Entry{tag, body}) <+ fetch(endpoint, prev) -> Fresh{tag, body};
```

`fetch` is keyed on `(endpoint, prev)`; `cache` is keyed on `endpoint` alone. Two in-flight `prev`
values for one endpoint (the first poll after a restart plus a retry) produce two `fetch` rows and
two candidate `cache` values in one tick, from ONE rule. The "jointly semidet per key per tick" law
(`LANG.md:57`) is stated over rules, so it does not even quantify over this case. See finding 12.

Resolution options:
- Define `->` as pure sugar: `rel f(a: Key(T,1), b: Key(U,2), out: R)` plus a mode adornment, and
  give streaming effects a distinct envelope type (`Stream(R)`), not a distinct arrow.
- Keep both but state the law: `->` asserts a functional dependency left-to-right AND the demand
  split; `Key` asserts uniqueness with no direction; a det effect is the case where they coincide.
- Kill `Key(Type)` on effect rels specifically (redundant there), keep it on state rels.

---

## 4. `<+` into a keyed rel contradicts the definition of `<+` in the same paragraph

**Severity: blocker**

Evidence. `LANG.md:31-33`: "`head <+ body;` EDGE: fires on body-atom ARRIVALS this tick, appends;
consequences never retract (occurrences cannot un-happen)." `LANG.md:20-21`: a keyed rel "holds at
most one row per key; new derivation REPLACES (emits -old/+new)."

A keyed replace emits `-old`. `-old` retracts the consequences of the old row. So `<+` into a keyed
rel does the one thing `<+` is defined not to do, and `LANG.md:62` prescribes exactly that
combination ("fresh response replaces (edge)").

Constructed repro:

```
rel cache(input: Key(Url), entry: Entry);
rel stars(endpoint: Url, count: Int);
cache(endpoint, Entry{tag, body}) <+ fetch(endpoint, prev) -> Fresh{tag, body};
stars(endpoint, n) <- cache(endpoint, Entry{body, ..}), jsonp(body, "stargazers_count", n);
```

Tick 5: a `Fresh` with a new body arrives. `cache` replaces, so `-old` fires, so `stars` retracts
the old count and asserts the new one. A reader of `stars` sees a retraction that the `<+` arrow
promised could not happen. This is v5's `resp_current -> stars` chain (`examples/gh-cache.dl:99-113`),
and it is the behavior you want; the arrow's stated law is what is wrong.

Second problem, same arrow: the write discipline is decided by the HEAD REL DECLARATION, possibly
in another file, not by the arrow. Reading `x <+ y;` at a call site tells you nothing about whether
x grows or is overwritten. One character of syntax (`-` vs `+`) times one non-local property
(keyed or not) gives four behaviors.

Resolution options:
- Forbid `<+` into a keyed rel; keyed rels are written only by `<-` and the key does the
  latest-wins. `<+` then means append, always, with no exception.
- Keep the combination but restate `<+` honestly: "fires on arrivals; whether the head retains or
  replaces is the head rel's key discipline", and drop the "occurrences cannot un-happen" line.
- Split into three arrows (`<-` level, `<+` append, `<=` keyed-replace-on-arrival) so the write
  discipline is local to the rule text.

---

## 5. Mixed heads under count-IVM silently mask level retractions

**Severity: design-gap**

Evidence. `LANG.md:59` and `ARCH.pl:66-71`: mixed heads are sound under count-IVM because a row
carries per-origin support, superseding v5's one-rel-one-rule-kind law. That claim addresses the v5
hazard (`rebuild_derived`'s DELETE-all wiping source rows; see `std/entry.dl:30-34`, which splits
`entry_pkg` from `entry_drv` for exactly this reason). It does not address a second hazard the mix
creates.

Constructed repro:

```
rel hit(path: Path, line: Int);
hit(path, line) <- current_scan(path, line);
hit(path, line) <+ scan_arrival(path, line);
```

Tick 1: `scan_arrival("a.rs", 7)` and `current_scan("a.rs", 7)` both hold. `hit("a.rs",7)` has
support count 2 (one edge origin, one level origin). Tick 2: the file is edited, `current_scan`
loses the row, the level origin subtracts. Support is now 1 from the edge origin, which never
retracts. The row stays. A rule that reads `hit` as a level fact now reads a stale row forever, and
nothing in the program text says so.

The v5 corpus has 55 files heading `diag(...)` (`.dl/no-new-eprintln.dl:73-94` heads it from three
separate rules). If any one of them were an edge rule, the rail would be a permanent ratchet in the
wrong direction: a fixed lint would never stop reporting.

Resolution options:
- Ban mixed level/edge heads on one rel (retain the v5 law's shape, drop its stated reason).
- Allow the mix but type the rel: `hit` becomes `Set` or `Log`, never both, and a `<+` into a `Set`
  is a type error.
- Allow the mix and require the checker to prove the two rules derive disjoint tuple sets, which is
  the same undecidable obligation as finding 12.

---

## 6. Killing `delta()` removed the only way to say "no backlog replay"

**Severity: blocker**

Evidence. `LANG.md:34-35`: "Edge rules with several atoms fire on ANY atom's arrival joined against
others' current sets (semi-naive shape) - known consequence: late-subscriber backlog replay."
`surface-boil.md:11`: "`delta()` wrapper dead."

`ghcacher.pl:79-80`, the kernel-4 example this language is graded against, is:

```prolog
rule(sse(Client, row(Ep, Kind, Val)) <-
       (subscriber(Client), delta(change_log(Ep, Kind, Val)))).
```

The `delta()` wrapper is what pins "only rows arriving now" to ONE atom while `subscriber` is read
as a current set. Without it, the candidate language can only write:

```
rel sse(client: ClientId, row: Row);
sse(client, Row{endpoint, kind, value}) <+ subscriber(client), change_log(endpoint, kind, value);
```

which fires on arrivals of EITHER atom. A subscriber connecting at tick 900 receives every
`change_log` row ever written. `surface-boil.md:49-51` acknowledges this and says "sometimes wanted
(SSE catch-up); must be known", but there is no syntax to choose. Per-rule, not per-language, is
the granularity that is needed, and `delta()` was that syntax.

Same shape bites the lint corpus. `.dl/rails.dl:101-105` joins four atoms
(`conn_fn`, `call_kind`, `loop_reachable_fn`, `changed_line`). Written as an edge rule, an arrival
on `changed_line` replays the entire `loop_reachable_fn` set, which on this repo is the transitive
call graph. It happens to be a level rule, which is the point: 55 of 55 diag-heading files are
level-only. Edge is the exotic case, and it is the case that lost its qualifier.

Resolution options:
- Restore a per-atom arrival marker under a different name (`new change_log(...)`, `+change_log(...)`)
  so the join side is chosen in the rule.
- Define `<+` as "fires on arrivals of the FIRST body atom only, others read as current sets", so
  atom order encodes the choice. Cheap, positional, and matches how the ghcacher rule reads.
- Keep the any-atom rule and add an opt-out annotation on the rel (`rel change_log(...) no_replay;`),
  which is coarser and cannot express per-consumer catch-up.

---

## 7. Within a tick, `pre(x)` and a plain read of a keyed `x` cannot both be defined

**Severity: blocker**

Evidence. `LANG.md:34`: "A body is one time cut (all atoms at the same instant)."
`ARCH.pl:62-64`: "Reading the row before the tick's fold IS the previous value; the batched UPSERT
at tick commit is both the update and the downstream delta."

If the upsert lands at commit, then during the tick every read of a keyed rel returns the old row,
so `pre(x)` and `x` are the same value and `pre` is useless. If a plain read returns the post-fold
value, then `pre(x)` and `x` disagree inside one body, so the body spans two instants and
`LANG.md:34` is false.

Constructed repro that is unwritable either way:

```
rel cache(input: Key(Url), entry: Entry);
rel churn(endpoint: Url, old_tag: Tag, new_tag: Tag);
cache(endpoint, Entry{tag, body}) <+ fetch(endpoint, prev) -> Fresh{tag, body};
churn(endpoint, old_tag, new_tag) <-
    pre(cache(endpoint, Entry{tag: old_tag, ..})),
    cache(endpoint, Entry{tag: new_tag, ..});
```

Under commit-time writes, `new_tag` binds to `old_tag` and `churn` is always empty (or always
degenerate). Under fold-time writes, the rule works but `LANG.md:34` is broken and every level rule
in the program now has to be told which side of the fold it evaluates on.

v5 avoided this by making the carry explicit and one tick wide: `examples/gh-cache.dl:103-104` has
`etag_next(ep, tag) <- resp_current(ep, tag, _)` and `etag(ep, tag) <- @next etag_next(ep, tag)`.
The twin was ugly and the language is right to want it gone, but the twin was also the tie-break
rule, and nothing replaced it.

Missing rules to state:
- Visibility of a keyed rel's write within the same tick (pre-fold, post-fold, or forbidden to read
  the same keyed rel a rule writes).
- Whether that visibility differs between `<-` and `<+` bodies.
- Whether `pre` on a NON-keyed rel means "last tick's membership" (a set diff) or is undefined.

Resolution options:
- Post-fold reads are illegal for a rel the same stratum writes; keyed rels get a stratum boundary,
  making `pre` = pre-stratum and the tick still one cut per stratum.
- Keep one cut, define all reads as pre-fold, and give the language an explicit `next` head marker
  for the write side (v5's `@next`, renamed).
- Forbid `pre` entirely and require the old value to be read from the effect envelope's own
  arguments, which is what `arm(next(unchanged), pre(S), S)` (`ghcacher.pl:56`) is really doing.

---

## 8. Same-tick retraction against same-tick arrival has no stated order

**Severity: design-gap**

Evidence. `LANG.md:30-33` defines level as "maintained view over current membership" and edge as
"fires on body-atom arrivals this tick". Neither says what "current" means when the tick's delta
set contains both a retraction and an arrival that a single rule reads.

Constructed repro (the un-watch case that `surface-boil.md:52-53` gestures at):

```
rel watch(endpoint: Url);
rel hit(endpoint: Url, line: Int);
rel history(endpoint: Url, line: Int);
hit(endpoint, line) <- watch(endpoint), scanned(endpoint, line);
history(endpoint, line) <+ hit(endpoint, line);
```

Tick T: `-watch("a")` and `+scanned("a", 7)` arrive together. Under a post-state reading, `watch("a")`
is gone, `hit` is never derived, `history` gains nothing. Under a delta-stream reading, `+scanned`
joins against `watch`'s pre-tick set, `hit("a",7)` is derived, `history` gains a permanent row, and
then `hit` retracts and `history` keeps it. The two readings differ by a row that can never be
removed.

`surface-boil.md:52-53` says "edge-derived history survives by design" but does not say whether it
should have been derived. This is the difference between "un-watch is clean" and "un-watch leaks a
row per racing arrival".

Resolution options:
- Declare the tick input a SET of deltas applied atomically; all rules evaluate against the
  post-state; arrivals are computed as post-state minus pre-state. Simple, and kills the race.
- Declare it a delta stream evaluated against pre-state, and add a rule that an edge rule's body
  atoms must all be present in the post-state for the append to commit.
- Stratify: retractions apply before arrivals within a tick, stated as a law.

---

## 9. The cadence bucket has no home, so the rate cap is gone

**Severity: blocker**

Evidence. `LANG.md:23`: `rel fetch(endpoint: Url, prev: Tag) -> FetchResult;`. That is a
two-column request key. `examples/gh-cache.dl:46-66` explains at length that a two-column key is
exactly the bug: an already-fired content-addressed request id is never re-fired, so a poll whose
args never change fires once and goes silent. v5's fix is a third column,
`poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b)` (`gh-cache.dl:64-66`), where `b` is
`now/300`. `surface-boil.md:39-40` restates the problem as "edge-derived demand rows must salt with
arrival tick".

Salting with the ARRIVAL TICK is wrong and reintroduces the failure v5 documents. `LANG.md:49` says
the global tick T advances every tick; `examples/gh-cache.dl:26-29` notes `DL_POLL_SECS` can be 5
seconds. Salting with T mints a new request id every 5 seconds and hits GitHub 720 times an hour
against a 5000/hour limit, for one endpoint. The salt must be a QUANTIZED WALL CLOCK, which is a
third time coordinate, and `LANG.md:48` says there are two.

`LANG.md:49` offers "observed via clock rels" as the escape, but no clock rel appears in any example
and `ghcacher.pl:37` declares `source(every_300, [bucket])`, a source form that `LANG.md:15` killed.

Missing: the surface form for a quantized clock, the type of its bucket column, and the rule that a
demand row's key includes it. Also missing: what happens on a bucket flip that produces no new
response (v5's `resp` accumulates one row per bucket; a keyed `fetch` would collapse them and lose
the response history that `gh-cache.dl:89-90` deliberately keeps).

Resolution options:
- Make the clock a rel with a Key on the bucket (`rel tick_300(bucket: Key(Int));`) and require
  effect demand keys to include a clock column when the effect is periodic; the checker can enforce
  "an effect whose demand rel has no clock column fires at most once per distinct arg tuple, ever"
  as a warning.
- Admit three time coordinates in `LANG.md:48` and give wall time a type (`Instant`, `Bucket(300)`).
- Push cadence into the `bind` (`bind fetch = shell { ... } every 300s;`), which keeps program text
  transport-free per `LANG.md:42` but moves the rate cap out of the checkable program.

---

## 10. Append-only rels have no retention bound, and the retention analysis bounds the wrong thing

**Severity: blocker**

Evidence. Mission (`LANG.md:3-5`) asks for "highly efficient RAM usage". `LANG.md:32`: edge-rule
consequences "never retract". There is no retention syntax anywhere in LANG.md.

`ARCH.pl:165` is the only bound: `technique(memory_bound, retention_depth, 'max pre depth = ticks kept')`,
and `ARCH.pl:64` defines it as the depth of `pre` reads on the hist table. That bounds the PREVIOUS-VALUE
history, not the append-only rels, which are the unbounded ones. `change_log` in `examples/gh-cache.dl:132-137`
is read by nobody's `pre`, so its retention depth is 0, yet it must keep every row because level
rules read its current membership.

Constructed program with no bound expressible:

```
rel change_log(endpoint: Url, kind: Str, value: Str);
change_log(endpoint, "stars", n) <+ stars(endpoint, n);
change_log(endpoint, "full_name", v) <+ full_name(endpoint, v);
```

One watched repo, one star change per hour, one year: 8760 rows, fine. `.dl/flow-panel.dl` scale is
the counterexample that matters: the v5 root db reached 877MB with `rel_port_of_reach` alone at
291k rows (CLAUDE.md storage-diet notes). An edge-fed equivalent grows monotonically with no
declared ceiling and no eviction rule.

Also missing: the interaction with teardown. `LANG.md:64-65` says "scope exit = range-DELETE of the
path prefix", which removes DEMAND rows. `LANG.md:50-51` says derived rows are shared across
subscribers, so they are not under any one path. Count-IVM reclaims level-derived rows when support
drops to zero; `surface-boil.md:52-53` states edge-derived rows survive on purpose. So teardown
reclaims exactly the rels that were already bounded and reclaims nothing from the rels that grow.

Resolution options:
- Per-rel retention in the declaration: `rel change_log(...) keep 30d;` or `keep 100_000;`, with the
  bound part of the type so the checker can refuse an unbounded edge rel that no query windows.
- Require every edge rel to carry a Key on a bucket column, making it a sliding window by
  construction (changes semantics: collapses within-bucket duplicates).
- Declare edge rels non-resident by law (sqlite only, never joined against as a full set), which
  also fixes finding 11 but forbids the `sse` backlog join outright.

---

## 11. Backlog replay is declared a feature and declared illegal

**Severity: design-gap**

Evidence. `LANG.md:35` calls late-subscriber backlog replay a "known consequence". `ARCH.pl:36-39`:
"ONLY DELTAS CROSS THE COASTLINE. The sqlite/rx boundary and the disk/memory boundary coincide. A
stage that would drag a table into JS heap is illegal, not slow."

Backlog replay on a subscriber arrival is the join of one new row against the full current set of
`change_log`. Under the SSE shape (`ghcacher.pl:79-80`) the result crosses into rx, one emission per
backlog row. That is dragging a table across the coastline, by the definition ARCH.pl gives.

The two documents can both be true only if the replay stays in sqlite and is paged, which is a
statement about the SSE consumer, not about the rule. Nothing says which.

Resolution options:
- State that an edge rule's non-arriving sides are evaluated in sqlite and the result is streamed
  with a cursor, never materialized; add a row-count cap that fails the tick.
- Forbid an edge rule whose non-arriving atom is an unbounded rel (checkable once finding 10's
  retention bounds exist).
- Keep replay only for explicitly windowed reads (`change_log[last 100]`), which is new syntax.

---

## 12. "Jointly semidet per key per tick" is not decidable as stated

**Severity: design-gap**

Evidence. `LANG.md:56-58`: "rules heading a keyed rel must be jointly semidet per key per tick
(checker discharges; yield points separate seeds from transitions)."

Discharging it requires proving, for rules R1 and R2 heading keyed rel K, that no database makes
both bodies produce different values for one key. Over conjunctive queries with negation against an
unknown EDB, that is satisfiability of a first-order formula, not decidable in general. It is
decidable only for the syntactic special case where the rules are guarded by disjoint constructors
of one enum, which is exactly the shape `ghcacher.pl:53-58` uses and nothing else.

Two cases the quantification misses entirely:

- **One rule, many rows.** Finding 3's example: a single rule `cache(endpoint, Entry{...}) <+
  fetch(endpoint, prev) -> Fresh{...}` produces two rows for key `endpoint` when two `prev` values
  are in flight. "Jointly" across rules never looks at this.
- **World-sourced multiplicity.** The effect rel's fill count is decided by the bind, not the
  program (`LANG.md:40-42`, binds live in a separate file). A static check over program text cannot
  see whether a bound protocol returns one row or many. `LANG.md:72` lists streaming effects as
  open, which is the same gap.

Resolution options:
- Restate the obligation over the JOIN OUTPUT, not over rule pairs: "the multiset of derivations for
  key k in tick T has cardinality at most 1", and discharge it only by (a) the key of the body
  functionally determining the head key, or (b) an explicit reducer.
- Require an explicit tie-break on every keyed rel (`rel cache(input: Key(Url), entry: Entry) by
  max(tag);`), which makes the runtime total and the check unnecessary. v5 does this by hand with
  `resp_latest(ep, max(b))` (`examples/gh-cache.dl:99-102`).
- Make it a runtime error with a named row, not a compile-time claim.

---

## 13. The lifetime lattice is not a total order and the join rule is unstated

**Severity: design-gap**

Evidence. `mode-dominance.md:25` asserts `finite < until(Signal) < never`, a 3-point total order.
`until(S1)` and `until(S2)` are incomparable without deciding which signal fires first, which is a
runtime question. The `min` in `mode-dominance.md:41` is therefore partial.

Second gap: `mode-dominance.md:37` gives derived rules the lifetime "join of body inputs" and never
defines the operator. For a level rule the output can keep producing as long as any input keeps
producing against persisting rows, so it is a MAX, not the MIN used for scope nesting. The doc uses
one word for two lattice operations pointing opposite ways.

Constructed case:

```
rel timer(bucket: Int);
rel config(name: Str);
rel job(name: Str, bucket: Int);
job(name, bucket) <- config(name), timer(bucket);
```

`config` is finite (facts), `timer` is never. `min` gives `job: finite`, which is false: `job` gains
a row on every timer bucket forever. `max` gives `never`, correct here. But
`pair(a) <- timer(a), pre(timer(a))` under `max` is also `never`, which is right, while
`snapshot(a) <- timer(a), once(a)` would need finite. No rule distinguishes these.

Third gap: `mode-dominance.md:68` claims `subscribe change_log` is "provably" never because the
chain bottoms at `every`. `every` is a BIND (`LANG.md:40-42`), attached per deployment. The proof
therefore only holds post-link, and nothing says mode analysis runs after linking. Relinking with
`bind clock = shell { ... }` changes the answer.

Resolution options:
- Make lifetime a two-element lattice for checking (`terminates` / `does not terminate`) and carry
  `until(S)` as an annotation, not an order element.
- Define two operators explicitly: `scope_min` for switch_map nesting, `join_max` for rule bodies,
  and give `until(S1) join until(S2)` the value `until(S1 and S2)` (both must fire), which is at
  least well defined.
- State that mode analysis is a post-link pass and that a program has no modes until bound.

---

## 14. Envelope exhaustiveness has two opposite polarities in two files

**Severity: design-gap**

Evidence. `checks.pl:17-21` `covers_enum/2` requires EVERY enum constructor to have an arm, a hard
failure. `surface-boil.md:16-17` says "Exhaustiveness inverts to a lint (no rule consumes Error(_) -
deliberate?)", a soft warning with keep-as-default.

Under the inversion, `ghcacher.pl:56-57` loses its two do-nothing arms:

```prolog
, arm(next(unchanged),         pre(S), S)
, arm(next(error(_)),          pre(S), S)
```

and then `covers_enum` fails on the graded example. The example currently passes only because it
was written under the old polarity.

Decidability: the inverted form IS decidable (a syntactic fold over patterns per enum,
`enum_match.pl`), but it has no failure condition, so it is a report rather than a check. The
strong form is also decidable but forces the boilerplate the inversion was meant to delete.

Separate wart in the same predicate: `checks.pl:19-21` matches arms by functor NAME only, so an arm
pattern `fresh(X)` against enum constructor `fresh(tag, body)` passes with the wrong arity.

Resolution options:
- Pick the inversion, delete `covers_enum`, and replace it with a report rel the program can query
  (`? unhandled_arm(enum, ctor)`), so "deliberate?" is answered by a rule, not by the compiler.
- Keep the strong form only for arms that CHANGE the value, and let keep-arms be implicit, which is
  the inversion with the check retained.
- Keep both, gated by an opt-in on the enum (`enum FetchResult exhaustive { ... }`).

---

## 15. Effects are read-shaped; the corpus has write effects that need an apply gate

**Severity: blocker**

Evidence. `LANG.md:22-27` models an effect as "demand rows = requests, world fills responses", with
an envelope enum carrying the result. `examples/gh-checkout.dl:50` is the counterexample:

```
checkout(slug, "", "0") <- repo(slug, root, url).
```

`checkout` clones, fetches, and fast-forwards real git checkouts. Its header
(`gh-checkout.dl:23-29`) documents three gates the candidate language cannot express: the sink does
NOT fire from a `?` query, it needs `--apply` or `DL_APPLY_SINKS=1` one-shot, and
`DL_CHECKOUT_DRY_RUN=1` diverts rows to `checkout_plan` instead of `checkout_done`.

`examples/gen-reference.dl:24-42` is the same shape at 30 files in the corpus: `gen(:append, path,
line)` WRITES files, in program order, converging (skip the write when bytes match). Nothing in
LANG.md describes a write effect, an apply gate, a dry-run mode, or ordered output accumulation.

The envelope model also does not fit: a write effect's "response" is an outcome
(`checkout_done(repo, branch, action, ok, detail)`, `gh-checkout.dl:57`), and its demand row must
not be content-address-deduped, because running the same fast-forward next hour is the entire point.
Content-addressed dedup (`LANG.md:24`) makes an idempotent-by-intent write fire exactly once, ever.

Resolution options:
- Add a second effect direction with its own arrow and gate: `rel checkout(slug: Str, branch: Str)
  <~ CheckoutResult;` where `<~` means write-effect, never deduped, gated by a link-time or CLI
  apply flag.
- Model writes as ordinary effects whose demand key includes a clock bucket (finding 9's mechanism),
  and put the apply gate in the `bind`.
- Declare write effects out of scope for v6 and accept losing `gen` (30 files) and `checkout`.

---

## 16. Expressiveness against 13 sampled v5 programs

"with-sugar" means the candidate language could express it after adding a construct that
`surface-boil.md` already sketches. "NO" means no sketch exists anywhere in the three spec files.

| v5 program | what it uses | expressible? | missing construct |
|---|---|---|---|
| `.dl/rails.dl` (134 L) | `scan` + `match_line` regex capture, `ast_yaml` with `not: inside:`, `count(l)` grouped by path, `n > 10`, recursive `loop_reachable_fn`, `!`-free but `p != fs:\`src/db.rs\``, `diag` with col/end_col spans, `${w}` interpolation | **NO** | extraction ops, aggregates, comparison, path literals, `diag` sink, interpolation. Recursion expressible in kernel, rejected by `checks.pl:32-36` |
| `.dl/no-new-eprintln.dl` (103 L) | `scan`+`match_line`, `comment(...)` op, range join `waiver_line >= line_number - 1`, `count`, negation `!eprintln_waived`, `!eprintln_baseline`, baseline FACTS as a ratchet table, `diag` x2, `diag_stage` routing, `? diag(...)` | **NO** | extraction, arithmetic in a comparison, negation surface, `diag`/`diag_stage` sinks, `?`. Facts alone survive (`LANG.md:39`) |
| `.dl/flow-panel.dl` (675 L) | multi-repo scan fan `scan("*","WORK",...)`, builtin rels `type_entity`/`type_link`/`call_edge`/`df_*`, recursive `mod_reach`, negation `!module_cycle`, `use "../std/entry.dl"` | **NO** | extraction with repo fan, builtin-rel catalog, negation, module import |
| `.dl/git-graph.dl` (147 L) | `sh` effect with a shell template and `{cutoff}` interpolation, `@async` gate, `json(body, q:[...])` array destructuring, `collect(slug, 100)` batching aggregate INSIDE a demand arg, `graph_node`/`graph_edge` sinks, `"${repo}#PR-${num}"` id construction | **NO** | effect binds are the one part covered (`LANG.md:40-42`); everything else missing. `collect(slug,100)` is the hardest: a demand row whose key is a SET, unaddressed by `Key(Type)` |
| `examples/gh-cache.dl` (141 L) | `sh` effect, `clock(300, b)` bucket salt, `@next` carry x2, `max(b)` latest-wins, `jsonp`/`json`, self-union `change_log`, `? change_log(...)` | **with-sugar (partly)** | the effect + envelope + Key(cache) part is the design's own worked example. Missing: the 300s bucket (finding 9), `max(b)` reduction (finding 12), `jsonp` extraction, `?` |
| `examples/gh-checkout.dl` (57 L) | `checkout` write sink, `repo` builtin config rel, apply gate + dry-run, `? checkout_done(...)` | **NO** | write effects (finding 15), config-sourced builtin rels, apply gating |
| `examples/callgraph-resolved.dl` (34 L) | `ast` tree-sitter query capture with named captures and span binding, range join `s <= l, l <= e`, `closure(calls)` builtin, `!calls(_, name)` | **NO** | extraction, comparison, `closure` builtin, negation |
| `examples/db-seam-callgraph-audit.dl` (132 L) | `call_def`/`call_edge`/`call_site`/`nest`/`df_node`/`module_binding` builtin rels, regex match `=~ /\.rebuild_derived$/`, recursive `reaches_sym`, `count(seam_sym)`, `!storage_dir_file`, `?` x4 | **NO** | builtin rel catalog, regex-as-comparison, aggregates, negation, `?` |
| `examples/graph-measure.dl` (92 L) | `scc(edge)` builtin, `count(member)`, `count(line)`, self-join with `a < b`, arithmetic head expressions `ua + ub - sh` and `sh * 100 / u`, filter `pct >= 40` | **NO** | graph builtins, aggregates, arithmetic in heads, ordering comparison |
| `examples/node2vec-callgraph.dl` (13 L) | `node2vec(g)` builtin returning `(a, b, score)` | **NO** | a builtin whose OUTPUT ARITY differs from its input rel; the candidate language has no form for a body atom that binds head vars from a graph algorithm |
| `examples/arch-conformance.dl` (51 L) | architecture as facts, `strip_prefix(f, p)` scalar fn bound with `=`, `s != f` test, `!allowed(ta, tb)`, `count(a)`, `diag` with interpolation | **with-sugar** | facts and the negation-free skeleton survive; scalar fns, `=`-binding, negation, aggregates, `diag` all missing. The closest program to expressible in the corpus |
| `examples/gen-reference.dl` (100+ L) | `gen(:append, path, row)` ordered file writes, `true()` singleton, self-describing catalogs `rel_catalog`/`fn_catalog`/`op_catalog`, `{name}` interpolation | **NO** | write effects, program-order output accumulation, the `true()` unit rel, catalogs |
| `std/entry.dl` (60+ L) | `use "std/flow.dl"` import, source/derived split unioned into one rel, name-keyed bridge, depth cap 64 on recursion | **NO** | module import, depth-capped recursion. The source/derived split it performs is the v5 law `ARCH.pl:66-71` declares obsolete, so this file gets SIMPLER under the candidate, the one place it wins |

Score: 11 NO, 2 with-sugar, 0 yes, over 13 programs. The single largest gap is extraction (139 of
173 corpus files call `scan`), for which the only text anywhere is `surface-boil.md:64-67`, a
"noted for later" paragraph saying quoted DSLs owe a parse, a check, and a lowering. There is no
candidate syntax for reading a file.

---

## 17. Extraction has no surface form at all

**Severity: blocker** (broken out from finding 16 because it is 80% of the corpus)

Evidence. `LANG.md` mentions no file, no path, no glob, no regex, no AST query. `surface-boil.md:64-67`
is the entire treatment: "Quoted DSLs (sg/shell/sql...): each owes parse (DCG or SWI
quasiquotation), check (against imported schema facts, e.g. node-types.json cons), lower".

The corpus shape is a source rule whose body mixes a scan with an extraction op and whose head binds
only from the op's captures (`.dl/no-new-eprintln.dl:22-32`, with the S6 sharp edge spelled out in
its comment at lines 29-30: a source-op body cannot join other rels). Three questions the candidate
language must answer and does not:

1. Is `scan("WORK", "src/**/*.rs", path, rev)` an effect (demand rows, world fills) or a rel? If an
   effect, its demand key is a glob and its fill is thousands of rows, which is the streaming case
   `LANG.md:72` leaves open.
2. Is `ast_yaml(path, rev, :rust, \`...\`, l, c, _, ec)` a bound effect (`bind ast = ...`) or a
   quoted DSL checked against an imported grammar? `surface-boil.md:65-66` wants the latter, which
   means the type checker needs `node-types.json` as input, i.e. a build-time dependency the
   language has not declared.
3. What is the RE-EXTRACTION trigger? v5 keys on `rev`; a keyed rel `rel file(path: Key(Path), rev:
   Rev, text: Str)` would replace on every edit and cascade retractions through every derived rel,
   which is finding 4 at repo scale.

Resolution options:
- Extraction is a bound effect whose demand key is `(glob, rev)` and whose envelope is
  `Files { paths }`, with per-file content a second effect keyed on `(path, rev)`; quoted match DSLs
  are separate pure functions over the content.
- Extraction is a builtin rel family (v5's shape), declared in a catalog the checker imports, with
  no surface syntax beyond calling it. Loses the "quoted DSL is checked" ambition.
- Extraction stays in rust (`ARCH.pl:155 tech(rust, future_bundle, [extraction, ...])`) and the
  language only consumes its output rels, in which case LANG.md should say the language cannot read
  a file and the rel catalog is the boundary.

---

## 18. Smaller warts

**18a. `struct` is a keyword with no example.** `LANG.md:15` lists `struct` among the four keywords.
No declaration of a struct appears in LANG.md, `surface-boil.md`, or `ghcacher.pl`. `LANG.md:37`
uses `Entry { tag, .. }` as a pattern and `LANG.md:19` uses `Entry` as a column type, so a struct is
presumably a named product type. Whether a `rel` row IS a struct (the ARCH.pl:180-182 "symmetric
struct/tuple discipline" says yes) or a struct is a separate nominal type is unstated, and it
decides whether `rel cache(input: Key(Url), entry: Entry)` is one table or two.
*Severity: wart.* Fix: one worked struct declaration in LANG.md, plus a sentence on rel-row identity.

**18b. `no_twin_names` is a spelling lint.** `checks.pl:27-29` fails any rel whose name ends
`_next`. A rel legitimately named `build_next` (the next build) fails; a twin named
`etag_shadow` passes. *Severity: wart.* Fix: detect the SHAPE (a rel read only by a `pre`/carry rule
and written only by its twin), not the suffix.

**18c. `? query` has no home in the surface.** 130 of 173 corpus files end in `?` queries;
`mode-dominance.md:63-69` types them; `LANG.md`'s Surface section does not list them.
*Severity: wart.* Fix: add `?` to `LANG.md:13-42` with its mode annotation.

**18d. `in` fan-out is unsold and its fallback is unstated.** `LANG.md:38` and
`surface-boil.md:32-33`. The fallback ("one plain edge rule per field, v5 style") is not what v5
does: `examples/gh-cache.dl:120-126` uses ONE `json` brace pattern to fan an array into one row per
element with sibling fields correlated. That is fan-out over a parsed value, not over a literal
list, and neither `in` nor the fallback covers it. *Severity: design-gap.* Fix: decide whether
fan-out is a pattern feature (destructuring a list-typed column) or a body operator.

**18e. "the db IS the API" versus keyed replace.** `LANG.md:6-7` invites outside readers to read
sqlite directly. `LANG.md:20-21` makes keyed rels mutate in place. Nothing states whether external
readers get a WAL snapshot or can observe a half-applied tick. *Severity: wart.* Fix: state that
readers see committed ticks only, which the "tick = one transaction" law (`LANG.md:46`) already
implies but does not say.

**18f. Recursion has no termination story.** `std/entry.dl` caps its reachability recursion at depth
64 and documents that a longer real path silently does not appear. `ARCH.pl:85-91` says in-tick
recursion must terminate (datalog's guarantee) which is true for pure datalog but not for datalog
with arithmetic in heads (`examples/graph-measure.dl:66-71` computes `ua + ub - sh`). No surface
form for a depth cap exists. *Severity: design-gap.* Fix: either ban arithmetic in recursive strata
or add a declared bound.

---

## Keep / kill / merge over every LANG.md surface construct

| construct | LANG.md ref | verdict | reason |
|---|---|---|---|
| `enum` keyword | :15 | keep | envelope types are the design's best idea; `ghcacher.pl:28-32` earns it |
| `struct` keyword | :15 | keep, specify | 18a: no example, rel-row identity unstated |
| `rel` keyword | :15 | keep | one declaration form for state, effects, and events is a genuine simplification |
| `bind` keyword | :15 | keep | `ARCH.pl:73-78` banked; link-time protocols are sound and testable |
| required column types | :17 | keep | v5 already does this (`rel watch(ep: text)`) |
| `Key(Type)` | :18-21 | keep | replaces v5's `resp/resp_latest/resp_current` chain, a real win |
| `Key(Type, N)` compound order | :19-20 | keep | needed for `rel edge(a,b)`; cheap |
| `->` effect split | :22-23 | **merge into Key** | finding 3: it is Key-plus-adornment; streaming needs an envelope, not an arrow |
| envelope enum on effects | :25-27 | keep | makes fill det, kills the `status == 200` filter chain |
| `<-` level rule | :29-30 | keep | 100% of the sampled lint corpus is level-only |
| `<+` edge rule | :31-33 | **kill as specified, respecify** | finding 4: its stated law is false when the head is keyed; finding 6: it lost `delta()`'s qualifier |
| one-time-cut body | :34 | **kill or qualify** | finding 7: incompatible with `pre` under any write-visibility choice |
| any-atom edge firing | :34-35 | **kill** | finding 6 and 11: no per-rule control, and it violates ARCH.pl:38 |
| `pre(atom)` | :36 | keep, define | finding 7: needs a stated visibility rule; `ARCH.pl:62-64` and `LANG.md:34` disagree today |
| `x.field.sub` dot access | :36-37 | keep | `kernel.pl:38` has its sugar entry; cheapest construct in the spec |
| `Entry { tag, .. }` patterns | :37 | keep | needed by the envelope arms |
| `name in listexpr` | :38 | **kill for now** | 18d: unsold, and its stated fallback does not match what v5 does |
| facts = bodiless clauses | :39 | keep | `.dl/no-new-eprintln.dl:67-70` (the baseline ratchet) and `examples/arch-conformance.dl:23-29` (the architecture) are both fact tables |
| `bind X = shell { ... }` | :40-42 | keep | `.dl/git-graph.dl:88-110` proves shell templates carry real weight |
| `match status { ... }` in a bind | :41 | keep | the status-to-envelope mapping has to live somewhere non-program |
| exactly one bind per effect | :42 | keep | but see finding 13: it makes mode analysis post-link |
| tick = one sqlite txn | :46-47 | keep | |
| global tick T, erased | :48-49 | keep, extend | finding 9: a third coordinate (quantized wall clock) is required and missing |
| subscription-relative time | :50-51 | keep | |
| mode type (card, lifetime) | :52-55 | keep, respecify | finding 13: lattice is not total, join operator undefined |
| jointly-semidet-per-key law | :56-58 | **respecify** | finding 12: not decidable as quantified, and misses single-rule multiplicity |
| mixed heads under count-IVM | :59 | keep, restrict | finding 5: edge support masks level retraction |
| SWR cache semantics | :60-63 | keep | the one place the spec and v5 agree end to end |
| demand rows + range-DELETE teardown | :64-65 | keep, insufficient | finding 10: reclaims nothing from edge rels |
| aggregation | absent | **add** | 76 of 173 files |
| negation | absent | **add** | 86 of 173 files; also contradicts `ARCH.pl:167 technique(absence, ...)` which requires a reference clock |
| comparison / arithmetic | absent | **add** | 166 of 173 files |
| scalar functions | absent | **add** | 28 files |
| string interpolation | absent | **add** | 69 files |
| extraction ops | absent | **add** | 139 files; finding 17 |
| recursion | kernel only | **add to surface** | 44 files; and `checks.pl:32-36` currently rejects it |
| `? query` | absent from Surface | **add** | 130 files; 18c |
| `diag` / `gen` / `graph_node` / `checkout` sinks | absent | **add** | 55 / 30 / 2 / 1 files; finding 15 |
| module import (`use`) | absent | **add** | 16 files |
| retention declaration | absent | **add** | finding 10; the mission asks for tight RAM and nothing bounds anything |

---

## Resolve before any implementation, ranked

1. **Extraction (finding 17).** 80% of the corpus starts with `scan`. Until the language can name a
   file, every other decision is being made on a program shape the language cannot host. Decide:
   bound effect, builtin rel family, or out of scope with the rel catalog as the boundary.

2. **The `<+` / keyed-rel overload and its stated law (findings 4, 5, 6).** One arrow currently
   means append, or replace, or replace-while-claiming-not-to, decided by a non-local declaration,
   with the atom-arrival qualifier deleted. Three separate corrections, one construct. Nothing
   downstream is stable until the write disciplines are named and local.

3. **Within-tick visibility and the delta/post-state question (findings 7, 8).** "A body is one time
   cut" plus `pre` plus commit-time upserts cannot all hold. This decides whether a carry can be
   written at all, whether un-watch leaks, and whether stratification is required. It is the cheapest
   of the five to settle and it gates the mode analysis.

4. **Retention and the growth story (findings 10, 11).** The mission sentence says tight RAM; the
   spec has one unbounded construct and no bound syntax, and the only declared bound (`pre` depth)
   measures a different table. Needs a per-rel declaration before any storage lowering is designed.

5. **The cadence bucket (finding 9).** `examples/gh-cache.dl:46-66` is a written post-mortem of
   exactly this failure, the candidate spec's own worked example omits the column, and
   `surface-boil.md:39-40` proposes the wrong salt (arrival tick). Small fix, high blast radius: it
   is the difference between 12 and 720 API calls per hour on the flagship example.

Not in the top five but cheap and worth doing in the same pass: point `check(sugar_grounds_out)` at
the live surface (finding 1) and retarget `no_self_union` (finding 2), so the two files that claim
to check the design stop passing vacuously.
