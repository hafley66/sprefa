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

test("openapiToDl6: strict keeps a ref target's generic columns the compiler accepts", () => {
  // Sabotage receipt: asserting the old whole-rel drop
  // (/rel item\(price: json, kids: json, name: text\)/) fails against this doc.
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
  assert.match(prog, /rel item\(price: option\(int\), kids: list\(kid\), name: text\)/);
  assert.equal(strict.gapList.length, 0);
  const tmp = path.join(os.tmpdir(), "openapi_strict_ref_target.dl6");
  fs.writeFileSync(tmp, prog);
  const r = compile(tmp);
  assert.equal(r.code, 0, `strict ref-target compile failed:\n${r.stdout}`);
});

test("openapiToDl6: strict keeps option(<rel>) on a ref target", () => {
  // Sabotage receipt: asserting the old drop (/kid: json/) fails against this
  // doc; the compiler accepts option(<rel>) on a reference target since the
  // type_decl mirror follows the option desugar's column deletion.
  const doc = {
    components: {
      schemas: {
        Holder: { type: "object", properties: { item: { $ref: "#/components/schemas/Item" } } },
        Item: {
          type: "object",
          properties: {
            price: { type: "integer", nullable: true },
            kid: { allOf: [{ $ref: "#/components/schemas/Kid" }], nullable: true },
            name: { type: "string" },
          },
        },
        Kid: { type: "object", properties: { name: { type: "string" } } },
      },
    },
  } as never;
  const strict = new OpenapiToDl6(doc, "strict");
  const prog = strict.convert();
  assert.match(prog, /rel item\(price: option\(int\), kid: option\(kid\), name: text\)/);
  assert.deepEqual(strict.gapList, []);
  const tmp = path.join(os.tmpdir(), "openapi_strict_option_ref.dl6");
  fs.writeFileSync(tmp, prog);
  const r = compile(tmp);
  assert.equal(r.code, 0, `strict option-ref compile failed:\n${r.stdout}`);
});

test("openapiToDl6: a nullable lifted object takes the _object suffix", () => {
  // Sabotage receipt: asserting the unsuffixed /kid: option\(item__kid\)/
  // fails, and that spelling stops as
  // unsupported_construct(option_companion_name_collision(item__kid/1, item/3, kid)).
  const doc = {
    components: {
      schemas: {
        Holder: { type: "object", properties: { item: { $ref: "#/components/schemas/Item" } } },
        Item: {
          type: "object",
          properties: {
            price: { type: "integer", nullable: true },
            kid: { type: "object", nullable: true, properties: { name: { type: "string" } } },
            sibling: { type: "object", properties: { name: { type: "string" } } },
            name: { type: "string" },
          },
        },
      },
    },
  } as never;
  const strict = new OpenapiToDl6(doc, "strict");
  const prog = strict.convert();
  assert.match(prog, /kid: option\(item__kid_object\)/);
  assert.match(prog, /sibling: item__sibling,/);
  assert.match(prog, /rel item__kid_object\(name: text\)\./);
  assert.deepEqual(strict.gapList, []);
  const tmp = path.join(os.tmpdir(), "openapi_strict_companion_collision.dl6");
  fs.writeFileSync(tmp, prog);
  const r = compile(tmp);
  assert.equal(r.code, 0, `strict collision compile failed:\n${r.stdout}`);
});

test("openapiToDl6: strict drops a ref target whose every column is a nullable ref", () => {
  // Both columns move to companion split rels, so the ref target keeps no
  // stored columns and no identity:
  // unsupported_construct(reference_target_has_no_columns(item__pair/0)).
  const doc = {
    components: {
      schemas: {
        Holder: { type: "object", properties: { item: { $ref: "#/components/schemas/Item" } } },
        Item: {
          type: "object",
          properties: {
            pair: {
              type: "object",
              properties: {
                before: { type: "array", nullable: true, items: { $ref: "#/components/schemas/Kid" } },
                after: { type: "array", nullable: true, items: { $ref: "#/components/schemas/Kid" } },
              },
            },
          },
        },
        Kid: { type: "object", properties: { name: { type: "string" } } },
      },
    },
  } as never;
  const strict = new OpenapiToDl6(doc, "strict");
  const prog = strict.convert();
  assert.deepEqual(
    strict.gapList,
    [
      "item__pair.before: option(list(kid)) -> json (probe did not compile)",
      "item__pair.after: option(list(kid)) -> json (probe did not compile)",
    ],
  );
  const tmp = path.join(os.tmpdir(), "openapi_strict_empty_ref_target.dl6");
  fs.writeFileSync(tmp, prog);
  const r = compile(tmp);
  assert.equal(r.code, 0, `strict empty ref target compile failed:\n${r.stdout}`);
});

test("openapiToDl6: snake_casing name collision refused loudly", () => {  const doc = {
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

test("openapiToDl6: expansionDl6 emits one schema_expansion fact per rel, compiles", () => {
  const c = new OpenapiToDl6(fixture, "full");
  const exp = c.expansionDl6();
  assert.ok(exp.startsWith("rel schema_expansion(source: text, rel: text, decl: text)."));
  // a component rel carries its PascalCase source and a decl naming the rel
  assert.match(exp, /schema_expansion\('AbilityDetail', 'ability_detail', 'rel ability_detail\(/);
  // a lifted inline-object rel inherits its component's source
  assert.match(exp, /schema_expansion\('AbilityDetail', 'ability_detail__meta', 'rel ability_detail__meta\(/);
  const tmp = path.join(os.tmpdir(), "openapi_expansion.dl6");
  fs.writeFileSync(tmp, exp);
  const r = compile(tmp);
  assert.equal(r.code, 0, `expansion compile failed:\n${r.stdout}`);
});
