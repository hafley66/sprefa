# SWI-Prolog library ecosystem, 10.x line

Verified empirically against SWI-Prolog 10.0.2 (arm64-darwin, homebrew) on
2026-07-27. Every claim below has a receipt: either a `swipl -g ... -g halt`
transcript or a file/line citation. Scratch DBs used `/tmp`; packs installed
to the default `~/.local/share/swi-prolog/pack/`.

## SQLite verdict

Two live options, both working, not equivalent.

**prosqlite (pack, v2.0)**: builds clean via `pack_install(prosqlite,
[interactive(false)])` (one clang warning, harmless: unused-var path in
`c/prosqlite.c:97`). Round-tripped inserts and selects against a scratch file
and cross-checked with the `sqlite3` CLI:

```
?- sqlite_connect('/tmp/prosqlite_test.db', Conn, [ext(''), exists(false)]),
   sqlite_query(Conn, 'create table people (id integer primary key, name text, age integer)', _),
   sqlite_query(Conn, 'insert into people (name, age) values (\'alice\', 30)', _),
   findall(row(Id,Name,Age), sqlite_query(Conn, 'select id, name, age from people order by id', row(Id,Name,Age)), Rows),
   sqlite_disconnect(Conn).
Rows = [row(1,alice,30), row(2,bob,25)].
```
`sqlite3 /tmp/prosqlite_test.db "select * from people;"` returned the same
two rows. Gotcha found the hard way: the default `ext(sqlite)` option
silently appends `.sqlite` to any filename whose extension is not literally
`sqlite`, so `'/tmp/foo.db'` becomes `/tmp/foo.db.sqlite` unless you pass
`ext('')`. `sqlite_format_query/3` takes a `Format-Args` pair (`~w`/`~q`
style), not parameter placeholders, so building SQL text with it is manual
escaping, not real bind parameters. GitHub commit history
(nicos-angelopoulos/prosqlite) shows the last commit August 2024 (v2.0
release); no activity since, so call it dormant, not dead.

**swiplite (pack, v1.1, module name `library(sqlite)` despite the pack name)**:
last updated April 2026 per the pack page, actively maintained. Builds via
CMake against homebrew's libsqlite3 (3.53.2), its own bundled test suite
passes (1/1). It exposes the real SQLite C API shape: `sqlite_prepare/3`,
numbered `?1`/`?2` bind parameters via `sqlite_bind(Statement, bv(Val1,
Val2, ...))`, `sqlite_do/1` for non-SELECT, `sqlite_one/2` /
`sqlite_many/4` / `sqlite_row/2` for results. That is genuine parameterized
binding, not string formatting:

```
?- use_module(library(sqlite)),
   sqlite_open('/tmp/swiplite_test.db', DB, [mode(create)]),
   sql_command(DB, "create table people (id integer primary key, name text, age integer)"),
   setup_call_cleanup(
       sqlite_prepare(DB, "insert into people (name, age) values (?1, ?2)", Insert),
       ( sqlite_bind(Insert, bv('alice', 30)), sqlite_do(Insert),
         sqlite_reset(Insert),
         sqlite_bind(Insert, bv('bob', 25)), sqlite_do(Insert) ),
       sqlite_finalize(Insert)),
   sql_query_all(DB, "select id, name, age from people order by id", Rows),
   sqlite_close(DB).
Rows = [row(1,alice,30), row(2,bob,25)].
```
`sqlite3 /tmp/swiplite_test.db "select * from people;"` matched. It also
enables `foreign_keys` by default (`sqlite_open/3` option
`foreign_keys(true)`), which prosqlite does not surface at all.

**ODBC route**: `library(odbc)` loads cleanly (bundled, no pack needed) and
homebrew's `unixodbc` is already on this machine (`odbcinst -j` resolves
config paths). But `/opt/homebrew/etc/odbcinst.ini` has zero drivers
registered; there is no sqlite ODBC driver installed (would need
`sqliteodbc`, not currently brewed here). Untested beyond confirming the
Prolog-side module loads; the driver gap makes this the higher-setup-cost
path with no payoff over the two direct bindings above.

