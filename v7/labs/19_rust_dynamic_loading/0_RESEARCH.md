# DL7 generated Rust dynamic loading and generation-boundary HMR

Research cutoff: 2026-09-04. Runtime code was not changed by this lab.

This document separates four questions that were previously collapsed:

1. Can checked DL7 generate native Rust code?
2. What binary interface lets a resident host call that code?
3. At what point may one loaded generation replace another?
4. What does each compile, link, load, and swap path cost on the same fixture?

The current repository answers the first question for native plan constructors.
The other three need an executable receipt.

## 1. Evidence classes

| class | meaning |
|---|---|
| current repository | source or test present in this checkout on 2026-09-04 |
| historical repository | committed measurement from an earlier V6 experiment |
| upstream contract | current project documentation, language reference, OS manual, or paper |
| benchmark arm | proposed local measurement; no number is claimed until the JSONL receipt exists |

Crate build durations shown by docs.rs describe docs.rs infrastructure. They do
not predict this repository's build time and are excluded from the benchmark
results.

## 2. Repository baseline

### 2.1 V6 receipt

Historical commit `beb8e55b3` added
`plans/2026-08-12-rust-dl6-reload.RESEARCH.md`. Its temporary harness measured
the following on macOS 14.6.1, Apple M2 Pro, and
`rustc 1.97.0-nightly (9eb3be26b 2026-05-18)`:

| operation | samples in the historical receipt | reported result |
|---|---|---:|
| small generated source to `cdylib` | 1.98 s, 2.02 s, 2.01 s | 2.00 s mean |
| large generated source to `cdylib`, after a temporary raw-string delimiter repair | 2.04 s, 2.08 s, 2.09 s | 2.07 s mean |
| warm `dlopen` of the repaired large library | 0.334 ms, 0.335 ms after a 161.734 ms cold sample | 0.334 to 0.335 ms warm |
| first call to `program()` | 5.646 ms, 3.476 ms, 4.146 ms | 3.476 to 5.646 ms |
| warm process spawn plus SQLite open/query/close | 2.214 ms, 1.734 ms after a 117.449 ms cold sample | 1.734 to 2.214 ms warm |

