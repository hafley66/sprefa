# CST queries, the human version

## where it stands

```
.dl6 rule with ts_query(...)  --compiles-->  query text     (works today)
query text  --runs on a real tree-sitter-->  captured rows  (ZERO lines exist)
```

today the compiler happily compiles the query and then NOTHING executes it.
docs claim there is a refusal. there is not. silence, not refusal.

## ladder A: build the runner

- lives in sprefa-extract. zero new deps: tree-sitter already there,
  ast-grep already exposes the parsed tree. one parse serves both.
- mirror the existing `--ast-pattern` CLI shape.
- decide EARLY (step A3): tree-sitter queries and ast-grep metavariables,
  one executor or two? the cache digest depends on the answer.

## ladder B: extract --serve

- suspected 87x slowdown from spawning a process per call.
- MEASURE FIRST: nothing separates spawn cost from parse cost yet.
- the one line that matters if it is queueing: an inner concatMap
  (serial) where a mergeMap (parallel) belongs.
- buy-check done: talk NDJSON over stdio ourselves, libraries add cost.

## maybe-live BUG found on the way

```
cache key = (host, digest-of-inputs)
query template is NOT an input
=> change the template, cache serves YESTERDAY's rows. forever.
```

verified on paper, experiment not run. needs your ok (mutates store state).

## packet C: how do you SPELL a query (your ruling)

| | looks like | today |
|---|---|---|
| (a) quoted | `ts("(call_expression ...)", ...)` | not built |
| (b) native | `ts((call_expression ...), ...)` | not built |
| (c) bare term | `ts_query([node(call_expression, ...)])` | works now |

- the "native gets error squiggles free" claim died on audit. errors point
  at the whole statement either way. both need the same ~155 lines for
  real inner squiggles.
- (b)'s hidden cost: `(foo ...)` collides with parenthesized math in the
  grammar. 25-50 lines of disambiguation + risk to a big parser test.
- (a)'s hidden cost: 20-40 lines to make syntax errors point inside the
  string. after that (a) == (b) on errors.
- (c) is ugly but ships nothing and stays either way.

3 questions before anyone builds:
1. langium parser still demoted? (if yes, both sugars cost 0 there)
2. is editor syntax-coloring inside the pattern worth (b)'s collision work?
3. does (c) stay a legal spelling after a sugar lands?
