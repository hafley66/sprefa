# 12. Effects and the daemon

> `sh` effects, `@async`, the clock, and why results only arrive under the daemon.

**Goal:** run a shell command *from a rule*, on a schedule, and understand the
one-shot/daemon split that every effect program lives with.

Everything so far derived facts from facts. An **effect** derives facts from
the world: a shell command runs, its output becomes rows. The design keeps the
datalog pure; a rule never blocks on a command. Instead it *requests* the
effect, and the answer arrives as ordinary rows on a later tick.

This lesson's program has no fixture at all. Make a fresh directory, and keep
its path short (the daemon binds a Unix socket at `<root>/.dl/daemon.sock`,
and macOS caps socket paths at 104 bytes, so a deeply nested root cannot host
a daemon):

```sh
mkdir -p /tmp/dl-tour/.dl && cd /tmp/dl-tour
```

## The program

Save as `12.dl`:

```dl
rel probe(bucket: int).
probe(bucket) <- clock(10, bucket).

sh now(bucket) -> (stamp: text) = `printf 'tick %s at %s' {bucket} "$(date -u +%H:%M:%SZ)"`.

rel observed(bucket: int, stamp: text).
observed(bucket, stamp) <- @async probe(bucket), now(bucket) -> (stamp).

? observed(bucket, stamp).
```

Three pieces:

- `clock(10, bucket)` is a built-in: the current time, floored to a 10-second
  bucket. A new bucket value appears every 10 seconds, which makes any rule
  joining it re-derive on that cadence. This is the only clock there is; there
  is no `sleep`, no timer callback.
- `sh now(bucket) -> (stamp: text) = \`...\`` declares the effect: a name, its
  parameters, the columns it returns, and a shell template. `{bucket}`
  interpolates the argument. One line of stdout fills one result row.
- The `@async` rule wires them: whenever a `probe(bucket)` row exists, request
  `now(bucket)`, and when its result exists, head an `observed` row.

One rule of the template language: every parameter must appear in the
template. Declare `sh now(bucket)` without using `{bucket}` and the
typechecker stops you before anything runs:

```
12.dl:1: error[unused-hole]: `sh now` param `bucket` never appears as `{bucket}` or `$bucket` in the template
```

That is deliberate: an effect's identity is its arguments. An argument the
command ignores would mean two different requests producing the same command,
and the cache (below) would serve one's answer to the other.

## Run it one-shot: nothing

```sh
dl 12.dl --root /tmp/dl-tour --no-daemon
```

```
? observed => bucket	stamp
  (0 rows)
```

Zero rows, exit 0, no error. This is the gotcha to internalize: a one-shot run
performs **one tick**. The tick derived `probe`, queued the `now` request, and
exited. Nobody was left alive to run the command and feed the answer back.
Effects need a process that outlives the tick: the daemon.

## Run it under the daemon

Discovery mode (a bare `dl` with no program argument) merges every `.dl` file
in `<root>/.dl/` and keeps a daemon running for that root:

```sh
cp 12.dl .dl/12.dl
dl --root /tmp/dl-tour        # first call spawns the daemon, prints 0 rows
sleep 8
dl --root /tmp/dl-tour        # attaches to the warm daemon
```

```
? observed => bucket	stamp
  178334140	tick 178334140 at 12:37:27Z
  178334141	tick 178334141 at 12:37:27Z
  178334142	tick 178334142 at 12:37:27Z
  178334143	tick 178334143 at 12:37:27Z
  178334144	tick 178334144 at 12:37:31Z
  (5 rows)
```

(Your bucket numbers and stamps will differ; they are wall-clock derived. The
shape is what to check.)

Read the rows carefully, because they show two behaviors at once:

- **Each bucket appears exactly once.** The effect id is content-addressed on
  `(head, kind, args)`. The daemon ticks every couple of seconds and `probe`
  keeps re-deriving, but `now(178334140)` has already run, so it never fires
  again. Joining the clock into the arguments is *the* rate-limit idiom: no
  clock, one request ever; `clock(10, ...)`, one request per 10 seconds.
- Requests that queued while nothing was draining (here, buckets queued
  before the daemon settled) all drain together and carry the same stamp.
  The rows record when each command *ran*, not when its bucket began.

When you are done, stop the daemon and check its log; effect failures land
there, not in your terminal:

```sh
dl --stop --root /tmp/dl-tour
cat .dl/daemon.log | tail
```

## The wider family

`sh` is the read-only kind. Its siblings, same shape, different contracts:

- `sh!` marks a mutation: exactly-once. A request claims its slot atomically
  before running, so a crash mid-flight quarantines rather than double-fires.
- `sh*` is a generator: every stdout line fans into its own row (crawl a
  package registry, list a queue).
- `@next` carries a relation's rows into the *next* tick, which is how a
  fetched value (an etag, a cursor) becomes an input to the following fetch.

`dl examples --show gh-cache` combines all of them into a conditional-GET
GitHub poller in ~90 lines and is the canonical next read. One honest warning
from its school of hard knocks: the effect cache keys on arguments, *not* on
the template text, so editing the backtick body and re-running is a silent
no-op for already-answered requests (start a fresh `--db`, or change an
argument).

## Exercise

Break the command (misspell `date`), reload, and find the failure in
`.dl/daemon.log`. Then fix it and explain why the previously failed buckets
do or do not re-run, using the content-addressing rule.
