# L4 hs idioms: REPORT

## Base proof

```
$ git merge --ff-only a7108169
Already up to date.
```

## Corpus

All cloned into `/tmp/hs-refs/` with `git clone --depth 1`, one commit each.
Line counts are `*.hs` files excluding `dist-newstyle`/build artifacts.

| project | commit | hs files | hs lines |
|---|---|---|---|
| postgrest | f84c44dafa | 123 | 28,614 |
| graphql-engine | 742ffcf93c | 1,118 | 267,870 |
| haskell-language-server | 82a7d796fe | 1,324 | 82,767 |
| servant | 866e272feb | 217 | 28,498 |
| rio | b9b08cb34c | 86 | 8,197 |
| katip | d8fcaf5403 | 28 | 6,627 |
| co-log | 85490e9ad3 | 13 | 1,415 |
| hs-opentelemetry | 5a267e6909 | 284 | 100,050 |
| effectful | 0d8eef128f | 108 | 17,038 |
| safe-exceptions | c020dde882 | 4 | 830 |

GHC 9.14.1, cabal 3.16.1.0 at /opt/homebrew/bin.

## Counted, not asserted

**Logger imports per project** (`grep -rl "import.*<Pat>" <proj> --include='*.hs'`):

| logger | graphql | HLS | postgrest | rio | katip | co-log | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|
| Katip | - | - | - | - | 24 | - | 2 | - |
| Colog (co-log) | - | 3 | - | - | - | 12 | 2 | - |
| System.Log.FastLogger | 5 | - | 1 | 1 | - | - | 1 | - |
| Control.Monad.Logger | - | - | - | 1 | - | - | 4 | - |

**App monad types** (all `ReaderT`/`Env -> IO` under the hood):

| project | type | file:line |
|---|---|---|
| graphql-engine | AppM = ReaderT AppEnv (TraceT IO) | App.hs:672 |
| HLS | IdeAction = ReaderT ShakeExtras IO | Shake.hs:1081 |
| rio | RIO = ReaderT env IO | rio/src/RIO/Prelude/RIO.hs:38 |
| effectful | Eff = Env es -> IO a | Effectful/Internal/Monad.hs:126 |
| servant | Handler = ExceptT ServerError IO | Handler.hs:20 |
| postgrest | ExceptT + explicit AppState | App.hs:160, MainTx.hs:54 |

**Error library imports per project:**

| library | graphql | HLS | postgrest | servant | rio | katip | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|
| Control.Exception.Safe | 19 | 12 | - | - | - | 9 | - | 1 |
| Control.Exception (base) | 44 | 42 | via Protolude | 17 | 1 | 7 | 49 | 12 |
| Control.Monad.Catch | 18 | 3 | - | 7 | 3 | 1 | 2 | 5 |

postgrest: 59 source files import Protolude (which re-exports Control.Exception).

**Testing imports per project:**

| lib | graphql | HLS | postgrest | servant | rio | katip | hs-otel | effectful |
|---|---|---|---|---|---|---|---|---|
| hspec | 298 | 3 | 60 | 44 | 12 | - | 69 | - |
| tasty | 2 | 43 | - | 2 | - | 9 | 5 | 23 |
| QuickCheck | 12 | 10 | - | 17 | 1 | 2 | 2 | - |
| hedgehog | 31 | - | - | - | - | 1 | 1 | - |
| golden | - | 2 | - | 1 | - | 1 | - | - |

**Tooling:** cabal is the CI default even where `stack.yaml` survives (HLS,
servant, postgrest, co-log run cabal in CI). hpack only in rio and
hs-opentelemetry. `fourmolu` is the explicit formatter (servant,
hs-opentelemetry); `ormolu` appears only as an HLS plugin name. hlint pinned
in graphql-engine, HLS, servant, rio, hs-otel, co-log. Full table in
IDIOMS.md section 8.

## Starter output

`labs/hs-idioms/starter` runs in ~0.4 s; 78-line `src/Main.hs` wires
`ReaderT Env IO` (section 1), a co-log `LogAction` logged through the env
(section 2), the Either/bracket/throwIO error discipline of postgrest and
safe-exceptions (section 4), and `+RTS -s` (section 6).

Its own log lines, verbatim:

```
starting
config=port=8080
workers=4

app started on port 8080
```

Its `+RTS -s` block, verbatim:

