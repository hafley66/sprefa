# Haskell idioms that professional projects actually use

Lane L4. Every claim below carries a `file:line` from a repo cloned into
`/tmp/hs-refs/` (shallow, one commit each, SHAs in REPORT.md) or a compiled
probe in `probes/`. Claims with neither are in section 9. All `rg`/count
commands are shown or are reproducible on the cloned trees.

Corpus: postgrest, graphql-engine (hasura), haskell-language-server (HLS),
servant, rio, katip, co-log, hs-opentelemetry, effectful, safe-exceptions.

## 1. The application monad

Answer: a `ReaderT Env IO`. Every shipping server in the corpus is a
`ReaderT` over `IO` under the hood, sometimes newtype-wrapped, sometimes with
an extra layer.

| project | type | where |
|---|---|---|
| graphql-engine | `newtype AppM a = AppM (ReaderT AppEnv (TraceT IO) a)` | graphql-engine/server/src-lib/Hasura/App.hs:672 |
| HLS | `newtype IdeAction a = IdeAction { runIdeActionT :: (ReaderT ShakeExtras IO) a }` | haskell-language-server/ghcide/src/Development/IDE/Core/Shake.hs:1081 |
| rio | `newtype RIO env a = RIO { unRIO :: ReaderT env IO a }` | rio/rio/src/RIO/Prelude/RIO.hs:38 |
| effectful | `newtype Eff (es :: [Effect]) a = Eff (Env es -> IO a)` | effectful/effectful-core/src/Effectful/Internal/Monad.hs:126 |
| servant | `newtype Handler a = Handler { runHandler' :: ExceptT ServerError IO a }` | servant/servant-server/src/Servant/Server/Internal/Handler.hs:20 |
| postgrest | explicit `AppState` threading; `ExceptT` at the handler edge | postgrest/src/library/PostgREST/App.hs:160, src/library/PostgREST/MainTx.hs:54 |

Reading the table: rio is a newtype over `ReaderT env IO`, and its own
documentation says so (`rio/rio/src/RIO/Prelude/RIO.hs:37-46`). effectful's
`Eff` is `Env es -> IO a`, structurally a `ReaderT` over `IO`; its README says
so in words: "essentially a ReaderT over IO on steroids"
(effectful/README.md:77). graphql-engine and HLS, the two biggest, use a plain
`ReaderT` with the env type explicit and no effects library.

Which one a new 2026 project starts with: `ReaderT Env IO`, newtype-wrapped,
with `mtl`. Backed by graphql-engine (App.hs:672) and HLS (Shake.hs:1081)
using exactly that, and rio defining itself as that (Prelude/RIO.hs:38).
`effectful` is the current contender but none of the four shipping servers use
it; it is solo in its own repo. `polysemy` appears nowhere in the corpus.

Postgrest is the deliberate outlier: it threads `AppState` as an explicit
argument rather than through a `Reader` class, and uses `ExceptT` for the
handler boundary (App.hs:160 `runExceptT`). Its whole error story is
`Either`-shaped at the boundary (section 4).

The starter (`starter/src/Main.hs:25-31`) uses exactly the recommended shape:
`newtype App a = App { unApp :: ReaderT Env IO a }`.

## 2. Logging

Number of source files importing each logger, per project (command:
`grep -rl "import.*<Pat>" <proj> --include='*.hs'`):

| logger | graphql | HLS | postgrest | rio | katip | co-log | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|
| Katip | - | - | - | - | 24 | - | 2 | - |
| Colog (co-log) | - | 3 | - | - | - | 12 | 2 | - |
| System.Log.FastLogger | 5 | - | 1 | 1 | - | - | 1 | - |
| Control.Monad.Logger | - | - | - | 1 | - | - | 4 | - |

Interpretation:

- graphql-engine and postgrest log through `System.Log.FastLogger`
  (graphql-engine/server/src-lib/Hasura/Logging.hs:89 imports `System.Log.FastLogger qualified as FL`; postgrest/src/library/PostgREST/Logger/Apache.hs:7). That is the low-level route: you build `LogStr`, the library writes to a handle with a lock. Neither project uses a "structured logging" package; they build their own message renderers on FastLogger.
