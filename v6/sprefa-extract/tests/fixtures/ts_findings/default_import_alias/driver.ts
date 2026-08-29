// Corpus finding, NOT FIXED: a default import binds a LOCAL spelling to the
// exporting module's default export, and `call_name_match` never reads the
// `specifier` row, so the call cannot resolve. The `specifier` record already
// carries everything the join needs (kind "default", module "./generator.ts",
// imported "default", name "runGenerator"); the resolve arm matches on the
// callee spelling alone against the corpus-wide def index.
//
// Measured on microsoft/TypeScript @9a8581c3: `tools/scripts/tsc/generate.ts`
// calls three default imports this way, and the miss makes the whole code
// generator subtree unreachable from any entrypoint crawl. 5 `main` call sites
// in that corpus resolve to 0 edges.
//
// Repro:
//   extract --resolve --family call driver.ts generator.ts
// Expected: one resolved_edge caller driver.ts:run -> generator.ts:main.
// Observed: no edge; the site's callee spelling `runGenerator` names no def.
import runGenerator from "./generator.ts";

export function run(): void {
    runGenerator();
}