**Verdict for S8 (cross-checking against the TS/SQLite engine)**: use
**swiplite**, not prosqlite. Real bind parameters matter for anything built
programmatically (the conformance harness would otherwise be
string-interpolating SQL), it is the actively maintained one, and the API
surface (`sqlite_prepare`/`sqlite_bind`/`sqlite_many`) maps directly onto
what a cross-check harness needs: prepare once, bind per row, compare
against the TS engine's output. prosqlite's simpler `sqlite_query/3` is
fine for one-off ad hoc queries (schema dumps, smoke checks) but not for a
harness that binds values in a loop. Neither pack is bundled; both are
pack-installed C extensions, so "bought not built" is satisfied either way,
this is a pick-the-better-vendor decision, not a build-vs-buy one.

## Library / pack table

| library/pack | bundled or pack | verified | relevance to sprefa v6 |
|---|---|---|---|
| `library(sqlite)` (pack `swiplite`) | pack | receipt above, own test suite 1/1 pass | primary SQLite cross-check binding for S8 |
| `prosqlite` | pack | receipt above | fallback/simpler API, dormant since Aug 2024 |
| `library(odbc)` | bundled | loads; no sqlite driver installed, untested past that | not worth the driver-install cost given swiplite works |
| `library(plunit)` | bundled | `run_tests` with `forall(...)` sub-tests, `setup`/`cleanup` all passed; concurrency is `set_test_options([jobs(4)])`, NOT a per-`begin_tests` option (`concurrent(true)` prints `Unknown message: plunit(concurrent)` and is ignored, see `plunit.pl:572-574`) | v6's hand-rolled PASS-line harness in `v6/prolog/conformance/` duplicates what plunit does natively, including parallel test jobs |
| `library(prolog_coverage)` | bundled (note: not `library(test_cover)`, that name does not exist) | `show_coverage/1` loads after `use_module(library(prolog_coverage))` | coverage for the hand-rolled test set, currently absent |
| `library(persistency)` | bundled | wrote/reread a fact across two process runs, second run saw the persisted `assert_fact(foo,42)` without reassert | could replace ad hoc file-based state in v6 fixtures |
| `library(broadcast)` | bundled | `listen/3` + `broadcast/1` fired synchronously in-process | candidate for v6's hand-rolled event/trigger machinery |
| `library(settings)` | bundled | `setting/4` declared + read back (10) via `initialization/1` | not currently used; low urgency |
| `library(debug)` | bundled | loads; standard `debug/3` and `assertion/1` machinery | already idiomatic, no gap |
| `library(record)` | bundled | `record point(x:integer=0,y:integer=0)` compiled, `default_point/1` + accessor worked | dicts cover this need in v6's SQL-facing code; record still useful for accessor-heavy fixed-shape terms |
| `library(dcg/basics)` | bundled | loads | untouched by v6 so far, available if a DSL parser is ever needed |
| `library(dcg/high_order)` | bundled | loads | ditto |
| `library(pure_input)` / `phrase_from_file/2` | bundled | `phrase_from_file/2` resolves after loading `pure_input`; `/3` form does not exist | for streaming fixture files without loading them whole |
| `library(http/thread_httpd)` + `library(http/http_dispatch)` | bundled | live server started, `curl` round-trip returned `hello from swipl` | if v6 ever needs an HTTP surface for cross-checking against the TS/rxjs HTTP boundary |
| `library(http/http_open)` | bundled | loads | HTTP client side of the above |
| `library(http/websocket)` | bundled | loads | untested live; module present |
| `library(http/sse)` (Server-Sent Events) | **NOT present in 10.0.2** | `use_module` fails, `source_sink ... does not exist` | landed later, in 10.1.11 (`sse_open/0,1`, `sse_send/1,2`); this install predates it, do not assume it exists |
| `library(process)` | bundled | `process_create(path(echo), ['hello','world'], [stdout(pipe(Out))])` read back `"hello world"` | shell effects with real argv lists, no shell-string quoting hazard |
| `library(redis)` | bundled | loads (connect attempt against a closed port failed as expected, module itself is fine) | bundled client, no extra pack needed if v6 ever wants a redis-backed queue (repo law says buy, not build, for queues anyway) |
| `library(stomp)` | bundled | file present, not exercised live | bundled, same caveat |
| MQTT | pack (`mqtt`, wraps mosquitto) | not installed/tested, found via pack list only | third-party wrap, not bundled; lower priority |
| `library(yall)` | bundled | `maplist([X,Y]>>(Y is X*2), [1,2,3], L)` gave `[2,4,6]` | lambda shorthand, already idiomatic Prolog style |
| `library(apply_macros)` | bundled | loads (goal-expansion library, effect is compile-time inlining of `maplist`/`foldl`, not independently visible via `-g`) | perf-only; skip until a hot loop actually needs it |
| `library(solution_sequences)` | bundled | `distinct(X, member(X,[1,1,2,2,3]))` gave `[1,2,3]` | dedup without hand-rolled sort/dedup |
| `library(aggregate)` | bundled | `aggregate_all(count/bag/sum, ...)` all correct | already the right tool for any fixture summary stats |
| `library(assoc)` | bundled | `list_to_assoc` + `get_assoc` worked | AVL-tree map, fine for small fixture-side lookups |
| `library(rbtrees)` | bundled | loads | lower-level twin of assoc; assoc is the friendlier front end over the same tree |
| `trie` (builtin, not a library) | **builtin in core**, no `use_module` needed | `trie_new/1`, `trie_insert/2`, `trie_gen/2` all worked directly | genuinely new to an older-Prolog programmer: this used to require a pack, now it is a C-level builtin with GC'd blobs |
| `library(nb_set)` | bundled | `empty_nb_set/1` + `add_nb_set/2` + `nb_set_to_list/2` deduplicated `[3,1,2,1]` to `[1,2,3]` | destructive/non-backtrackable set, useful for tick-scoped accumulation without an assoc rebuild each time |
| global variables (`nb_setval/2`, `nb_getval/2`) | bundled, core | `nb_setval(x,1), nb_getval(x,V)` gave `1` | already idiomatic; no gap |
| `library(tableutil)` | bundled, **new in 10.0.2** | not exercised, found via changelog | toplevel utility to dump tables/relations/statistics; worth a look for `dl daemon health`-style introspection tooling parity, prolog side |