- HLS uses co-log: `Ide/Logger.hs:33 import Colog.Core (LogAction...)` and threads a `LogAction` around (`LanguageServer.hs:38 import Colog.Core qualified as Colog`, `LanguageServer.hs:176` builds a `Colog.LogAction`).
- rio ships its own `LogFunc` in the env (`rio/src/RIO/Prelude/Logger.hs:128 data LogFunc = LogFunc`), re-exported as `logInfo`/`logDebug` (Logger.hs:179,183). It is snoyberg's house style: a `LogFunc` field in the `LogOptions`/env, not a typeclass.

So the "two schools" are real: structured via co-log (HLS) and raw via
FastLogger (postgrest, graphql-engine). katip and monad-logger are barely used
outside their own repos (katip's 24 are its own tests; monad-logger shows up
only as a transitive use in hs-opentelemetry and rio).

How the logger is threaded: in the env, not passed per-call, not global.
HLS keeps a `LogAction` in shake extras; postgrest keeps a `LoggerState` and
reads it via `getLogger` (`Logger.hs:72`); rio reads `LogFunc` from the env.

How levels are set at runtime: config-driven. postgrest reads
`log-level` into `LogLevel` (`postgrest/src/library/PostgREST/Config.hs:109,133`:
`data LogLevel = LogCrit | LogError | LogWarn | LogInfo | LogDebug`) and the
logger reads it live on each event because it can be reloaded
(`Logger.hs:61-84`, gating with `logLevel >= LogError`).

A real log line from the running probe, verbatim (probes/logprobe, co-log-core
on GHC 9.14.1):

```
level=info component=logprobe msg=hello_from_logprobe
```

The starter emits the same shape:

```
starting
config=port=8080
workers=4
app started on port 8080
```

Build-vs-buy for logging: buy. Candidates checked and why:
- `katip`: builds on 9.14.1 (verified by building `katip-0.8.8.4` with `cabal build`; took ~9s of compile). Rejected as a default for a new project: it pulls a lens-based stack, and its API ceremony (a `Scribe`, `registerScribe`, `runKatipContextT` with namespace and context) is heavier than the single `LogAction` that HLS already uses. Verified: its imports stay inside its own repo in the corpus.
- `co-log` (co-log-core): builds on 9.14.1 (probes/logprobe). Used by HLS. Chosen for the starter.
- `fast-logger`: the low-level backend postgrest and graphql-engine use; right choice when you want max control, wrong default for a small starter.
- `monad-logger`: a typeclass (not a value), the RIO/older style; in the corpus it appears only as incidental use, not as an app default.

## 3. Tracing

- HLS traces with `hs-opentelemetry`: `ghcide/src/Development/IDE/Core/Tracing.hs:33 imports OpenTelemetry.Eventlog (SpanInFlight...)` and wraps each handler in a span ("Trace a handler using OpenTelemetry", Tracing.hs:69).
- graphql-engine does not use hs-opentelemetry; it has its own `TraceT` layer and `ignoreTraceT`/`runTraceT` in its monad (`Hasura/App.hs:672,689`, `Hasura/Tracing/Class.hs:56`).
- hs-opentelemetry itself (the reference) spans `OpenTelemetry.Eventlog`, `OpenTelemetry.Trace`, exporters for OTLP and Jaeger, in `hs-opentelemetry/src/OpenTelemetry/*`.

Maturity verdict, measured: equitable. HLS, a long-running production LSP, uses
`OpenTelemetry.Eventlog`, which is the low-touch path (write events into the
GHC eventlog, open with eventlog2html or the OTel processor). The full
instrument-and-export-with-a-collector path (OTLP/Jaeger exporters) exists in
hs-opentelemetry but the corpus does not show a server wiring it; the one
production consumer, HLS, uses only the eventlog backend. So: depend on
hs-opentelemetry for the eventlog backend, which is proven in the corpus; treat
the wire-export path as unproven (section 9).

