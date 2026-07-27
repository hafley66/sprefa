# shell_stream lab: streaming shell effects

Lab: `v6/prolog/labs/shell_stream.pl` (20 checks, all PASS, 49ms including two
real `process_create` spawns).
Fixture: `v6/prolog/labs/shell_stream_fixture.jsonl` (3 JSONL lines).
Run: `swipl -q -l v6/prolog/labs/shell_stream.pl -g go -g halt`

Target: `sprefa-extract`, a process that emits one JSON object per line for as
long as it runs, then exits with a code. One demand row, many response rows,
one terminal event.

## 1. Verdict on the surface form

Chosen: candidate (a), envelope-with-terminal, with one change. The terminal
constructors move into their own enum, and the two enums are named together by
a wrapper in the arrow's result position.

```
enum ExtractEvent { Line { obj: Json } }
enum ExtractEnd   { Done { code: Int }, Err { msg: Str } }

rel extract(args: Str, salt: Digest) -> Stream(ExtractEvent, ExtractEnd);
```

`Stream(Item, End)` is the same move `Key(Type)` already makes in a column's
type position: a wrapper that carries a property the checker needs, expressed
as a type rather than as a keyword or a sigil. Two more facts complete the set:

| result position | cardinality | lifetime | meaning |
|---|---|---|---|
| `-> FetchResult` | det | finite | today's effects, unchanged |
| `-> Stream(Item, End)` | multi | finite | many items, terminal guaranteed |
| `-> Tail(Item)` | multi | never | no terminal type, so no completion |

`check(mode_read_off_result_type)` and `check(multi_finite_needs_terminal_enum)`
grade exactly this: the mode is a function of the result type alone, and a
result type is `finite` at multi cardinality if and only if it names a terminal
enum. Nothing else in the signature changes, and `-> T` for a plain enum `T`
stays legal as the det case.

### Why not (b), a distinct `->*` arrow

`->*` states multiplicity and nothing else. It does not say which constructor
ends the stream, and lifetime analysis needs that. So `->*` has to be paired
with a terminal marker somewhere anyway, at which point the marker carries all
the information and the arrow is redundant with it. A second arrow is also a
second grammar production and a second sugar chain to ground out, for zero
information the type system was not already able to hold.

### Why not (c), two linked rels

Three costs, all structural:

1. It breaks "exactly one bind per effect". A process is one OS resource that
   produces both the lines and the exit code. Two rels means either two binds
   aimed at one process, or one bind heading two rels. Neither is expressible
   in the bind syntax as LANG states it.
2. The two rels have to agree on request identity. The completion rel's key
   must be derived from the line rel's demand row, and the surface has no way
   to say "these two rels share a request". Under one rel this is free: it is
   the same interned demand term (`check(identical_demand_dedups)`).
3. Joint liveness becomes unstated. A checker cannot see that the line rel
   stops producing when the completion rel fires. With one rel that is a single
   status row (`check(terminal_is_terminal)`).

### Why the terminal constructors split into their own enum

Candidate (a) as posed puts `Line`, `Done` and `Err` in one enum. Splitting
them buys two things and costs nothing:

- Exhaustiveness gets sharper instead of noisier. Under the flat enum, every
  match site over items must also carry `Done` and `Err` arms that can never
  fire at that point. `check(flat_envelope_costs_dead_arms)` shows the item arm
  set failing to cover `extract_event_flat` while covering `extract_event`.
- The two consumers genuinely have different rule shapes. Per-item is an
  append rule over a row stream; per-terminal is a once-per-request rule that
  also reads an aggregate over the items already landed
  (`extraction_complete(id, count, code)` in the lab). Two types, two bodies.

`check(missing_terminal_arm_rejected)` confirms the existing exhaustiveness
fold still bites: a program that drops the `Err` arm fails to cover
`extract_end`. The check is the same fold from `books/v6/enum_match.pl`, run
twice instead of once.

### The bind keeps the transport and the exit-code match

```
bind extract = shell {
  `sprefa-extract {args}`
  -> stdout_line(text) => Line { obj: json(text) }
   , exit(code)        => match code { 0 => Done { code }, c => Err { msg } }
};
```

The exit-code-to-constructor match is the same shape ghcacher already uses for
HTTP status (`match status { 200 => Fresh, 304 => Unchanged, s => Error }`).
It stays in the bind because "nonzero exit means failure" is a property of
running processes, not of the program's model. The lab encodes it as
`bind_exit/2` and `check(live_nonzero_exit_keeps_rows)` drives it from a real
`/bin/sh` that prints two lines and exits 3.

## 2. What streaming does to the mode table

`plans/2026-07-27-mode-dominance.md` has this row:

```
| `external = shell {...}` | finite per request (det: 1 next + complete) |
```

