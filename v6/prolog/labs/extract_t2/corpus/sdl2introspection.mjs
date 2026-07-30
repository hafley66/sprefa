// sdl2introspection.mjs : GraphQL SDL -> introspection JSON, the only step
// between a repo's .graphql file and the json plane.
//
//   npx --yes -p graphql@16.11.0 node sdl2introspection.mjs in.graphql out.json
//
// This is the whole answer to slot_graphql_entry: graphql-js is the reference
// implementation, `buildSchema` + `introspectionFromSchema` is two calls, and
// the result is ordinary JSON. The alternative CLIs (get-graphql-schema,
// @graphql-inspector/cli) introspect a RUNNING SERVER over HTTP, which a
// parse-only pipeline over checked-out repos cannot use.
//
// The cost this measures, and the reason the slot exists: introspection JSON is
// several times the size of the SDL it came from.
import { readFileSync, writeFileSync } from "node:fs";
import { buildSchema, introspectionFromSchema } from "graphql";

const [, , input, output] = process.argv;
const sdl = readFileSync(input, "utf8");
const started = Date.now();
const introspection = introspectionFromSchema(buildSchema(sdl));
const elapsed = Date.now() - started;
const text = JSON.stringify(introspection);
writeFileSync(output, text);
console.log(
  `sdl ${sdl.length} bytes -> introspection ${text.length} bytes ` +
    `(${(text.length / sdl.length).toFixed(2)}x) in ${elapsed}ms`,
);