## 4. Errors and exceptions

Counts of files importing `Control.Exception.Safe` (safe-exceptions) vs
`Control.Exception` (base) vs `Control.Monad.Catch`:

| library | graphql | HLS | postgrest | servant | rio | katip | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|
| Control.Exception.Safe | 19 | 12 | - | - | - | 9 | - | 1 |
| Control.Exception (base) | 44 | 42 | - | 17 | 1 | 7 | 49 | 12 |
| Control.Monad.Catch | 18 | 3 | - | 7 | 3 | 1 | 2 | 5 |

(postgrest imports exceptions transitively via `Protolude`; 59 of its source
files import Protolude.)

The rule the projects follow, not the one the tutorials teach:

1. Normal expected failure is `Either`/`ExceptT`, lifted at the boundary, not
   thrown. postgrest: `type DbHandler = ExceptT Error SQL.Transaction`
   (`postgrest/src/library/PostgREST/MainTx.hs:54`) and `runExceptT` at the handler edge
   (`App.hs:160`). servant: `Handler = ExceptT ServerError IO`
   (`Servant/Server/Internal/Handler.hs:20`).
2. Unexpected/resource failure is an exception, signalled with `throwIO`
   (never `throw`), delimited with `bracket`, caught for an exact type.
   safe-exceptions is the packaged form of this discipline and is used by
   graphql-engine (19 files) and HLS (12 files): `Control.Exception.Safe`
   exports `bracket`, `withException`, `catch` and the throw/catchAsynchon
   split (`safe-exceptions/src/Control/Exception/Safe.hs:15-59`).
3. `bracket`/`bracketOnError` are used for resources directly. postgrest:
   `bracket (initAdminServerSocket conf) ensureSocketClosed` (`App.hs:88`),
   `bracket (initServerSocket conf) NS.close` (`App.hs:107`).
4. `HasCallStack` is pervasive, especially in effectful (63 files),
   graphql-engine (35), HLS (26). It is how a thrown error carries the call
   site without manually passing locations. `error`/`undefined` still appear
   as programmer faults but production paths prefer `throwIO` with a call
   stack.

The starter (`starter/src/Main.hs:53-72`) demonstrates both: the port parse is
`Either` lifted with a throw at the edge, and the required config file is a
`bracket`-delimited handle whose `IOException` is caught for that type only,
with `throwIO` used for signalling.

## 5. Debugging a running process

- `Debug.Trace` survives and is used in shipping code: graphql-engine (3 files), HLS (6), rio (1). It survives because it is the cheapest way to print a value into stderr at a point without rebuilding an app monad; it is deliberately not wired into logging.
- `HasCallStack` annotations give printed stack traces on `error`/throw without a compile-time cost; counts above.
- `-xc` RTS option prints the error context at runtime; it is a `+RTS -xc`, always available in `-rtsopts` builds (see `+RTS --help` text from any `-rtsopts` binary).
- `ghci` breakpoints: `:break`, `:step`, `:list` work on any ghc; not part of the shipping process.
- `ghc-debug` is real and in production: graphql-engine links `ghc-debug-stub` (`graphql-engine/server/graphql-engine.cabal:281`, `:1065`) and opens a debug socket gated on `HASURA_GHC_DEBUG` (`server/src-exec/Main.hs:18 import GHC.Debug.Stub`, `:169-177`). This is the live-heap inspector; it needs the `ghc-debug` client plus an eventlog-capable binary.
- eventlog (`-l`) is what threadscope and eventlog2html consume. Verified generating on 9.14.1: `probes/memprobe +RTS -l` wrote a 10 KB `memprobe.eventlog` (section 6).
- reachable on a laptop today: `Debug.Trace`, ghci breakpoints, `-xc`, `HasCallStack`, eventlog `-l` + threadscope/eventlog2html. `ghc-debug` is reachable only if you link `ghc-debug-stub` like graphql-engine; it is not a free laptop-stock item, so whether the harness should adopt it is a question for section 9.

## 6. Profiling: time, allocation, peak memory (customer section)