```
          87,328 bytes allocated in the heap
           3,976 bytes copied during GC
          53,240 bytes maximum residency (1 sample(s))
          28,680 bytes maximum slop
               6 MiB total memory in use (0 MiB lost due to fragmentation)

                                     Tot time (elapsed)  Avg pause  Max pause
  Gen  0         0 colls,     0 par    0.000s   0.000s     0.0000s    0.0000s
  Gen  1         1 colls,     0 par    0.000s   0.003s     0.0025s    0.0025s

  INIT    time    0.002s  (  0.003s elapsed)
  MUT     time    0.002s  (  0.008s elapsed)
  GC      time    0.000s  (  0.003s elapsed)
  EXIT    time    0.001s  (  0.010s elapsed)
  Total   time    0.005s  (  0.023s elapsed)

  %GC     time       0.0%  (0.0% elapsed)

  Alloc rate    52,766,163 bytes per MUT second

  Productivity  34.0% of total user, 33.6% of total elapsed

```

## The two memory numbers

`probes/memprobe`, `n=600000`, default RTS, both tools on the same run.

| quantity | bytes | MiB | source |
|---|---|---|---|
| bytes allocated in the heap (volume) | 48,452,392 | 46.2 | `+RTS -s` |
| maximum residency | 18,366,456 | 17.5 | `+RTS -s` / `-t` `max_bytes_used` |
| total memory in use (RTS) | 46,137,344 | 44.0 | `-t` `max_mem_in_use_bytes` |
| /usr/bin/time -l maximum resident set size | 51,396,608 | 49.0 | `/usr/bin/time -l` |
| /usr/bin/time -l peak memory footprint | 49,038,400 | 46.8 | `/usr/bin/time -l` |

Gap: maxrss (49.0 MiB) is the whole OS process, GHC's `max_mem_in_use` (44.0
MiB) is the RTS-managed heap only. The ~5 MB difference is the binary, shared
libraries, RTS-reserved-not-live address space, and resident non-heap pages.
Maximum residency (17.5 MiB) is neither: it is only the live Haskell heap at
GC samples. `bytes allocated` (46.2 MiB volume) is cumulative allocation
throughput, not a peak. Three different numbers; a harness reads all three and
labels them.

Bench-harness record (`+RTS -t --machine-readable`, n=600000, field names on
GHC 9.14.1 verbatim):

```
 [("bytes allocated", "48452392")
 ,("num_GCs", "12")
 ,("max_bytes_used", "18366456")
 ,("peak_megabytes_allocated", "44")
 ,("total_cpu_seconds", "0.033238")
 ,("total_wall_seconds", "0.048515")
 ,("max_mem_in_use_bytes", "46137344")
```

GC knobs a benchmark must pin, measured (n=600000):

| RTS | bytes allocated | max residency | total memory in use |
|---|---|---|---|
| default | 48,452,392 | 18,366,456 | 44 MiB |
| -A32m | 48,419,384 | 44,328 | 68 MiB |
| -M16M | 21,009,712 | 13,123,576 | 21 MiB |
| -H64M | 48,452,392 | 12,206,200 | 61 MiB |

Pin `-A` at minimum (it collapses residency by moving all allocation into the
nursery), and `-M`/`-H` if used, recording the value with each run.

## Unproven

Listed in full in IDIOMS.md section 9: `-xc` not run as a triggered error,
`eventlog2html` not installed so not run, `threadscope` not available,
hs-opentelemetry wire-export path not stood up, ghc-debug client not run
against a socket (only graphql-engine's wiring cited), polysemy absence
bounded to the 10-project corpus.

## What I could not do

- katip JSON log-line probe: this katip snapshot (0.8.8.4) has no `WithJSON`
  constructor and its `registerScribe`/`initLogEnv`/`mkHandleScribe`
  signatures differ from the README, so no katip JSON line is in the report.
  katip still builds clean on 9.14.1 (compiled `katip-0.8.8.4`, ~9 s), which
  is the finding. co-log (used by HLS) supplies the verbatim structured line
  instead.
- `eventlog2html`, `threadscope`, and the `ghc-debug` client are not installed
  in this environment, so the render/visualize/inspect steps were not run; the
  raw `-l` eventlog and `-hc`/`-hy`/`-hT` heap profiles were produced.
- Did not stand up an OTel collector to exercise the hs-opentelemetry wire
  export path.
