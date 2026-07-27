# SWI-Prolog tooling, embedding, interop (2026-07-27)

Scope: tooling/embedding/interop only. Local install verified: SWI-Prolog 10.0.2,
arm64-darwin, homebrew (`swi-prolog 10.0.2 (1,520 files, 29.2MB)`).

## Verdict: swipl-wasm in node: YES, works, cheap

`swipl-wasm` on npm runs SWI-Prolog under node with no browser, no DOM shim.
Tested in `/tmp/swipl-wasm-test` (throwaway, not in repo).

Receipt (`pnpm add swipl-wasm`, version resolved 8.0.4, released 2026-07-14):

```
$ node script.mjs
init_ms: 73
query_ms: 0
double(21, Y) => Y = 42

$ time node script.mjs
real 0m0.176s   user 0m0.284s   sys 0m0.024s
```

Script consulted a real `.pl` file off disk into the Emscripten virtual FS
(`swipl.FS.writeFile`) then called `consult/1` then ran a goal:

```js
import SWIPL from "swipl-wasm";
const swipl = await SWIPL({ arguments: ["-q"] });
swipl.FS.writeFile("/fixture.pl", source);
swipl.prolog.query("consult('/fixture.pl').").once();
const result = swipl.prolog.query("double(21, Y).").once();
```

Numbers:
- init (module instantiate + boot image load): ~73ms
- per-query overhead: ~0ms once booted
- full process wall clock including node startup: 176ms
- install footprint: `dist/swipl/swipl.js` (192KB) + `swipl-web.wasm` (2.1MB) +
  `swipl-web.data` (1.6MB) for the node path; there is also a single-file
  browser bundle (`swipl-bundle.js`, 6.0MB, `.wasm`/`.data` inlined) that the
  node entry point does not use
- `du -sh node_modules/swipl-wasm` reports 0 (pnpm content-addressed store;
  the real bytes above are the accurate figure)

Maintenance: `SWI-Prolog/npm-swipl-wasm` on GitHub, last push **today**
(2026-07-27, `fix: update to emsdk v6.0.4 #1217`), 61 stars, 21 open issues,
73 published npm versions, releases roughly weekly (dependabot-driven plus
real fixes). This is not a stale side project.

Two quirks worth flagging before anyone wires the conformance oracle to this:

1. **Iterator protocol is non-standard on the final solution.** For a
   `member(X,[a,b,c])`-shaped query, `.next()` returns
   `{done:false, X:"a"}`, `{done:false, X:"b"}`, `{done:true, X:"c"}` (value
   populated on the SAME call where `done` flips true), then a 4th call
   returns `{done:true}` with no value. A plain `while (!sol.done)` loop
   silently drops the last row. Any replay harness must check `value`
   before checking `done`, not the other way round.
