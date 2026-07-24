/**
 * 2_schema.ts — decl -> SQLite DDL: rel_* current tables, delta log, effect_cache,
 * store_meta.
 *
 * Contract (plan M2, tasks.d.ts): `ddl(decls, retention) -> string[]`.
 *   rel_<name>(cols..., PRIMARY KEY(all cols))     -- set semantics
 *   delta(rel, row_digest, tick, weight)           -- tick shape (b), pinned in DECISIONS
 *   effect_cache(digest PK, host, state, requested_tick)
 *   store_meta(key PK, value)                      -- the monotone 'tick' counter row
 * Row identity = full column tuple; row_digest = oracle.mix XOR law (ingest.ts note 6).
 *
 * Owned by package M2 (schema+runtime). Placeholder until that package lands.
 */
export {};