## New in 9.2+/10.x that an older-Prolog programmer would not know exists

- `trie_new/1` and friends are now core builtins (C-level, garbage-collected
  blobs), not a pack. Confirmed: `current_predicate(trie_new/1)` succeeds
  with zero `use_module`.
- `library(tableutil)`, bundled starting 10.0.2: toplevel dump of tables,
  their relations, and statistics.
- `library(http/sse)` (Server-Sent Events: `sse_open/0,1`, `sse_send/1,2`,
  `sse_comment/1,2`, honors CORS via `http_cors`) landed in 10.1.11, a
  version newer than the 10.0.2 on this machine. Do not assume it is present
  without checking the installed point release.
- Thread classes and a `debug_mode(-Boolean)` property on
  `thread_property/2` for finer debugging control per changelog, not
  independently exercised here.
- `create_prolog_flag/3` gained `local(true)` for thread-local flags.

## Top 5 by payoff for this repo

1. **swiplite for S8.** Direct, parameterized, actively maintained SQLite
   binding. Working receipt above. This unblocks the standing
   cross-check-against-TS/SQLite question without any bespoke FFI work,
   consistent with "infra is bought, never built."
2. **plunit's native `jobs(N)` concurrency + `setup`/`cleanup` +
   `forall`.** v6's hand-rolled PASS-line harness in
   `v6/prolog/conformance/` is reimplementing test-runner mechanics
   (sequencing, sub-test expansion, timing) that plunit already ships and
   that this session confirmed work correctly. Migrating is a real
   reduction in bespoke surface, not a style preference.
3. **`library(prolog_coverage)`'s `show_coverage/1`.** No coverage signal
   exists today for the conformance suite; this is a zero-cost bundled
   addition once tests run under plunit.
4. **`library(process)` for shell effects.** Confirmed argv-list based
   `process_create/3` avoids shell-string injection entirely; relevant
   anywhere v6 shells out (e.g. driving the TS engine as a subprocess for
   cross-checks).
5. **`library(persistency)`.** Confirmed durable fact storage across
   process restarts with a two-line declaration. Candidate for replacing
   any hand-rolled file-based state in the fixture or lifetime-lab work
   (`v6/prolog/labs/sub_lifetimes.pl`, `mode_lab.pl`) if those need to
   survive across runs.