2. **Unknown-procedure errors do not throw a catchable JS exception.**
   `swipl.prolog.query("undefined_pred(1).").once()` prints
   `wasm:wasm_call_string/3: Unknown procedure: ...` straight to stdout
   (Prolog's own `print_message`) AND returns `{error:true, message:...}`
   as a normal return value, not a thrown error. A fixture harness diffing
   stdout against expected output will see this line unless it is filtered.

Threads are not available inside WASM (confirmed against the SWI-Prolog
WASM discourse thread, "SWI-Prolog in the browser using WASM"); the engine
uses cooperative `async` Engines instead. Irrelevant here since the fixture
replay is single-query, single-threaded already.

**Relevance to sprefa v6**: this closes the loop asked for. The js
conformance leg (`v6/dl`, `v6/sprefa-store/js`) could load the real
`v6/prolog/conformance/*.pl` oracle files into a `swipl-wasm` instance under
node and query the SAME fixtures both the TS engine and the Rust port will
eventually replay, instead of hand-porting the oracle's logic into TS. At
73ms cold-boot and ~0ms per query this is a non-issue at any test-suite size
sprefa runs today (conformance dirs are dozens of fixtures, not thousands).

## janus (Python bridge) and node/JS equivalents

`library(janus)` ships bundled in the 10.0.2 homebrew binary
(`library/ext/swipy/janus.pl`) but **failed to load on this machine**:

```
$ swipl -g "use_module(library(janus))"
ERROR: open_shared_object/3: dlopen(.../janus.so, ...):
  Library not loaded: @rpath/Python3.framework/Versions/3.9/Python3
  Referenced from: janus.so
  Reason: tried: .../Xcode.app/.../Python3.framework/Versions/3.9/Python3 (no such file) [...]
```

The homebrew build links against the Xcode-bundled Python 3.9 framework,
which is not present on this machine's Xcode install. Janus is real and
bundled, but "bundled" does not mean "works out of the box" on macOS; it
needs a matching Python framework at a specific path. This is a Python<->
Prolog bridge only; it has nothing to do with node/JS.

No janus-shaped bridge exists for node using the SWI C API the way janus
uses Python's. The JS-facing equivalent already lives one layer up: the
WASM build's own `js_call_string`/`query()` surface IS the thing janus's
Python docs say they modeled themselves after ("the Python interface is
modeled after the recent JavaScript interface developed for the WASM
version", swi-prolog.org janus docs). So for sprefa's purposes,
swipl-wasm's own query API already is "janus for node"; no separate bridge
package is needed or exists.

Other node bridges found, none embed the C API the way janus does:
- `rla/node-swipl-stdio`: spawns real `swipl` as a subprocess, talks over
  stdio. No native compile, no env vars, works against ANY installed swipl
  including 10.0.2, but pays process-spawn cost per session and needs a
  real swipl binary on the host (not usable inside a browser or a
  pure-npm CI runner without SWI-Prolog installed).
- `rla/node-swipl`: native (N-API/FFI-ish) binding to `libswipl`, unverified
  against 10.x here, higher install friction (needs libswipl at build time).
- `kloni/node-prolog-swi`: same shape, unverified, looked less active in
  search results than the two above.

None of these were installed/tested (out of scope once swipl-wasm proved
out; stdio-and-native bridges duplicate what the WASM build gives for free
plus a pnpm install).

## Rust embedding: swipl-rs

`crates.io/swipl` (wraps `swipl-fli`, workspace also has `swipl-macros`,
`cargo-swipl`, `swipl-info`), from `terminusdb-labs/swipl-rs`:

```
$ gh api repos/terminusdb-labs/swipl-rs --jq '{pushed_at, open_issues_count, archived, stargazers_count}'
{"pushed_at":"2024-02-20T10:21:53Z","open_issues_count":17,"archived":false,"stargazers_count":34}

$ curl crates.io/api/v1/crates/swipl
max_version 0.3.16, updated_at 2024-02-20, downloads 44049 (1034 recent)
```

Last commit **2024-02-20**, targeting "Swipl API updates 9.1.19" per its own
commit log, over 2 years stale relative to today (2026-07-27) and pinned to
a pre-9.2/pre-10.0 API surface. Not archived, but not moving; 17 open
issues sitting unaddressed since that date. High-level, safe wrapper design
is sound (procedural macros generate foreign-predicate glue, no manual
`unsafe`), but it is a "verify carefully before depending on it against
10.0.2" situation, not a "just cargo add it" situation. An older
`remexre/swipl-rs` crate (unrelated codebase, same name idea) exists and
looked less maintained still; not investigated further.

**Relevance to sprefa v6**: the planned Rust port has no ready-made,
actively-maintained swipl embedding crate to lean on. If the Rust leg ever
wants to call the Prolog oracle directly (rather than re-implementing its
logic, which is presumably the actual plan given the oracle's role), it
would mean either reviving/forking swipl-rs against 10.x's current C API or
shelling out to `swipl` as a subprocess (same stdio shape as the node
option above). No blocker today since the oracle role is comparison, not
runtime dependency.

## Everything else

| tool | status | verified | relevance to sprefa v6 |
|---|---|---|---|
| `qsave_program` single-file exe | works | yes, built `hello_exe` locally, Mach-O arm64, 613KB, `otool -L` shows only `libswipl.10.dylib` + `libSystem`, ran and printed argv, exit 0 | could ship the prolog oracle as a standalone binary for CI without a system swipl install; not needed once swipl-wasm is the chosen path |
| `swipl -g`/exit codes | stable | yes, `-g fail` exits 1, `-g` uncaught arithmetic error exits 2, normal success exits 0 | oracle fixture runners can shell out and trust exit code to distinguish failure-as-data vs error-as-bug |
| C FFI (9.x/10.x) | incremental, not a rewrite | changelog read, not locally exercised (no C embedding attempted) | `PL_new_term_refs()` now takes `size_t` not `int`, `PL_free_blob()` added, `PL_call()` now propagates exceptions, `SWI-cpp2.h` (C++ v2) is the maintained header, `SWI-cpp.h` (v1) is explicitly frozen/deprecated. Only matters if the Rust port ever binds the raw C API instead of using/forking swipl-rs |
| pack manager (`pack_install/1`) | works, low friction | yes, `pack_install(lsp_server, [interactive(false)])` completed in a few seconds, zero compiler steps (pure-Prolog pack); a C-backed pack already on this machine (`prosqlite`) ships prebuilt `arm64-darwin` `.so` rather than compiling on install | trivial to pull in extra tooling packs (lsp_server, coverage helpers) for oracle dev without touching the repo's Rust/TS build |
| `lsp_server` pack (jamesnvc/lsp_server) | active, "work in progress" | not exercised as an editor session, only installed+removed to confirm `pack_install` ergonomics | closest real LSP for VS Code: diagnostics (singleton vars, xref syntax errors), hover docs, go-to-def/references, rename, formatter. LogicMoo-sponsored. Best current option if the prolog oracle files ever need real IDE support beyond syntax highlighting |
| `vsc-prolog` (arthurwang) | stale | not installed | last marketplace update Dec 2018, 223K installs is legacy inertia, not currency; do not use as the basis for new tooling decisions |
| `new-vsc-prolog` (AmauryRabouan) | active | not installed | marketplace shows v1.1.15, updated 2026-06-28 (this month); syntax highlighting + linting + experimental debugger, but README states no bundled LSP server; pairs with lsp_server pack rather than replacing it |
| `sweep` (Emacs) | exists, not relevant here | not verified (Emacs-only per its own docs, user edits in VS Code) | out of scope, no action |
| `library(prolog_profile)` | bundled, unchanged shape | loaded cleanly locally (`use_module(library(prolog_coverage))` succeeded) | standard `profile/1` call-count/time profiler; usable as-is if the oracle's own perf ever needs a look |
| `library(prolog_coverage)` | bundled | loaded cleanly locally | line-annotated coverage via `show_coverage/1`, shares source-location plumbing with the source-level debugger; could validate fixture coverage of the oracle's rule set |
| source-level debugger | mature, no dramatic 10.x change found | not exercised interactively | graphical + source-level debugger unchanged in kind between 9.x and 10.x per changelog skim; nothing new enough to change a tooling decision here |
| janus (Python bridge) | bundled, broken here | see receipt above (dlopen failure, missing Python3.framework 3.9) | irrelevant to the node/TS/Rust legs directly; relevant only as the acknowledged design ancestor of swipl-wasm's own JS query API |

## Top 5 by payoff

1. **swipl-wasm in node for the js conformance leg.** Proven working today
   against real `.pl` consult + goal execution, 73ms boot, actively
   maintained (commits same day as this research). Directly answers the
   "dream shortcut" question: yes, buildable now.
2. **Guard the two swipl-wasm quirks before wiring fixtures.** The
   done-true-with-value iterator shape and the non-throwing
   unknown-procedure error are exactly the kind of silent mismatch that
   would produce a false "js conformance leg agrees with oracle" result.
   Cheap to guard, expensive to discover after the fact.
3. **`lsp_server` pack + `new-vsc-prolog` extension for editing the oracle
   files in VS Code.** Both current, `pack_install` is a one-line
   zero-build call. Neither was previously known to be live-maintained in
   2026; worth adopting over hand-written syntax rules.
4. **Do not plan on swipl-rs for the Rust port without a revival decision
   first.** 2+ years stale against a moving 9.x/10.x C API; treat "call the
   oracle from Rust via FFI" as a fork-and-fix cost, not a dependency add,
   or fall back to subprocess/stdio like the node bridges do.
5. **`qsave_program` gives a zero-dependency way to ship the oracle as a
   binary for CI machines without a system swipl.** Low priority relative
   to WASM-in-node, but a real fallback if a CI runner cannot install
   SWI-Prolog itself.
