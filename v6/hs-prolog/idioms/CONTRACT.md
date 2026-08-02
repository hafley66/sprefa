# Lane L4: what professional Haskell actually does for logs, tracing, and debugging

## Question this lane answers

If this repo grew a Haskell component, what would a working Haskell team already
expect to see in it: which logging library, which tracing story, how errors are
thrown and caught, how a running process is debugged, how a heap is profiled,
and what the module layout looks like. Answer from what REAL open source
projects do, with `file:line` from their actual source, not from what the
libraries advertise about themselves.

## Base

First action, from the worktree root:

    git merge --ff-only a7108169

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not
work around it.

## Ownership

You own `labs/hs-idioms/**` and `REPORT.md` at the worktree root. Nothing else in
this repo. Three sibling lanes are running in other worktrees.

Clone reference projects OUTSIDE the repo, under `/tmp/hs-refs/`, shallow
(`git clone --depth 1`). Never clone into the worktree.

## The evidence rule, which is the point of this lane

Every convention claim carries ONE of:

- `repo/path/File.hs:LINE` from a project you actually cloned into `/tmp/hs-refs/`,
  quoted, plus the command that found it (`rg` line included), or
- a compiled probe under `labs/hs-idioms/probes/` that runs.

A claim with neither goes in a section called `Unproven` at the end of the
document. That section existing with honest content is a better result than a
confident table that no compiler and no repo backs. This is measured: the
coordinator will spot-check citations by opening the cloned file at that line.

## Reference projects to clone

Start here, add or drop with a written reason:

    postgrest/postgrest              a production HTTP server, Warp based
    hasura/graphql-engine            large commercial Haskell server
    haskell/haskell-language-server  long-running process, LSP, heavy tracing
    haskell-servant/servant          the API layer most servers sit on
    commercialhaskell/rio            the RIO pattern, snoyberg's house style
    Soostone/katip                   structured logging
    kowainik/co-log                  the other structured logging school
    iand675/hs-opentelemetry         OpenTelemetry for Haskell
    haskell-effectful/effectful      the current effects contender
    fpco/safe-exceptions             exception discipline

Count, do not assert. For "which logger do people use", grep the cloned corpus
for the import and report the counts per project. A number beats an opinion.

## Deliverable

`labs/hs-idioms/IDIOMS.md`, a probes cabal project at `labs/hs-idioms/probes/`,
a wired starter at `labs/hs-idioms/starter/`, and `REPORT.md` at the worktree root.

### IDIOMS.md sections, in this order

1. **The application monad.** `ReaderT Env IO` vs `RIO` vs `effectful` vs
   `mtl` stacks vs `polysemy`. What each cloned project actually uses, with the
   type at `file:line`. Say which one a new 2026 project starts with and why,
   naming the projects that back the answer.
2. **Logging.** `katip` vs `co-log` vs `fast-logger` vs `monad-logger` vs plain
   `hPutStrLn`. Structured or not, how the logger is threaded (implicit through
   the env, or passed), how log context is scoped, how levels are set at runtime.
   Include what a real log line looks like from the running probe, verbatim.
3. **Tracing.** `hs-opentelemetry`, span creation, propagation across a request,
   what the instrumentation of a Warp/servant handler looks like in practice, and
   whether the ecosystem answer is mature enough to depend on. Cite HLS's own
   tracing if it uses something else.
4. **Errors and exceptions.** `safe-exceptions` vs `Control.Exception` vs
   `ExceptT` in the app monad, `bracket` discipline, `HasCallStack`, when a
   library returns `Either` and when it throws. Show the rule the reference
   projects follow, not the one the tutorials teach.
5. **Debugging a running process.** `Debug.Trace` and why it survives, `ghci`
   breakpoints, `-xc` stack traces, `HasCallStack` annotations, `ghc-debug`,
   `threadscope`, the eventlog. What is actually reachable on a laptop today.
6. **Profiling: time, allocation, and peak memory.** This section has a customer
   in this repo, so it must be RUNNABLE, not a survey:
   - `+RTS -s` and what each line means: `bytes allocated in the heap` is TOTAL
     allocation volume over the run, `maximum residency` is live heap at GC
     sample points, `total memory in use` is what the RTS took from the OS.
     These are three different numbers and conflating them is the failure mode.
   - `+RTS -t --machine-readable` for a parseable one-line record, which is what
     a bench harness wants. Show the actual field names it emits on GHC 9.14.
   - How `maximum residency` relates to `/usr/bin/time -l`'s `maximum resident
     set size`, and why they disagree. Measure both on one probe and report the
     two numbers side by side with the gap explained.
   - Heap profiling without a profiling build: `-hT`. With one: `-hc`, `-hy`,
     and `eventlog2html`.
   - What `-rtsopts`, `-threaded`, and `-with-rtsopts` cost and when each is set.
   - GC knobs that change the numbers: `-A`, `-M`, and idle GC. Say which of
     these a benchmark must PIN so runs are comparable.
7. **Testing.** `hspec` vs `tasty`, `QuickCheck` vs `hedgehog`, golden tests,
   what a `test-suite` stanza looks like in the cloned projects.
8. **Project layout and tooling.** cabal vs stack in 2026, `hlint`, `fourmolu`
   or `ormolu`, `weeder`, `.cabal` vs `hpack`, CI shape. Report which the cloned
   projects use, counted.
9. **Unproven.** Everything you could not back.

### The starter

`labs/hs-idioms/starter/` is a single small program, under 200 lines, that wires
the recommendations from sections 1, 2, 4, and 6 together and RUNS: an app monad,
a structured logger emitting real lines, one deliberate error path caught the way
the reference projects catch it, and `+RTS -s` output captured in REPORT.md. It
does not need to be a server. It needs to compile and run under 10 seconds.

## Build-vs-buy is a repo law

Where your answer is "write our own", first name the Hackage candidates you
checked and why each does not fit. No one-line dismissals. That law is why this
lane exists at all: the point is to buy the ecosystem's answer, not invent one.

## Toolchain

`ghc` 9.14.1 and `cabal` 3.16.1.0 at `/opt/homebrew/bin`, Hackage index fresh.
Note the GHC version when a library does not have a compatible bound: "does not
build on 9.14" is a first-class finding and belongs in the report, not in a
silent workaround.

## Style laws (repo-wide, enforced)

- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Comments state only constraints the code cannot show.
- Tables and file:line over prose. Under-word everything.

## REPORT.md format

    # L4 hs idioms: REPORT
    ## Base proof
    <git merge --ff-only output, verbatim>
    ## Corpus
    <every project cloned, its commit sha, its line count>
    ## Counted, not asserted
    <the import-count tables that decided sections 1, 2, 8>
    ## Starter output
    <the program's own log lines, and its +RTS -s block, verbatim>
    ## The two memory numbers
    <maximum residency vs /usr/bin/time -l maximum resident set size, one probe,
     both numbers, the gap explained>
    ## Unproven
    <every claim with no citation and no probe>
    ## What I could not do