This section is runnable. Probe: `probes/memprobe`, which builds a real list of
`Int` with explicit recursion (so the optimizer cannot fuse it away:
`probes/memprobe/Main.hs:8-13`) and walks it. Run `n=600000`.

### `+RTS -s` lines, verbatim, GHC 9.14.1 (`probes/memprobe/Main.hs`, run with `+RTS -s`)

```
      48,452,392 bytes allocated in the heap
      47,570,640 bytes copied during GC
      18,366,456 bytes maximum residency (3 sample(s))
       3,158,024 bytes maximum slop
              44 MiB total memory in use (0 MiB lost due to fragmentation)
```

What each line means:

- `bytes allocated in the heap` (48,452,392) is TOTAL allocation volume over
  the whole run: every byte the program ever asked the allocator for, summed.
  It is a throughput number, countably much bigger than any simultaneous
  amount.
- `maximum residency` (18,366,456) is the largest amount of live heap observed
  at a GC sample point. This is the "how much does the data actually need"
  number a capacity planner wants.
- `total memory in use` (44 MiB) is what the RTS asked the OS for: the heap
  reserved and kept, rounded to the segment granularity, including the slop.
  It is larger than residency because the RTS keeps allocated segments around
  for reuse.

### `+RTS -t --machine-readable`: the bench harness record

On GHC 9.14.1 this prints a single Haskell list of `(field, value)` string
pairs to stderr. The field names a harness must read (verbatim selection from
`probes/memprobe +RTS -t --machine-readable`, n=600000):

```
 [("bytes allocated", "48452392")
 ,("max_bytes_used", "18366456")
 ,("max_mem_in_use_bytes", "46137344")
 ,("peak_megabytes_allocated", "44")
 ,("total_wall_seconds", "0.037611")
 ,("total_cpu_seconds", "0.031293")
 ,("num_GCs", "12")
 ,("gen_0_collections", "9")
 ,("gen_1_collections", "3")
 ...
 ]
```

So a bench harness pins on three pairs: `bytes allocated` (volume),
`max_bytes_used` (residency, = maximum residency), and
`max_mem_in_use_bytes` (peak RTS memory). `-t --machine-readable` is the
parseable one-line-equivalent record and is what the harness should consume,
not `-s` text.

### maximum residency vs `/usr/bin/time -l` maximum resident set size

One probe, n=600000, both measured on the same run:

| quantity | bytes | MiB | source |
|---|---|---|---|
| bytes allocated in the heap | 48,452,392 | 46.2 | `+RTS -s` |
| maximum residency | 18,366,456 | 17.5 | `+RTS -s` / `-t` `max_bytes_used` |
| total memory in use (RTS) | 46,137,344 | 44.0 | `-t` `max_mem_in_use_bytes` |
| /usr/bin/time -l maximum resident set size | 51,396,608 | 49.0 | `/usr/bin/time -l` |
| /usr/bin/time -l peak memory footprint | 49,038,400 | 46.8 | `/usr/bin/time -l` |

The gap, explained:

- `maximum residency` (18.4 MB) and `maxrss` (51.4 MB) disagree because they
  measure different things. Residency is only the live Haskell heap observed
  at GC samples. maxrss is the peak resident memory of the whole OS process:
  the binary and its shared libraries (libgmp, the runtime's own machinery,
  system frameworks on macOS), the RTS-reserved-but-not-live address space,
  and pages the OS happened to keep resident. The 5.3 MB difference over
  `max_mem_in_use_bytes` (44 MiB) is the non-heap footprint the OS sees that
  the RTS does not account in its heap number.
- `bytes allocated` (48.4 MB volume) is neither: it is cumulative allocation
  throughput, not a peak. In this short run it happens to sit near maxrss only
  because nearly everything allocated stays live.
- Conclusion for the harness: if you want allocation volume, read `bytes
  allocated`; if you want live-heap peak, read `max_bytes_used`; if you want
  OS footprint, only `/usr/bin/time -l` gives you maxrss, and it is the one
  number that is comparable across totally different runtimes (an unrelated
  process). Never conflate the three.

