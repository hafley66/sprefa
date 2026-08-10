/**
 * serveOpenapi.test.ts -- unit coverage for `GET /openapi.json`
 * (serve/openapiDoc.ts, serve/4_http.ts): the OpenAPI 3.1 document the served
 * engine answers, whose components.schemas follows the loaded program (hot
 * reload: the doc is rebuilt on every program swap, so it tracks the loaded
 * rel_catalog the way the reload planner does).
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { post_program, request, start_served } from "./serveHelpers.ts";

const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

interface OpenapiDoc {
  readonly openapi: string;
  readonly info: { readonly title: string; readonly version: string };
  readonly paths: Readonly<Record<string, unknown>>;
  readonly components: { readonly schemas: Readonly<Record<string, unknown>> };
}

test("GET /openapi.json is always 200: routes present, schemas empty before any program", async () => {
  const served = await start_served();
  try {
    const result = await request(served.port, "/openapi.json", "GET");
    assert.equal(result.statusCode, 200);
    const doc = JSON.parse(result.body) as OpenapiDoc;
    assert.equal(doc.openapi, "3.1.0");
    // The engine's real route list (serve/4_http.ts), with the server's
    // `:rel` spelling translated to OpenAPI's `{rel}`.
    assert.ok(doc.paths["/program"], "missing /program");
    assert.ok(doc.paths["/edb/events"], "missing /edb/events");
    assert.ok(doc.paths["/idb/{rel}"], "missing /idb/{rel}");
    assert.ok(doc.paths["/ticks"], "missing /ticks");
    assert.ok(doc.paths["/stats"], "missing /stats");
    assert.ok(doc.paths["/openapi.json"], "missing /openapi.json");
    assert.deepEqual(doc.components.schemas, {});
  } finally {
    await served.stop();
  }
});

test("GET /openapi.json tracks the loaded program's rel shapes", async () => {
  // `at: span` is a rel-typed column, so the schema carries a real $ref -- the
  // OpenAPI pointer into components.schemas, distinct from JSON Schema's $defs.
  const source = "rel span(start: int, end: int).\nrel finding(path: text, at: span).\n";
  const served = await start_served();
  try {
    assert.equal((await post_program(served.port, source)).statusCode, 200);
    const result = await request(served.port, "/openapi.json", "GET");
    assert.equal(result.statusCode, 200);
    const doc = JSON.parse(result.body) as OpenapiDoc;
    assert.ok(doc.components.schemas.finding, "missing finding schema");
    assert.ok(doc.components.schemas.span, "missing span schema");
    const finding = doc.components.schemas.finding as { properties: Readonly<Record<string, unknown>> };
    assert.deepEqual(finding.properties.at, { $ref: "#/components/schemas/span" });
    const body = JSON.stringify(doc);
    assert.ok(body.includes("#/components/schemas/"), "expected an OpenAPI schema pointer");
    assert.ok(!body.includes("#/$defs/"), "JSON Schema $defs pointer leaked into OpenAPI");
  } finally {
    await served.stop();
  }
});

test("swapping programs rebuilds GET /openapi.json to the new program's shapes", async () => {
  const first = readFileSync(DOOR_DL6, "utf8");
  const served = await start_served();
  try {
    await post_program(served.port, first);
    const before = (JSON.parse((await request(served.port, "/openapi.json", "GET")).body) as OpenapiDoc).components.schemas;
    assert.ok(before.event);

    // A second, unrelated program drops the door rels and introduces its own.
    await post_program(served.port, "rel pick(a: int, label: text).\n");
    const after = (JSON.parse((await request(served.port, "/openapi.json", "GET")).body) as OpenapiDoc).components.schemas;
    assert.ok(after.pick, "expected the swapped program's pick schema");
    assert.ok(!after.event, "door rel survived the swap");
  } finally {
    await served.stop();
  }
});
