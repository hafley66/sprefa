/**
 * index.ts — lib root re-exports. Mirrors the Rust `lib.rs` public surface: the engine
 * modules (cascade/reach/reconcile/temporal), the Store/RelStore/GraphNs, the spine schema,
 * the measurement harness (benchgraph/memcap), the oracle math, and the parity Traits.
 *
 * `./engine/types.ts` is the header: every class contract in one file, no bodies. It goes
 * FIRST because it is what a reader (or an importer) should meet before any implementation.
 * The re-export is explicit rather than `export type *`: four header interfaces (GraphNs,
 * RelStore, Store, SqliteReach) are deliberately spelled the same as the classes that
 * implement them, and a barrel cannot carry both under one name. The class wins here, since
 * its instance type is identical and callers of the barrel want the constructor too. Import
 * from "./engine/types.ts" directly to get the interface on its own.
 */

export type {
  AssertTrue,
  EdgeRow,
  GraphNsStatics,
  Interner,
  InternerStatics,
  NodeRow,
  RelStoreStatics,
  SpanRow,
  SqliteDb,
  SqliteReachStatics,
  StoreStatics,
  TemporalStore,
  TemporalStoreStatics,
} from "./engine/types.ts";
export * from "./engine/engine.ts";
export * from "./engine/lib.ts";
export * from "./engine/spine.ts";
export * from "./engine/algo.ts";
export * from "./engine/measure.ts";
export * from "./engine/oracle.ts";
export * from "./engine/tasks.ts";
