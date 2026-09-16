## [unreleased]

### 🚀 Features

- *(v8)* Dl8 evaluator kernel with v7 oracle parity
- *(v8)* DL7 reader with byte-exact v7 parity
- *(v8)* Port the DL7 lowerer to Rust, byte-parity on every fixture
- *(v8)* Macrotime with v7 parity through the real binary
- *(v8)* Dl8 expand --trace prints each macrotime wave
- *(v8)* Filesystem, TSI and source-fact loaders with v7 parity
- *(v8)* Checker with v7 parity through the real binary
- *(v8)* Logical program reifier with v7 parity through the real binary
- *(v8)* Comptime rounds with v7 parity through the real binary
- *(v8)* Dl8 compile, every phase wired, v7 compile_dl7/4 parity
- *(v8)* Tracing spans and structured events across every phase
- *(v8)* Float and bool terms, int_add kernel
- *(v8)* Sum, min, max aggregate heads
- *(v8)* Effect rows for served relations
- *(v8)* Term_lt kernel relation
- *(v8)* SQLite row store, dl8 eval --db
- *(v8)* Programmable Fold with the four builtins as the fast path
- *(v8)* Relation names travel with the runtime program
- *(v8)* Reconciler with timer source and fetch_json executor
- *(v8)* Emit body joins as JOIN ... ON in a keyed order
- *(v8)* Soopy and extract executors

### 🐛 Bug Fixes

- *(v8)* Dl8 eval reads the program key of an oracle fixture
- *(v8)* Canonicalize the extract manifest path in the e2e test

### 📚 Documentation

- *(v8)* README rows for read, macrotime, lower and the wave-two stubs
- *(v8)* The dl8 book, every claim a fixture
- *(v8)* One chained mermaid per book chapter
- *(v8)* Modules, prelude and hosting parts of the book; namespacing inspection
- *(v8)* Fold coordinator corrections into modules pages and the namespacing inspection
- *(v8)* Render mermaid in the book via mdbook-mermaid

### ⚡ Performance

- *(v8)* Fixed-array term reads instead of a heap row per lookup
- *(v8)* Inline the two halves of the eval inner loop

### 🚜 Refactor

- *(v8)* One Slice trait for the four effect sinks
- *(v8)* Split the eight long functions at their repeated shapes
- *(v8)* No function over 70 lines
- *(v8)* One kernel_arity in _5_kernel.rs

### 🧪 Testing

- *(v8)* Extract from hafley-rs feeds dl8 compile --tsi
- *(v8)* Build extract from its own manifest
- *(v8)* One test per sqlite_emit fixture, and the fold delete closure

### ⚙️ Miscellaneous Tasks

- *(v8)* Phase stubs for the read, macrotime and lower lanes
- *(v8)* Clippy map_or leftovers
- *(v8)* Phase stubs for check, load, comptime and reify lanes
- *(v8)* Pub fields on the lower graph indexes, README rows for check and load
- *(v8)* Depend on hafley-observe from the local hafley registry, tracing crate
- *(v8)* Regenerate the oracle expected from dl8
- *(v8)* Term_lt joins KERNEL_RELATIONS after the merge, oracles refrozen
- *(v8)* Git-cliff changelog, regenerated on every push to main
- *(v8)* Regenerate CHANGELOG.md

### 💼 Other

- *(v8)* The _2_lower port plan and the lowering oracle
- *(v8)* Sqlite emitter draft, uncommitted opus lane work