### Heap profiling

- Without a profiling build: `+RTS -hT` (closure types). Verified on 9.14.1:
  `probes/memprobe +RTS -hT` wrote `memprobe.hp`. Works on a normal `-rtsopts`
  build.
- With a profiling build (`-prof -fprof-auto`): `-hc` (by cost centre) and
  `-hy` (by type). Verified: compiled `probes/memprobe/Main.hs` with `ghc -O2
  -prof -fprof-auto -rtsopts` and ran `+RTS -hy`, which produced a
  `memprobe-prof.hp` grouped by type (the `Int` list shows as list/Int
  closures). `hp2ps` (shipped with ghc, at `/opt/homebrew/bin/hp2ps`) turns the
  `.hp` file into a PostScript/EPS picture.
- `eventlog2html` was not installed in this environment (`which` found nothing),
  so the eventlog-to-HTML step is noted but not run here; the `-l` eventlog
  itself was verified (10 KB `memprobe.eventlog` produced by `+RTS -l`). See
  section 9.

### `-rtsopts`, `-threaded`, `-with-rtsopts`

- `-rtsopts` lets the binary accept `+RTS ...` on the command line. It costs
  nothing at runtime and is on in probes and starter. Set it on any
  executable a harness will measure; without it, `+RTS -s` is rejected.
- `-threaded` links the threaded runtime and is required for `-N`. In this
  session, building with `-with-rtsopts=-N1` without `-threaded` made the
  binary refuse to start with "the flag -N1 requires the program to be built
  with -threaded" (a real failure I hit and fixed by dropping `-N1`). Decide
  early whether the benchmark is single- or multi-cpu and set the flag once.
- `-with-rtsopts` bakes RTS options into the binary. Useful to force an
  option even when the launcher ignores `+RTS`, but it fights a harness that
  wants to vary options: prefer `+RTS` on the command line and leave the
  binary clean.

### GC knobs a benchmark must PIN

Measured on the same probe (n=600000, `+RTS -s`), residency column:

| RTS | bytes allocated | max residency | total memory in use |
|---|---|---|---|
| (default) | 48,452,392 | 18,366,456 | 44 MiB |
| `-A32m` | 48,419,384 | 44,328 | 68 MiB |
| `-M16M` | 21,009,712 | 13,123,576 | 21 MiB |
| `-H64M` | 48,452,392 | 12,206,200 | 61 MiB |

`-A` (allocation area size) changes residency dramatically: with `-A32m` the
whole list fits in the nurseries so a GC never runs and recorded residency
collapses to 44 KB, while `allocated` stays ~equal. `-M` (max heap) throttles
total allocation and forces earlier GCs. These make runs incomparable unless
pinned. A benchmark must pin at least `-A` (and should pin `-M`/`-H` if it
sets them at all), and must pick a single value and record it with the run.
The two-number comparison table above used the default RTS for both GHC and
`/usr/bin/time` so the gap shown is the pure runtime difference.

## 7. Testing

