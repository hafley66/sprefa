# 039b Lower `bytes` across targets

## Assignment

Implement issue `bytes-target-lowering` from the Sprefa-v6 board. Work from
Sprefa commit `36f56f008`, which completed `039a` and added `bytes` to the DL6
parser, printer, catalog, SQLite schema lowering, and IR column class.

Read first:

- `v6/dl/fixtures/bytes-type-system.dl6`
- `v6/prolog/emit_rust.pl`
- `v6/prolog/emit_ts.pl`
- `v6/sprefa-engine-rs/src/types.rs`
- `v6/sprefa-engine-rs/src/sql.rs`
- `v6/sprefa-engine-rs/src/incremental.rs`
- `v6/sprefa-engine-rs/src/ticklog.rs`
- `v6/tsv2/runtime/types.ts`
- `v6/tsv2/runtime/0_sql.ts`
- `v6/tsv2/runtime/1_incremental.ts`
- `v6/tsv2/runtime/tickLog.ts`
- `v6/tsv2/serve/1_hosts.ts`

Line numbers are hints only. Re-find every symbol.

## Required type signatures

Rust target:

```rust
enum RowColumnType {
    // existing variants
    Bytes,
}

enum Value {
    // existing variants
    Bytes(Vec<u8>),
}
```

TypeScript target:

```ts
type IRowColumnType =
  | "text"
  | "int"
  | "bool"
  | "float"
  | "ref"
  | "json"
  | "list"
  | "bytes";

type IRowBytes = Uint8Array;
type IRowValue = IRowScalar | IRowValueArray | IRowBytes;
```

Program JSON and any other JSON-only transport use one explicit tagged form:

```ts
type IEncodedBytes = { readonly $bytes: string }; // canonical RFC 4648 base64
```

Native runtime rows remain `Vec<u8>` and `Uint8Array`. Do not represent bytes
as ordinary strings or JSON number arrays inside those runtimes.

## Instance lifetime

1. The compiler emits column type `bytes` in `ProgramJson` and generated TS.
2. A native arrival owns `Vec<u8>` or `Uint8Array`.
3. SQL bind stores that value as a BLOB without UTF-8 conversion.
4. SQL reads reconstruct the native byte container.
5. Equality, keys, deltas, retractions, and tick logs preserve exact byte
   identity, including empty bytes, embedded NUL, invalid UTF-8, and `0xff`.
6. JSON-only boundaries encode and decode the tagged base64 object exactly
   once at their boundary.

## Storage, reads, writes, uniqueness

- SQLite storage class is BLOB. Verify `typeof(column) = 'blob'` through an
  integration test.
- Byte equality is byte-for-byte equality. No text decoding, normalization,
  or lossy display conversion is permitted.
- The Rust and TS bind helpers must accept native bytes at SQL parameter seams.
- Boundary readers must distinguish BLOB from TEXT.
- Tick-log rendering must be deterministic and must distinguish bytes from a
  text value containing the same base64 characters.
- Host JSON decoding may accept only the tagged `$bytes` object for a declared
  `bytes` output. Shell template interpolation and environment variables must
  return a named unsupported-boundary error unless an existing explicit binary
  transport already exists. Do not place arbitrary bytes in argv or env.

## Required implementation coverage

1. `emit_rust.pl` emits `bytes` without collapsing it to text/json.
2. `emit_ts.pl` emits `bytes` in boundary, stored, and declared type tables.
3. Rust `ProgramJson`, `RowColumnType`, `Value`, SQL bind/read, incremental
   bind/read, host output coercion, and tick-log paths support bytes.
4. TypeScript runtime types, SQL bind/read, arrival validation, host JSON
   output coercion, HTTP validation, and tick-log paths support bytes.
5. Remove the temporary `bytes_arrival_unavailable` refusal only when both
   emitted runtimes have typed arrival validation and storage coverage.
6. Regenerate only directly affected checked-in golden artifacts. Do not run a
   broad golden rewrite unless a focused gate proves it is required.

## Acceptance tests

Add one compiler-to-runtime golden per target. Each must originate from authored
DL6 containing a `bytes` column, consume native byte arrivals, and prove:

- empty bytes round-trip;
- `[0x00, 0x7f, 0x80, 0xff]` round-trips exactly;
- equal byte sequences join or deduplicate as equal;
- one changed byte remains distinct;
- deletion retracts the exact prior byte row;
- SQLite reports BLOB storage;
- tick-log or JSON transport uses the tagged base64 form and cannot collide
  with text;
- malformed base64 and an untagged string/array receive deterministic named
  errors.

Prefer maximal deterministic snapshots over granular presence assertions. Do
not use `toBeDefined`.

## Gates

Run focused tests while implementing. At completion run, at most once each:

```bash
swipl -q -l v6/prolog/compile/test/plunit_tests.pl \
  -g "run_tests([bytes_type_system])" -g halt

(cd v6/sprefa-engine-rs && cargo test --offline)
(cd v6/sprefa-engine-rs && cargo clippy --offline --all-targets -- -D warnings)
(cd v6/tsv2 && npm test -- --run)
(cd v6/tsv2 && npm run typecheck)
git diff --check
```

Record baseline failures separately. Do not edit unrelated failures.

## Laws

- Work only inside the lane worktree.
- Preserve author-driven numeric file ordering. Tests mirror source numbering.
- Keep this as a type lowering. Do not add source mutation effects, batching,
  byte literals, wrapper composition, or collection aggregation.
- No N+1 SQL reads or per-value subprocesses.
- No shelling out for base64.
- No writes to user state, running daemons, or external repositories.
- Run `cargo fmt` once immediately before the commit if Rust changed.
- One commit, no push. Use `Refs-Issue: @bytes-target-lowering`.
- If an existing runtime boundary cannot carry native bytes without changing a
  public dependency or transport contract beyond this brief, stop at that
  boundary and report the exact signature and call sites. Do not substitute
  strings silently.

## Final report

Return:

1. commit hash;
2. files and signatures changed;
3. exact runtime encoding at every boundary;
4. golden and integration receipts;
5. full-gate results and baseline failures;
6. any stopped boundary with exact symbols and reason.
