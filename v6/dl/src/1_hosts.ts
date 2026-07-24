/**
 * 1_hosts.ts — HostDef registry: sh executor, builtin sg (ast-grep), builtin extract.
 *
 * Contract (plan M4, tasks.d.ts): `HostDef{name, requestCols, responseCols, run}`;
 * `shHost(decl)` fills the backtick template ({col} raw / $col env) and spawns;
 * `builtinSg` = `sg run --pattern <p> --json <path>` (bin from node_modules/.bin,
 * devDep @ast-grep/cli); `builtinExtract` = DL_EXTRACT_BIN. `HostRunner` subscribes
 * deltas$ for __req_* inserts, digest-caches via effect_cache (the `?` idempotence law),
 * commits __resp_* rows in one batch. One in-flight run per digest; errors land as
 * cache state 'error'; the stream never dies.
 *
 * Owned by package M4 (hosts). Placeholder until that package lands.
 */
export {};