Files importing each test library, per project (command `grep -rl "Test.Hspec"
etc.):

| lib | graphql | HLS | postgrest | servant | rio | katip | co-log | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|---|
| hspec (`Test.Hspec`) | 298 | 3 | 60 | 44 | 12 | - | - | 69 | - |
| tasty (`Test.Tasty`) | 2 | 43 | - | 2 | - | 9 | - | 5 | 23 |
| QuickCheck | 12 | 10 | - | 17 | 1 | 2 | - | 2 | - |
| hedgehog | 31 | - | - | - | - | 1 | 1 | 1 | - |
| golden (tasty-golden/goldenVsString) | - | 2 | - | 1 | - | 1 | - | - | - |

- hspec is the dominant fixture framework (graphql-engine 298, hs-otel 69,
  postgrest 60, servant 44). tasty is HLS's choice (43) and effectful's (23).
  Either is in common use; the corpus has no single winner across projects.
- property testing: QuickCheck (servant 17, graphql-engine 12) and hedgehog
  (graphql-engine 31) both appear; graphql-engine uses both.
- golden testing exists but is minor (HLS 2, katip 1, servant 1). Golden is
  HLS's idiom for output-file comparisons.
- `test-suite` stanza shape: cabal `test-suite ... type: exitcode-stdio-1.0
  main-is:` (see `labs/hs-idioms/starter/starter.cabal:15-20` for the identical
  shape; grep any of the cloned `.cabal` files for `test-suite`).

## 8. Project layout and tooling

Counted from the cloned trees:

| project | stack.yaml | package.yaml (hpack) | .cabal | formatter cfg | hlint cfg | CI (GitHub Actions) |
|---|---|---|---|---|---|---|
| postgrest | y | n | y | - | - | y |
| graphql-engine | n | n | y | - | y | CI (cabal/stack both) |
| HLS | y | n | y | - | y | y (cabal & stack) |
| servant | y | n | y | fourmolu.yaml | y | y (cabal) |
| rio | y | y | - | - | y | y (stack) |
| katip | y | n | y | - | - | n |
| co-log | n | n | y | - | y | y (cabal) |
| hs-opentelemetry | y | y | y | fourmolu.yaml | y | n |
| effectful | n | n | y | - | - | y (cabal) |
| safe-exceptions | y | n | y | - | - | y (stack) |

Read the table:

- Build tool 2026: cabal is the default in CI even where `stack.yaml` exists
  (HLS, servant, postgrest, co-log run cabal in CI). Only rio,
  safe-exceptions, and katip's CI are stack-only. `stack.yaml` surviving in
  repos is mostly legacy; the active CI is cabal. For a new project, cabal.
- hpack (`package.yaml`) is rare: only rio and hs-opentelemetry use it. Hand
  `.cabal` (with `cabal-version: 2.4`, per-component dirs) is the norm.
- formatter: `fourmolu` is the explicit pick (servant, hs-opentelemetry and
  their CI), `ormolu` appears only as an HLS plugin name. No project at repo
  root pins ormolu. HLS ships both formatter plugins
  (`haskell-language-server/.github/workflows/test.yml:179-184`).
- hlint: present (`.hlint.yaml`, config or CI) in graphql-engine, HLS, servant,
  rio, hs-opentelemetry, co-log. Not universal (postgrest, katip,
  safe-exceptions, effectful do not pin it).
- CI is GitHub Actions for 7 of 10; the 3 without (hs-opentelemetry, katip,
  co-log) rely on nix/Hackage build checks.

## 9. Unproven

Claims in this document with no citation and no running probe:

- `-xc` behavior. I listed it as available on `-rtsopts` builds from the
  `+RTS --help` text but did not run a program that triggers a runtime error
  to show an actual `-xc` backtrace. The claim "prints a stack trace on
  error" is from the RTS help and from `HasCallStack` documentation, not from
  a probe run here.
- `eventlog2html` rendering. `which eventlog2html` found nothing in this
  environment, so the heap-profile-to-HTML step was not executed. Only the
  raw `-l` eventlog file was produced.
- `threadscope` was not available to open the generated eventlog, so no
  threadscope screenshot/graph was produced.
- The hs-opentelemetry wire/export path (OTLP gRPC/HTTP exporter feeding a
  collector) is unproven: the one corpus consumer (HLS) uses only the
  eventlog backend. I did not stand up a collector.
- `ghc-debug` "reachable on a laptop today" is asserted from graphql-engine's
  wiring (cabal:281,1065 and Main.hs:169-177) but I did not run the ghc-debug
  client against a socket, because `ghc-debug` is not installed here.
- `polysemy` "appears nowhere in the corpus" is a negative claim bounded by
  the 10 reference projects; another corpus could find it.

## What I could not do

Full list in REPORT.md. Summary: katip's API-friction demo failed against
this katip snapshot (no `WithJSON` constructor and `registerScribe`/`initLogEnv`
signatures differ from the README), so no katip JSON line is shown; co-log
(used by HLS) provides the verbatim log line instead. katip still builds
fine on 9.14.1, which was the finding.
