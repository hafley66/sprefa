/**
 * index.ts — lib root re-exports. Mirrors the Rust `lib.rs` public surface: the engine
 * modules (cascade/reach/reconcile/temporal), the Store/RelStore/GraphNs, the spine schema,
 * the measurement harness (benchgraph/memcap), the oracle math, and the parity Traits.
 *
 * `./engine/types.ts` is the header: every class contract in one file, no bodies. It goes
 * FIRST because it is what a reader (or an importer) should meet before any implementation.
 * Its interfaces are I-prefixed (IStore, IRelStore, ...), so each one sits beside the class
 * that implements it without either having to give up the plain name.
 */

export type * from "./engine/types.ts";
export * from "./engine/engine.ts";
export * from "./engine/lib.ts";
export * from "./engine/spine.ts";
export * from "./engine/algo.ts";
export * from "./engine/measure.ts";
export * from "./engine/oracle.ts";
export * from "./engine/tasks.ts";