That row is wrong for any process that writes more than one line, which is
most of them. It splits in three:

| upstream | cardinality | lifetime |
|---|---|---|
| `shell` bound to a det envelope (one exit status, no stdout rows) | det | finite |
| `shell` bound to `Stream(Item, End)` | multi | finite |
| `shell` bound to `Tail(Item)` (`tail -f`, a watcher) | multi | never |

The new cell is (multi, finite). Before this lab, `multi` appeared in the table
only with `never` (the `change_log` tail). (multi, finite) is RxJava's
`Observable` with a completion guarantee, and it is the common case for shell
work: every extractor, every `git log`, every `find`.

Two consequences for the analysis:

1. **Lifetime is now a type-level claim with a link-time obligation.** The
   signature says `Stream(...)`, therefore finite. Whether the bound process
   actually exits is a property of the bind. A bind that attaches `tail -f` to
   a `Stream`-typed rel is a link error, not a runtime surprise. This is a new
   check the linker owes, and it has no home in ARCH.pl today.
2. **The static lifetime and the runtime lifetime are different objects.** The
   mode is per-rel and computed at compile time. The open/finished state is
   per-request and lives in a row (`Streams: Id-Status` in the lab). Dominance
   as mode-dominance.md describes it operates on the static one; teardown by
   range-DELETE operates on the runtime one. The plan does not currently name
   both.

`check(mode_is_functional)` grades that `result_mode/3` yields exactly one
mode per result type, so the analysis stays a fold with no search.

## 3. Backpressure

Verdict: bind and buffer policy, no surface syntax, with one condition.

The reasoning is that the tick already is the buffer. Arrivals are batched into
a tick and written in one transaction, so the runtime knob is "how many
arrivals may accumulate before the tick must commit". That is the same knob as
the write-batch bound, which exists independent of streaming. The three
available policies are the standard three:

| policy | fit for an extractor |
|---|---|
| block the pipe (real backpressure) | correct default. The OS pipe already does this when the reader stops draining, and a stalled extractor is harmless |
| buffer without a bound | a memory hazard, and it violates "only deltas cross the coastline" |
| drop | wrong. Dropping extraction lines silently corrupts the analysis, with no signal to any downstream rule |

The condition: **backpressure stays invisible only while the policy is
lossless**. The moment a bind may drop, the item type has to carry a gap
marker so downstream rules can see the hole, and that is a surface-visible enum
change. So the rule is: lossy transports must widen the item envelope. That
reuses the existing "failure is a value" law rather than adding a mechanism.

What does need surface visibility is the **retention bound on the item rel**,
which is a separate question from arrival rate. A rel that folds lines into an
aggregate keeps nothing; a rel that keeps every line grows without bound.
`algorithm(retention_bound, ast, fold, 'books/v6/algos/retention.pl')` already
exists and this is exactly its job. Streaming effects make it a requirement
rather than an optimization, because an extractor is the first source that can
produce unbounded rows from a single demand row.

## 4. Request identity: dedup, with a content salt

Decision: content-addressed dedup, unchanged from LANG. A repeated identical
demand row does not re-fire. Re-extraction is expressed by a salt column whose
value is **the input digest**, not an arrival tick.

`check(identical_demand_dedups)`: replaying the demand term after completion
leaves one stream, three rows, and records a `deduped` note.
`check(new_salt_refires_fresh_stream)`: the same args under a new digest opens
a second stream with its own id, its own rows, and its own completion fact.

Justification for content salt over clock salt: LANG's open question is stated
once, as "Edge-derived demand rows need arrival-tick salt or repeated identical
requests dedup into silence." That question actually has two answers depending
on why the request should recur. A poll recurs because time passed, so its salt
is the clock bucket, which is what ghcacher's
`poll(Ep, Prev, B) <- watch(Ep), cache_tag(Ep, Prev), every_300(B)` does. An
extractor recurs because the input changed, so its salt is the input digest.
With a digest salt, dedup and re-extract-on-change are the same rule and no
new mechanism appears. With a clock salt an extractor re-runs on a timer over
unchanged files, which is the exact waste the whole design is trying to avoid.

The lab also grades that the terminal is final: a `Line` arriving after `Done`
is refused and recorded, not appended (`check(terminal_is_terminal)`).

## 5. Ambiguities found in LANG.md

1. **"Envelope enums make the fill det" conflates two properties.** Det means
   (i) exactly one response row per request and (ii) failure is a value rather
   than a throw. Streaming keeps (ii) and drops (i). The sentence has to be
   split, because (ii) is a law and (i) is a mode.
2. **The `external = shell {...}` row in mode-dominance.md is stated as det.**
   Quoted above. Most useful shell commands are (multi, finite).
3. **"Demand rows = requests (content-addressed dedup)" does not say what the
   content is.** For a det HTTP fetch the input columns are the whole content.
   For an extractor over a file tree they are not, and dedup without a digest
   column means the extractor runs once, ever.
