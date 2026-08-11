// openapiToDl6.test.ts — converter + compile on a tiny clean hand openapi doc
// exercising every mapping row (list(rel), LIFT, payload enum, option, json_list).

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { test } from "node:test";

import YAML from "yaml";

import { OpenapiToDl6, OpenapiNameCollision, snakeCase } from "../scripts/openapi_to_dl6.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TSV2 = path.resolve(HERE, "..");
const V6 = path.resolve(TSV2, "..");
const COMPILE_SH = path.join(V6, "prolog", "compile", "scripts", "compile_dl6.sh");
const HAND = path.join(HERE, "fixtures", "openapi_mapping_hand.yml");

function compile(dl6Path: string): { code: number; stdout: string } {
  const out = path.join(os.tmpdir(), `openapi_test_${path.basename(dl6Path)}.ts`);
  try {
    const stdout = execFileSync(COMPILE_SH, [dl6Path, out], { encoding: "utf8" });
    return { code: 0, stdout };
  } catch (e) {
    const err = e as { status?: number; stdout?: string; stderr?: string };
    return { code: err.status ?? 1, stdout: (err.stdout ?? "") + (err.stderr ?? "") };
  }
}

const fixture = YAML.parse(fs.readFileSync(HAND, "utf8"));

test("openapiToDl6: full mapping emits the expected rel lines", () => {
  const c = new OpenapiToDl6(fixture, "full");
  const prog = c.convert();
  const rels = c.declaredRels.map((r) => r.name);

  assert.ok(rels.includes("ability_detail"));
  assert.ok(rels.includes("ability_summary"));
  assert.ok(rels.includes("move_summary"));
  // inline lift (recursive)
  assert.ok(rels.includes("ability_detail__meta"));
  assert.ok(rels.includes("ability_detail__meta__nested"));
  assert.ok(rels.includes("ability_detail__moves"));
  // payload enum rel
  assert.ok(rels.includes("ability_detail__union"));

  // nullable scalar -> option(T)
  assert.match(prog, /is_main_series: option\(bool\)/);
  assert.match(prog, /maybe_score: option\(int\)/);
  // nullable ref (oneOf [ref, null]) -> option(ref)
  assert.match(prog, /maybe_summary: option\(ability_summary\)/);
  // array of scalars -> json_list
  assert.match(prog, /tags: json_list\(text\)/);
  assert.match(prog, /scores: json_list\(int\)/);
  // array of component refs -> list(rel)
  assert.match(prog, /related: list\(move_summary\)/);
  // array of inline objects -> list(lift)
  assert.match(prog, /moves: list\(ability_detail__moves\)/);
  // inline object -> lifted rel ref
  assert.match(prog, /meta: ability_detail__meta/);
  // oneOf payload enum
  assert.match(prog, /rel ability_detail__union\(variant_1\(payload: ability_summary\) ; variant_2\(payload: move_summary\)\)/);
});

test("openapiToDl6: full-mapping output compiles (compile_dl6.sh exit 0)", () => {
  const c = new OpenapiToDl6(fixture, "full");
  const tmp = path.join(os.tmpdir(), "openapi_hand_gen.dl6");
  fs.writeFileSync(tmp, c.convert());
  const r = compile(tmp);
  assert.equal(r.code, 0, `compile failed:\n${r.stdout}`);
});

test("openapiToDl6: strict equals full on the clean hand fixture, compiles, no drops", () => {
  // The hand fixture is deliberately non-interconnected, so strict mode must
  // not drop any column (no rel is a ref target carrying generic columns).
  const strict = new OpenapiToDl6(fixture, "strict");
  const full = new OpenapiToDl6(fixture, "full");
  assert.equal(strict.convert(), full.convert());
  assert.equal(strict.gapList.length, 0);
  const tmp = path.join(os.tmpdir(), "openapi_hand_strict.dl6");
  fs.writeFileSync(tmp, strict.convert());
  const r = compile(tmp);
  assert.equal(r.code, 0, `strict compile failed:\n${r.stdout}`);
});

test("openapiToDl6: strict drops a ref-target's generic columns with attribution", () => {
  // A rel used as a ref target that also carries option/list columns is refused
  // by the compiler (0_type_plane.pl:128); strict drops exactly those columns.
  const doc = {
    components: {
      schemas: {
        Holder: { type: "object", properties: { item: { $ref: "#/components/schemas/Item" } } },
        Item: {
          type: "object",
          properties: {
            price: { type: "integer", nullable: true },
            kids: { type: "array", items: { $ref: "#/components/schemas/Kid" } },
            name: { type: "string" },
          },
        },
        Kid: { type: "object", properties: { name: { type: "string" } } },
      },
    },
  } as never;
  const strict = new OpenapiToDl6(doc, "strict");
  const prog = strict.convert();
  assert.match(prog, /rel item\(price: json, kids: json, name: text\)/);
  assert.ok(strict.gapList.some((g) => g.startsWith("item.price: option(int)")));
  assert.ok(strict.gapList.some((g) => g.startsWith("item.kids: list(kid)")));
  assert.ok(strict.gapList.every((g) => g.includes("0_type_plane.pl:128")));
});

test("openapiToDl6: snake_casing name collision refused loudly", () => {
  const doc = {
    components: {
      schemas: {
        FooBar: { type: "object", properties: { a: { type: "string" } } },
        Foo_Bar: { type: "object", properties: { b: { type: "string" } } },
      },
    },
  };
  // FooBar and Foo_Bar both snake to foo_bar
  assert.throws(() => new OpenapiToDl6(doc as never), (e: unknown) => e instanceof OpenapiNameCollision);
});

test("openapiToDl6: snakeCase keeps digit runs intact", () => {
  assert.equal(snakeCase("LanguageDetail"), "language_detail");
  assert.equal(snakeCase("iso639"), "iso639");
  assert.equal(snakeCase("iso3166"), "iso3166");
  assert.equal(snakeCase("HTTPResponse"), "http_response");
});