That library still carried `PROGRAM_JSON`. Current V6 source emits the string
and a `program()` decoder in
[`v6/prolog/emit_rust.pl`](../../../v6/prolog/emit_rust.pl#L685), while the
resident engine reads the generated `.rs` file as text in
[`v6/sprefa-engine-rs/src/run.rs`](../../../v6/sprefa-engine-rs/src/run.rs#L38).
The receipt therefore measured compilation and OS loading around a serialized
plan. The measured first call decoded that serialized plan; generated tick code
had zero implementation in the loaded artifact.

A later V6 execution shootout corrected a separate harness issue. The committed
session record reports 0.77 to 0.95 s cold builds and 0.27 to 0.46 s warm
rebuilds for compiler-emitted monomorphic Rust in
[`chat_log/20260812.3.emit-rust-ir-280-to-285-lane-forensics-claudemd-rewrite.md`](../../../chat_log/20260812.3.emit-rust-ir-280-to-285-lane-forensics-claudemd-rewrite.md).
Those numbers used a different generated program and harness. They are input to
fixture design. Section 10 defines the DL7 matrix.

### 2.2 Current DL7 Rust and SQLite paths

The current DBSP Rust emitter generates direct native plan constructors in
[`v7/src/3_emit/1b_dbsp_rust_emitter.pl`](../../src/3_emit/1b_dbsp_rust_emitter.pl#L21).
Its test explicitly rejects `PROGRAM_JSON` and `from_str` in
[`v7/test/11_dbsp_rust_emitter.test.pl`](../../test/11_dbsp_rust_emitter.test.pl#L8).
Those generated source files are currently modules of their consuming crate, so
Cargo compiles and links them when that crate builds. This proves native source
generation and static inclusion. It provides no separately loadable artifact.

The same checked DL7 program emits SQLite DDL/rules and an ordered tick in
[`v7/src/3_emit/1a_dbsp_plan_emitter.pl`](../../src/3_emit/1a_dbsp_plan_emitter.pl#L9).
The SQLite integration executes that generated tick in
[`v7/test/13_sqlite_plan.e2e.pl`](../../test/13_sqlite_plan.e2e.pl#L8).

Persistent SQLite state and `__dl7_catalog` behavior are recorded in
[`v7/README.md`](../../README.md#L103). A changed plan is rejected before
retained rows are modified. Catalog migration remains unimplemented. Generated
Rust and generated SQLite SQL already share a checked source, while dynamic
loading and generation replacement are absent from the current runtime.

The current generated constructor signatures use Rust `Vec`, `String`, and
`dd_runner` structs. Exporting those signatures directly from a separately
compiled library would use Rust layouts and allocation ownership across the
boundary. The Rust Reference states that the Rust ABI has no stability
guarantee, while `extern "C"` follows the target's C ABI. The dynamically loaded
entry must therefore have a separately defined ABI or use a crate that defines
one. [Rust Reference: external ABIs](https://doc.rust-lang.org/reference/items/external-blocks.html#abi)

### 2.3 Boundary names

The benchmark and loader use a three-level vocabulary:

| name | measurement or transition |
|---|---|
| `Build.Compile.Cold` | compiler plus linker in a fresh target directory |
| `Build.Compile.Warm` | compiler plus linker with the same target directory |
| `Build.Link.Wall` | one linker-driver invocation measured by a wrapper |
| `Load.Open.Wall` | `Library::new` or the comparison-arm equivalent |
| `Load.Symbol.Wall` | root symbol lookup and descriptor validation |
| `Call.First.Wall` | first generation function call after validation |
| `Swap.Validate.Wall` | ABI, capability, schema, and catalog checks |
| `Swap.Publish.Wall` | atomic active-generation pointer replacement |
| `Swap.Retire.Wall` | drain, shutdown, and optional unload of the old generation |
| `Memory.Rss.Peak` | peak resident bytes for the measured process set |
| `Artifact.File.Bytes` | artifact bytes on disk |

The term generation means one committed outside-world tick boundary. Internal
recursive rounds use a different clock. Current design states that a tick may
publish only after recursive groups stabilize in
[`v7/design/5_REACTIVE_FIXPOINT_MARBLES.md`](../../design/5_REACTIVE_FIXPOINT_MARBLES.md#L213).

## 3. Native loader and OS contracts

### 3.1 `libloading`

[`libloading` 0.9.0, released 2025-11-05](https://docs.rs/crate/libloading/latest),
wraps the platform loader and binds each `Symbol` lifetime to its `Library`.
It leaves platform behavior visible. `Library::new` is unsafe because library
initializers and finalizers execute. `Library::get` receives an exact symbol
name, performs no mangling, and requires the caller to supply the correct type.
`Library::close` can be a platform-dependent no-op and `Drop` ignores close
errors. [Library API](https://docs.rs/libloading/latest/libloading/struct.Library.html)

On Unix, the cross-platform `Library::new` currently maps to
`RTLD_LAZY | RTLD_LOCAL`. Symbol type mismatch is undefined behavior.
[Unix API](https://docs.rs/libloading/latest/libloading/os/unix/struct.Library.html)

The loader should use one absolute path per immutable artifact and one root
symbol containing an ABI-major suffix. Search-path loading adds platform state
that is unrelated to DL7 module identity.

### 3.2 Linux

Linux `dlopen` returns the same handle when the same shared object is opened
again and maintains a reference count. `dlclose` unloads only after the count
reaches zero and no other object requires its symbols. A successful `dlclose`
does not guarantee that the symbols left the address space. `RTLD_NODELETE`
retains code and globals. Constructors run before `dlopen` returns and
destructors run before `dlclose` returns. [Linux man-pages 6.18,
`dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html)

Linux/glibc also exposes `dlmopen` namespaces. The current manual documents a
maximum of 16 namespaces and several flag constraints. This is a Linux-only
comparison arm for repeated loading or symbol isolation.

### 3.3 macOS

Apple's archived manual documents same-path handle reuse, reference counting,
initializers before return, and `RTLD_NODELETE`. The page is dated 2006, so the
benchmark must record observed behavior on the tested macOS and dyld version.
[Apple archived `dlopen(3)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/dlopen.3.html)

`hot-lib-reloader` documents an additional macOS requirement: the reloadable
copy is code-signed using Xcode command-line tools. The raw `libloading` arm
must separately record whether ad hoc signing is required for the generated
artifact on each tested host.

### 3.4 Windows

`LoadLibraryExW` loads a module into the process. A fully qualified path limits
the search for the requested module, while dependency search still follows the
selected flags. A pathless name can return the first loaded module with the same
base name. [Microsoft `LoadLibraryExW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw)

`FreeLibrary` decrements a per-process reference count and removes the module
after the count reaches zero, after `DllMain(DLL_PROCESS_DETACH)` returns.
Microsoft also documents a race when a thread unloads the DLL in which it is
executing. [Microsoft `FreeLibrary`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-freelibrary)

Unique immutable filenames avoid in-place replacement of a mapped DLL and
base-name aliasing. The Windows benchmark must include the full-path load and
old-artifact deletion after retirement.

## 4. ABI contract to benchmark

Rust produces a system dynamic library with `crate-type = ["cdylib"]` on
Linux, macOS, and Windows. `dylib` is a Rust dynamic dependency format;
`cdylib` is the system-library output intended for a foreign ABI boundary.
[Rust Reference: linkage](https://doc.rust-lang.org/reference/linkage.html)

The raw C ABI arm should export one symbol:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn sprefa_dl7_module_v1() -> *const SprefaDl7ModuleV1;
```

The root descriptor is a proposed prefix-extensible `#[repr(C)]` benchmark
table:

```rust
#[repr(C)]
pub struct SprefaDl7ModuleV1 {
    pub descriptor_bytes: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub abi_patch: u16,
    pub reserved: u16,
    pub capability_bits: u64,
    pub schema_digest: [u8; 32],
    pub program_digest: [u8; 32],
    pub build_digest: [u8; 32],
    pub open: unsafe extern "C" fn(*const HostV1, *mut ModuleHandle) -> Status,
    pub tick: unsafe extern "C" fn(ModuleHandle, Bytes, *mut OutputSinkV1) -> Status,
    pub shutdown: unsafe extern "C" fn(ModuleHandle) -> Status,
}
```

`Bytes`, `Status`, `ModuleHandle`, `HostV1`, and `OutputSinkV1` must themselves
contain fixed-width integers, pointers plus lengths, and `extern "C"` function
pointers with `#[repr(C)]`. Rust `bool`, Rust enums without an explicit
representation, references, slices, `String`, `Vec`, trait objects, and
generated plan structs stay behind the boundary in the raw arm.

The output sink lets each side free what it allocated. An alternate returned
buffer requires a matching plugin-owned `free` function. Panics are caught
inside each exported function and converted to `Status`. The Rust Reference
notes that runtime loading bypasses rustc's checks for inconsistent panic
strategies across separately linked artifacts. [Rust Reference: prohibited
linkage and unwinding](https://doc.rust-lang.org/reference/linkage.html#prohibited-linkage-and-unwinding)

### 4.1 Version and symbol negotiation

The host performs these checks before `open`:

1. Load the immutable full path using local symbol visibility where supported.
2. Resolve exactly `sprefa_dl7_module_v1`.
3. Read only fields within `descriptor_bytes`.
4. Require `abi_major == 1`.
5. Require `abi_minor` within the host's declared inclusive range.
6. Ignore unknown suffix fields and unknown optional capability bits.
7. Require every host-required capability bit.
8. Compare `schema_digest` and the SQLite catalog contract before state access.
9. Compare the manifest's `build_digest` with the descriptor and recompute the
   artifact file digest before publication.
10. Call `open`, then one fixture-specific validation call, before making the
    generation reachable by new ticks.

Patch version is diagnostic unless an explicit compatibility rule assigns it
meaning. ABI version, DL7 language version, generated-plan IR version, runtime
kernel version, and application version are separate fields.

## 5. Build versus dependency capability inventory

The table records supplied and application-owned capabilities. It does not rank
the rows.

| arm | verified release | supplied capability | application-owned work | upstream limit relevant to HMR |
|---|---|---|---|---|
| raw `libloading` plus C ABI | [`libloading` 0.9.0, 2025-11-05](https://docs.rs/crate/libloading/latest) | cross-platform open/get/close wrapper; `Symbol` lifetime tied to `Library` | descriptor layout; version negotiation; state handoff; drain; thread shutdown; allocation ownership; error protocol | platform close can be a no-op; incorrect symbol type is undefined behavior |
| `abi_stable` | [`abi_stable` 0.11.3, 2023-10-12](https://docs.rs/crate/abi_stable/latest) | load-time recursive layout checks; root modules; prefix types; FFI-safe std wrappers; trait objects; nonexhaustive enums; loading across Rust versions | generation drain; state compatibility; application capabilities and catalog checks | upstream explicitly excludes library unloading; current crate depends on `libloading ^0.7.3` |
| `stabby` | [`stabby` 72.1.16, 2026-07-20](https://docs.rs/crate/stabby/latest) | stable-layout types and compact sum types; exported signature reports or canaries; `libloading` extension with checked lookup | generation drain; state handoff; module lifecycle; application protocol | docs.rs failed to build 72.1.16; latest page warns that stable Rust 1.78+ trait-object vtables use a leaked global set with linear lookup under 100 entries and cloning on insertion |
| `safer_ffi` | stable [`0.1.13, 2024-09-17`](https://docs.rs/crate/safer-ffi/latest); prerelease `0.2.0-rc1`, 2026-01-16 | `#[derive_ReprC]`; `#[ffi_export]`; C header generation; fixed-layout wrappers for strings, vectors, boxes, slices, callbacks | dynamic loader; descriptor/version protocol; swap lifecycle; application compatibility checks | user guide labels the project alpha; 0.1.13 optionally depends on `stabby ^36.1.1` |
| `hot-lib-reloader` | [`0.8.2, 2025-08-11`](https://docs.rs/crate/hot-lib-reloader/latest) | watches build output; creates shadow copies with unique counters/UUIDs; wrapper generation; before/after reload observation | stable ABI; generation admission gate; catalog validation; state transfer | signature or shared type-layout changes can crash; generic reload functions unsupported; global state and `TypeId` need explicit handling; default debounce 500 ms |
| Subsecond | stable [`0.7.10, 2026-07-30`](https://docs.rs/crate/subsecond/latest); prerelease `0.8.0-alpha.1` same date | function jump table; patch application; Dioxus compiler/devtools protocol; ThinLink integration through Dioxus CLI | DL7 emitter integration; generation transaction; SQLite/catalog compatibility | experimental; patches tip crate only; struct changes require re-instancing/unwinding; statics are retained without destructor calls; tip-crate thread locals can reset; ThinLink unavailable standalone |

`abi_stable` provides its own interface-crate model and explicitly documents
plugin systems without unloading, plus load-time type checks and prefix
evolution. [abi_stable overview](https://docs.rs/abi_stable/latest/abi_stable/)

`stabby` can generate `<symbol>_stabbied` functions that compare type reports,
or canary symbols that include rustc, optimization level, and target details.
Its `StabbyLibrary` extension rejects missing canaries or report mismatches.
[stabby loader integration](https://docs.rs/stabby/latest/stabby/libloading/index.html)

`safer_ffi` generates C ABI exports and headers from `ReprC` types. Its public
documentation does not supply dynamic loading, semantic version negotiation,
or HMR lifecycle. [safer_ffi header generation](https://docs.rs/safer-ffi/latest/safer_ffi/headers/)

`hot-lib-reloader` supplies file watching and unique shadow-name mechanics on
top of `libloading`. Its before/after events can bracket a generation gate, but
the crate does not know DL7 tick completion or SQLite catalog compatibility.

Subsecond patches function calls through a jump table and requires a separate
compiler/protocol implementation. Its documented ThinLink is bundled through
the Dioxus CLI and is not available as a standalone linker.
[Subsecond current documentation](https://docs.rs/subsecond/latest/subsecond/)

## 6. Unload and retirement contract

`Library` lifetime prevents a Rust `Symbol` borrowed from it from outliving the
handle. HMR adds values that the type system cannot see: copied function
pointers, plugin-allocated objects, callbacks registered in the host, TLS,
threads, process-global registries, `atexit` handlers, and destructors in
dependencies.

Each loaded generation therefore owns an explicit runtime record:

```rust
struct Generation {
    library: Library,
    descriptor: SprefaDl7ModuleV1,
    handle: ModuleHandle,
    active_calls: AtomicUsize,
    retiring: AtomicBool,
    artifact_path: PathBuf,
}
```

Retirement preconditions:

1. The active dispatch pointer no longer references the generation.
2. `active_calls == 0` after an acquire/release drain.
3. The plugin's `shutdown` returned success.
4. Every plugin-started thread has joined.
5. Every host callback registration has been removed.
6. No plugin-owned allocation, handle, function pointer, or future remains in
   host state.
7. No call can re-enter the generation after the drain check.

Two policies must be benchmarked:

| policy | operation | observable result |
|---|---|---|
| `Swap.Retire.Pin` | retain every old `Library` until process exit | swap has no `dlclose`; RSS and loaded-image count can grow by generation |
| `Swap.Retire.Unload` | call `shutdown`, drain, then close the old `Library` | close latency and post-close RSS are measured; OS may retain mappings or symbols |

Pinning is required for the `abi_stable` arm because its upstream contract
excludes unloading. It is also a diagnostic arm for raw C ABI and `stabby`.
The raw unload arm needs stress tests that swap while concurrent callers enter
and leave the generation.

## 7. Compile, cache, and link variables

Cargo's dev profile defaults to incremental compilation with 256 codegen units.
The release profile defaults to no incremental compilation, optimization level
3, and 16 codegen units. Incremental state is only used for workspace members
and path dependencies. LTO adds link work. [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)

The first benchmark profile is fixed and checked into the future harness:

```toml
[profile.hmr]
inherits = "dev"
opt-level = 1
debug = "line-tables-only"
incremental = true
codegen-units = 256
lto = "off"
panic = "abort"
```

`profile.hmr` measures edit-to-load. A `profile.release` arm measures final
runtime code. Every receipt records the full resolved profile, `rustc -Vv`,
`cargo -V`, target triple, `Cargo.lock` digest, `RUSTFLAGS`, linker executable
and version, CPU, memory, OS, and filesystem.

Cargo stores intermediate incremental state under the build directory and
allows a per-run target directory. `cargo build --timings` supplies unit and
codegen timing. Compiler-internal concurrency and precise linker-invocation
timing require separate probes. [Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html),
[Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html)

The harness therefore wraps the linker driver. The wrapper records monotonic
start/end timestamps and argv, invokes the real driver, records exit status,
and writes one JSONL row. On macOS the driver can include work around `ld`; the
receipt labels it `Build.Link.DriverWall`.

### 7.1 Linker matrix

| platform | system arm | additional detected arms | verified upstream status |
|---|---|---|---|
| Linux `x86_64-unknown-linux-gnu` | rustc default | `rust-lld`, GNU BFD, mold, Wild when installed | Rust 1.90.0 made rust-lld the default for this target; Wild currently supports Linux shared objects and states that incremental linking remains an end goal |
| macOS `aarch64-apple-darwin` | Apple `ld` | `ld64.lld` when installed | LLD supports Mach-O after ELF and PE/COFF in its documented completeness order |
| Windows `x86_64-pc-windows-msvc` | `link.exe` | `lld-link` when installed | LLD documents DLL creation, import libraries, and export by name or ordinal |

The Rust project reported a 7x linker reduction and 40 percent end-to-end
incremental rebuild reduction for ripgrep 13 debug builds when moving its Linux
default to LLD. Those numbers describe that workload and establish a matrix
arm. [Rust 1.90 LLD report, 2025-09-01](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/)

The 2026 mold paper reports 2.4 to 16.1x over LLD and up to 112x over GNU ld on
its large-program suite. This lab measures the generated DL7 fixture instead of
transferring those ratios. [Ueyama, "mold: A Massively Parallel Linker",
2026-08-24](https://arxiv.org/abs/2608.23228)

Wild's author published an incremental-link design in 2024. Current project
documentation says shared-object output works and incremental linking remains
unimplemented. This keeps Wild in the detected full-link matrix and excludes a
claimed incremental-link result. [Wild README](https://github.com/wild-linker/wild),
[incremental-link design](https://davidlattimore.github.io/posts/2024/11/19/designing-wilds-incremental-linking.html)

The research host currently reports macOS 14.6.1,
`aarch64-apple-darwin`, `rustc 1.100.0-nightly (17fd5b8a3 2026-08-28)`, LLVM
23.1.0, Cargo 1.100.0-nightly, Apple ld 1115.7.3, and an installed
`/opt/homebrew/bin/ld64.lld`. No benchmark numbers were collected by this
document.

## 8. Content-addressed artifacts

Cargo fingerprints answer whether one Cargo unit is fresh. Their layout is an
internal implementation detail and dependency fingerprints propagate dirtiness.
[Cargo fingerprint internals](https://doc.rust-lang.org/stable/nightly-rustc/cargo/core/compiler/fingerprint/index.html)
The HMR store needs a stable application-level identity because OS loaders can
reuse same-path handles and memory-mapped files must remain unchanged.

The build key is input-addressed:

```text
BLAKE3(
  "sprefa-dl7-native-v1\0" ||
  canonical generated Rust bytes ||
  ABI descriptor schema bytes ||
  checked DL7 program digest ||
  imported-module closure digests in stable order ||
  Cargo.lock digest ||
  rustc -Vv bytes ||
  target triple ||
  resolved profile ||
  RUSTFLAGS ||
  linker identity
)
```

This follows the same input categories used by `sccache` for Rust, which hashes
source files with BLAKE3 and includes rustc path, host triple, sysroot, sysroot
shared-library digests, rlib dependencies, and parsed rustc arguments.
[sccache caching contract](https://github.com/mozilla/sccache/blob/main/docs/Caching.md)

Publication protocol:

1. Generate into a staging directory named by the build key.
2. Build with a target directory partitioned by toolchain, target, profile, and
   linker identity.
3. Hash the completed library bytes as `artifact_digest`.
4. Write a manifest containing both digests, exported ABI version, schema and
   program digests, imports, compiler metadata, linker metadata, file size, and
   build timings.
5. Atomically rename the staged directory to
   `artifacts/<build-key>/` after library and manifest validation.
6. Load the absolute immutable path
   `artifacts/<build-key>/<artifact-digest>.<platform-extension>`.
7. Never overwrite a published artifact. Garbage collection only considers an
   artifact after no active or retiring generation references it.

The module graph supplies the invalidation set. Compare these artifact
topologies with identical generated semantics:

| topology | artifact unit | edit rebuild set |
|---|---|---|
| `Artifact.Program.Monolith` | one checked program `cdylib` | entire program |
| `Artifact.Module.File` | one `cdylib` per authored module plus resident dispatch | edited module and reverse dependency closure |
| `Artifact.Module.Scc` | one `cdylib` per strongly connected component | edited SCC and reverse dependency closure |

Imports and reads remain graph edges supplied to generic reachability and SCC
operations. The artifact graph consumes DL7 identities and edges without adding
types to the language kernel.

## 9. Generation-boundary HMR transaction

The active generation is immutable. Build and validation occur while generation
G continues to accept work. Publication occurs between committed ticks:

```text
Build.Start
    -> Build.Compile
    -> Load.Open
    -> Load.Symbol
    -> Swap.Validate
    -> Tick.G.AdmissionClose
    -> Tick.G.RunToCompletion
    -> Swap.Publish
    -> Tick.GPlus1.AdmissionOpen
    -> Swap.Retire
```

Detailed transaction:

1. Compute the affected module closure and build immutable candidate artifacts.
2. Open every candidate and validate its root descriptor while G stays active.
3. Validate the candidate set as one graph: ABI ranges, capability requirements,
   imports, symbol roots, program digest, schema digest, and SQLite catalog.
4. Stop admitting a new outside tick.
5. Let G close and commit pure rules, execute committed host demands, ingest
   their response assertions in later ticks, and repeat until its host-response
   queue and `Time.Next` carry are empty. Long-lived source subscriptions remain
   resident host resources and contribute queued arrivals rather than blocking
   this drain.
6. Atomically replace one `Arc<GenerationSet>` dispatch root.
7. Admit the next outside tick as G+1.
8. Drain calls into the old set, invoke shutdown, and apply the selected pin or
   unload policy.

A compile, open, symbol, descriptor, catalog, or validation failure leaves G
active and never closes admission. A failure after admission closes reopens G
without changing the dispatch root. SQLite migration is a separate transaction
because current V7 rejects a changed catalog before retained rows are modified.

State placement for the initial lab:

| state | owner across generations |
|---|---|
| retained rows and outside tick number | resident SQLite |
| pending arrivals | resident admission queue |
| checked type graph and module graph | resident host |
| generated code and immutable constants | loaded generation |
| per-generation caches | generation handle, discarded or explicitly exported during retirement |

The loaded artifact target is a generated tick body using the root descriptor.
Native plan constructors remain a historical compile-time comparison row and do
not define the dynamic ABI.

## 10. Executable benchmark specification

No harness is present in this lab yet. The following CLI, fixture, matrix, and
receipt schema are the implementation contract for the executable lab. A result
table stays empty until the command exists and completes.

### 10.1 Required command

```bash
cd /Users/chrishafley/projects/sprefa
cargo run \
  --manifest-path v7/labs/19_rust_dynamic_loading/bench/Cargo.toml \
  --profile hmr -- \
  matrix \
  --samples 31 \
  --modules 64 \
  --many-edit-count 32 \
  --output v7/labs/19_rust_dynamic_loading/results/raw.jsonl
```

The future harness must use `tempfile` directories for destructive cold-build
isolation. It must not run `cargo clean` against the repository workspace.
Sample 0 warms filesystem and loader caches and is retained with
`sample_role: "warmup"`; samples 1 through 30 are summarized with median, p05,
p95, minimum, and maximum. Raw rows remain the primary receipt.

### 10.2 Fixture

Generate a deterministic 64-module DL7 graph with these properties:

| fixture part | fixed value |
|---|---|
| module graph | 8 layers of 8 modules; stable import/read edges; one SCC of 4 modules |
| source types | DL7 products, sums, lists, and tagged host-facing values used only through the checked boundary |
| runtime work | one arrival batch, positive joins, one recursive closure, one retraction batch |
| state | direct native and SQLite variants reach byte-identical sorted deltas |
| unchanged edit | rewrite identical generated bytes and preserve content digest |
| one-module edit | change one leaf rule constant without changing ABI or schema |
| many-module edit | change 32 modules across all layers without changing ABI or schema |
| schema edit | separate negative-control case; change one exported layout and require validation rejection or explicit migration path |

Every arm receives the same logical input and produces the same canonical
output digest. Compiler, linker, loader, and runtime measurement must be outside
the correctness digest.

### 10.3 Dimensions

Core native matrix:

| dimension | values |
|---|---|
| `Abi.Arm` | `RawC.Libloading`, `AbiStable`, `Stabby`, `SaferFfi` |
| `Code.Shape` | `Tick.Generated` |
| `Artifact.Topology` | `Program.Monolith`, `Module.File`, `Module.Scc` |
| `Edit.Shape` | `Cold`, `Unchanged`, `OneModule`, `ManyModule`, `SchemaReject` |
| `Profile` | `Hmr`, `Release` |
| `Linker` | every detected compatible linker from section 7.1 |
| `Retirement` | `Pin`, `Unload` where the arm supports unload |
| `State` | `OwnedMemory`, `Sqlite` |

Loader utility matrix:

| dimension | values |
|---|---|
| `Loader` | `Libloading`, `HotLibReloader` |
| `Watch` | `DirectArtifact`, `FilesystemEvent` |
| `Edit.Shape` | `Unchanged`, `OneModule`, `ManyModule` |

Subsecond is its own hotpatch arm because its build and patch protocol differs
from `cdylib` replacement.

### 10.4 Required metrics

| metric | clock and boundary |
|---|---|
| `Build.Generate.WallNs` | checked DL7 input to complete generated Rust bytes |
| `Build.Compile.WallNs` | Cargo process start to successful artifact message |
| `Build.Link.DriverWallNs` | linker wrapper entry to child exit |
| `Build.Units.Count` | Cargo JSON compiler-artifact units rebuilt |
| `Build.Artifacts.Count` | generated dynamic artifacts replaced by this edit |
| `Load.Open.WallNs` | immediately around open/load call |
| `Load.Symbol.WallNs` | root lookup through descriptor validation |
| `Call.First.WallNs` | first fixture call after candidate validation |
| `Call.Steady.WallNs` | median of 10,000 calls after first call |
| `Swap.Validate.WallNs` | candidate-set validation excluding compile/open |
| `Swap.Publish.WallNs` | admission-closed atomic pointer exchange only |
| `Swap.Retire.WallNs` | pointer exchange complete through shutdown and selected close/pin action |
| `Swap.Total.WallNs` | last G call complete through G+1 admission open |
| `Memory.Rss.BaselineBytes` | resident host before candidate load |
| `Memory.Rss.PeakBytes` | peak during load and swap |
| `Memory.Rss.AfterRetireBytes` | fixed 100 ms after retirement plus allocator trim policy recorded |
| `Artifact.File.Bytes` | library file bytes |
| `Artifact.Dependency.Bytes` | unique non-system dynamic dependency bytes, reported separately |
| `Artifact.LoadedImages.Count` | loaded image count before, peak, and after retirement |
| `Correctness.OutputDigest` | BLAKE3 of canonical deltas and committed state |

RSS collection uses `getrusage` plus a sampled platform source. Linux records
`/proc/<pid>/status` and smaps rollup where available; macOS records
`mach_task_basic_info`; Windows records `GetProcessMemoryInfo`. Process and
Wasmtime comparison arms report host, child, and total RSS separately.

### 10.5 Edit protocol

| edit | target-directory rule | required observation |
|---|---|---|
| `Cold` | fresh temporary target and artifact store | all dependency, compile, link, open, and first-call work |
| `Unchanged` | reuse target and artifact store; identical input bytes | zero generated artifacts published; content-addressed cache hit; optional Cargo freshness check timed separately |
| `OneModule` | reuse target; deterministic leaf edit | rebuilt Cargo units and artifacts counted; reverse dependency closure recorded |
| `ManyModule` | reuse target; deterministic 32-module edit | rebuilt Cargo units and artifacts counted; topology closure recorded |
| `SchemaReject` | reuse target; exported schema change | old generation remains active; rejection stage and latency recorded |

### 10.6 Result row

```json
{
  "schema": "sprefa.rust-dynamic-loading.bench.v1",
  "timestamp_utc": "...",
  "git_commit": "...",
  "dirty_paths_digest": "...",
  "platform": {"os": "...", "arch": "...", "cpu": "...", "memory_bytes": 0},
  "toolchain": {"rustc_vv": "...", "cargo_v": "...", "linker_v": "..."},
  "arm": {"abi": "RawC.Libloading", "code": "Tick.Generated", "state": "Sqlite"},
  "topology": "Module.Scc",
  "edit": "OneModule",
  "profile": "Hmr",
  "retirement": "Unload",
  "sample": 1,
  "build_key": "...",
  "artifact_digest": "...",
  "metrics": {
    "generate_wall_ns": 0,
    "compile_wall_ns": 0,
    "link_driver_wall_ns": 0,
    "load_open_wall_ns": 0,
    "load_symbol_wall_ns": 0,
    "first_call_wall_ns": 0,
    "swap_total_wall_ns": 0,
    "rss_peak_bytes": 0,
    "artifact_file_bytes": 0
  },
  "correctness_output_digest": "...",
  "status": "ok"
}
```

### 10.7 Result table

| arm | topology | edit | profile | linker | compile p50 | link p50 | open p50 | symbol p50 | first call p50 | swap p50 | peak RSS delta | artifact bytes | status |
|---|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| pending harness | | | | | | | | | | | | | unmeasured |

## 11. Measured comparison arms

Subprocess and Wasm are included only as rows in the same fixture and metric
schema.

### 11.1 Subprocess

The subprocess arm builds an executable artifact from the same generated code.
The resident host sends framed canonical `TickInputV1` bytes and receives
`TickOutputV1` bytes. State variants are:

1. Host-owned SQLite, with the child using an RPC boundary.
2. Child-owned SQLite, with generation swap using close-old, start-new, catalog
   validation, and ready acknowledgement.

Measure process spawn, ready latency, first call, steady IPC, swap, child RSS,
host RSS, total RSS, and executable plus non-system dependency bytes. Retirement
uses protocol shutdown with a timeout, then records whether termination was
graceful or forced. The V6 historical 1.734 to 2.214 ms warm process handover is
retained as context and rerun on the DL7 fixture.

### 11.2 Wasmtime

[`wasmtime` 48.0.1 was released 2026-08-24](https://docs.rs/crate/wasmtime/latest).
Its `Module` API supports synchronous compilation or serialization followed by
deserialization without recompiling. Serialized modules are accepted only by
the same Wasmtime version. File deserialization can mmap the artifact and
requires the file to remain unchanged for the module lifetime; arbitrary
precompiled bytes are unsafe. [Wasmtime `Module`](https://docs.wasmtime.dev/api/wasmtime/struct.Module.html)

Measure two Wasm rows:

| row | build/load boundary |
|---|---|
| `Wasm.Core.Compile` | generated core Wasm bytes through `Module::new`, instantiate, first call |
| `Wasm.Core.Precompiled` | precompile to immutable content-addressed artifact, `deserialize_file`, instantiate, first call |

The guest imports the same host state callbacks represented by `HostV1` and
exports the same logical tick operation. Report Wasmtime engine RSS separately,
and include the Wasmtime version and configuration in the build key. Component
Model/WIT lifting is an optional third row with its own label because it adds a
different marshaling boundary.

### 11.3 Subsecond

The Subsecond row measures Dioxus CLI build/ThinLink/patch delivery, jump-table
publication, first patched call, RSS, and patch bytes. The generation gate wraps
`apply_patch` so publication occurs only after G commits. Its results remain
separate from `cdylib` open/symbol columns where the operations have no direct
equivalent.

## 12. Verification gates

The future lab is complete only when these assertions are executable:

1. A generated dynamic artifact contains no `PROGRAM_JSON` and no
   `serde_json::from_str` program decoder.
2. `nm`, `llvm-nm`, or `dumpbin /exports` reports exactly the allowed root
   symbols for the selected arm.
3. Loading a fixture with a wrong major version, truncated descriptor, missing
   capability, changed schema, wrong output digest, and corrupted artifact each
   rejects before publication.
4. A failed candidate leaves the old generation able to execute the next tick.
5. Concurrent swap stress never calls an old generation after retirement.
6. Direct native and SQLite execution produce the same canonical additions and
   retractions before and after a successful swap.
7. An unchanged edit publishes zero artifacts.
8. One-module and many-module edits report the exact graph-derived rebuild set.
9. Pin and unload policies report loaded-image count and RSS across at least
   1,000 swaps.
10. The process and Wasmtime arms consume the same logical inputs and match the
    native output digest.

## 13. Research gaps

| gap | evidence needed |
|---|---|
| generated tick body | current DL7 Rust emitter constructs kernel plan values; it does not emit a standalone tick function |
| executable lab harness | `bench/Cargo.toml`, fixtures, linker wrapper, platform RSS probes, and JSONL summarizer do not exist |
| application emitter | no current DL7 application-emitter generation-boundary ABI was located in this checkout |
| SQLite migration | current catalog rejects a changed plan; schema/state migration protocol is pending |
| ABI payload | `Bytes` encoding, host callback table, status codes, cancellation, and diagnostics need checked type definitions |
| module artifact split | the current generated fixture is one statically included Rust region; per-file and per-SCC artifacts need a build graph |
| unload proof | plugin thread/TLS/global behavior and dependency destructors need 1,000-swap stress on Linux, macOS, and Windows |
| macOS loader currentness | Apple's linked `dlopen` manual is archival; record dyld behavior and code-sign requirements on supported macOS releases |
| Windows receipt | full-path load, immutable DLL naming, delete-after-unload, PDB output, `link.exe`, and `lld-link` remain unmeasured |
| Linux receipt | rust-lld, mold, GNU BFD, Wild, `dlopen`, and optional `dlmopen` remain unmeasured on the DL7 fixture |
| dependency versions | `abi_stable` latest release is from 2023; `stabby` latest docs build fails; `safer_ffi` latest stable is 0.1.13 while 0.2.0 is prerelease |
| raw ABI review | descriptor layout needs platform ABI inspection and generated C header/golden layout tests |
| old receipt replay | commit `beb8e55b3` contains the result document; the temporary harness is absent from the commit |

## 14. Source ledger

| subject | source | verified fact used |
|---|---|---|
| Rust artifact types | [Rust Reference linkage](https://doc.rust-lang.org/reference/linkage.html) | `dylib` and `cdylib` roles; platform extensions; runtime-loaded panic compatibility responsibility |
| Rust calling ABI | [Rust Reference external blocks](https://doc.rust-lang.org/reference/items/external-blocks.html#abi) | Rust ABI has no stability guarantee; C ABI matches target C convention |
| native loading | [`libloading` 0.9.0](https://docs.rs/crate/libloading/latest) | release date, symbol lifetime, dependencies |
| Unix loading | [Linux `dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html) | same-object handles, reference counts, close limits, namespaces, constructors/destructors |
| macOS loading | [Apple archived `dlopen(3)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/dlopen.3.html) | same-path handle and `RTLD_NODELETE`; archival date |
| Windows loading | [`LoadLibraryExW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw), [`FreeLibrary`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-freelibrary) | full-path and base-name behavior; ref-counted unload |
| stable Rust ABI library | [`abi_stable` 0.11.3](https://docs.rs/crate/abi_stable/latest) | release date, prefix modules, type-layout checks, explicit lack of unloading |
| stable layouts and checked symbols | [`stabby` 72.1.16](https://docs.rs/crate/stabby/latest) | release date, report/canary exports, docs build failure, trait-object warning |
| C FFI generation | [`safer_ffi` 0.1.13](https://docs.rs/crate/safer-ffi/latest) | stable and prerelease dates, exports, headers, MSRV |
| dynamic reload utility | [`hot-lib-reloader` 0.8.2](https://docs.rs/crate/hot-lib-reloader/latest) | shadow filenames, callbacks, limitations, macOS signing |
| hotpatch utility | [`subsecond` 0.7.10](https://docs.rs/crate/subsecond/latest) | jump-table model, release dates, ThinLink and current limitations |
| Cargo profiles and caches | [profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html), [timings](https://doc.rust-lang.org/cargo/reference/timings.html) | incremental scope/defaults, target/build directories, timings limits |
| cache-key precedent | [sccache caching](https://github.com/mozilla/sccache/blob/main/docs/Caching.md) | Rust input hash categories and BLAKE3 |
| LLD | [LLVM LLD documentation](https://lld.llvm.org/), [Rust 1.90 report](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/) | formats, Linux default, upstream workload numbers |
| mold | [2026 paper](https://arxiv.org/abs/2608.23228) | published benchmark ratios and scope |
| Wild | [project README](https://github.com/wild-linker/wild), [incremental design](https://davidlattimore.github.io/posts/2024/11/19/designing-wilds-incremental-linking.html) | shared objects work; incremental linking remains planned |
| Wasmtime | [`wasmtime` 48.0.1](https://docs.rs/crate/wasmtime/latest), [`Module` API](https://docs.wasmtime.dev/api/wasmtime/struct.Module.html) | release date; compile/serialize/deserialize contract and immutable file requirement |