4. **The open question "edge-derived demand rows need arrival-tick salt" is
   one question where there are two.** Clock salt and content salt are
   different answers for different effects. See section 4.
5. **Static lifetime and runtime lifetime are not distinguished.** The mode is
   a compile-time claim about a rel; open/finished is a runtime row about a
   request. Both are called "lifetime" in LANG.md and mode-dominance.md.
6. **Which arrow does an effect fill use?** LANG defines `<-` (level, IVM
   retracts) and `<+` (edge, appends, never retracts) but does not say which
   one a response feeds. This lab used `<+` for items and for the terminal.
   That choice has a consequence the spec does not state: if the demand row is
   later retracted, the extracted rows stay forever, because occurrences cannot
   un-happen. Whether that is wanted for extraction output is undecided.
7. **Ordering within a tick is observable, which contradicts the time cut.**
   LANG says "A body is one time cut (all atoms at the same instant)". The lab
   delivers two lines in one tick and assigns them sequence numbers 1 and 2, so
   an intra-tick order exists and downstream rules can read it
   (`check(lines_land_in_arrival_order)`). Either JSONL order is not preserved,
   or a tick is not a single instant for a multi fill. This is the sharpest
   conflict the lab found.
8. **The bind grammar has no two-channel form.** LANG's example bind is one
   backtick command, one output tuple, one match. A streaming shell bind has
   two channels (`stdout_line` and `exit`) mapping to two different enums. The
   syntax shown cannot express it.
9. **Teardown of an in-flight stream has no surface name.** LANG says scope
   exit is a range-DELETE of the demand path prefix. For a running process that
   must also kill the process, and the bind is the only place that knows how.
   The lab does not model it; it is the one part of the effect lifecycle left
   untested here.
10. **Exhaustiveness-as-lint has no notion of which envelope.** surface-boil.md
    asks whether "no rule consumes Error(_)" should be a lint. With split
    envelopes a program may legitimately consume every item and no terminal
    (fire and forget). The lint needs to distinguish "ignored the item enum"
    from "ignored the terminal enum", and only the second is clearly a defect.

## 6. Deviations from the LANG snapshot

1. Added two type constructors in the arrow's result position: `Stream(Item,
   End)` and `Tail(Item)`. LANG lists only `Key(Type)` as a type-position
   wrapper. No new keyword was added, which is why this route was preferred
   over a `final` marker on constructors.
2. Candidate (a) as posed puts the terminal constructors in the item enum. This
   lab splits them into two enums. Justification in section 1.
3. The nonzero-exit guard sits in the bind rather than in a rule guard, so the
   program never sees an exit code it must interpret. This follows the existing
   ghcacher precedent rather than the spec text, which is silent on it.

## 7. Tier order

Streaming effects belong to the **temporal tier** for arrival and to the
**fact tier** for everything after arrival. The compiler tier gains one fold.

What they need:

| dependency | status in ARCH.pl | why |
|---|---|---|
| `envelope_types` | labbed (enum_match) | exhaustiveness runs twice instead of once, unchanged otherwise |
| `demand_clocking` | labbed | the demand row is the request; unchanged |
| `mode_lab` | unbuilt | must learn to read the result-type wrapper and to emit (multi, finite) |
| `retention_bound` | shelved | becomes a requirement, since one demand row can now produce unbounded rows |

What they do **not** need: `register_lowering`. The lab uses no register. Item
arrival is an append (`<+`), the terminal fact is an append, and the per-request
status is a small keyed row that the keyed-rel semantics already covers
(latest wins, at most one row per key). `check(stream_effect_grounds_in_kernel)`
grades this: every part list for the three new sugar entries lies inside
`{ground_terms, rule, external_rel}` and none contains `register`.

So streaming effects can land **before** `register_lowering` in the roadmap,
which is not where a reading of "many rows over time needs state" would put
them. The state that looks like it needs a register is the sequence counter and
the open/finished flag, and both are rows in a keyed rel rather than folds over
a delta.

One new task falls out that has no row in ARCH.pl today: the **link-time
lifetime obligation** from section 2, where the bind must discharge the
`Stream` type's finiteness claim. It depends on `protocols` binding and on
`mode_lab`, and it is the only genuinely new analysis this lab produced.

## 8. What the lab does not prove

- Teardown of a running process on scope exit (ambiguity 9). Not modelled.
- Concurrency between two open streams whose lines interleave in one tick. The
  lab runs two streams but never interleaves their arrivals.
- Any backpressure behavior. Section 3 is reasoning, not measurement; the pipe
  in the live checks is three lines long and never fills.
- The sqlite lowering. Per lab law, the rows here are terms and the emission
  question stays in prose.
