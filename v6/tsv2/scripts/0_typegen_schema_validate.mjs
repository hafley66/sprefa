import fs from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";

const [schemaPath, kind] = process.argv.slice(2);
const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const fail = (message) => {
  throw new Error(`${schemaPath}: ${message}`);
};

if (schema.$schema !== "https://json-schema.org/draft/2020-12/schema") {
  fail("missing draft 2020-12 $schema");
}
if (typeof schema.$id !== "string") fail("$id must be a string");
if (schema.$defs === null || typeof schema.$defs !== "object" || Array.isArray(schema.$defs)) {
  fail("$defs must be an object");
}

const ajv = new Ajv2020({ allErrors: true, strict: true });
try {
  ajv.compile(schema);
} catch (error) {
  fail(`metaschema compilation failed: ${error.message}`);
}

const resolve = (node) => {
  if (node.$ref === undefined) return node;
  const prefix = "#/$defs/";
  if (!node.$ref.startsWith(prefix)) fail(`unsupported reference ${node.$ref}`);
  const target = schema.$defs[node.$ref.slice(prefix.length)];
  if (target === undefined) fail(`dangling $ref ${node.$ref}`);
  return target;
};

const containsOneOf = (node, seen = new Set()) => {
  node = resolve(node);
  if (seen.has(node)) return false;
  seen.add(node);
  if (node.oneOf !== undefined) return true;
  if (node.items !== undefined && containsOneOf(node.items, seen)) return true;
  return Object.values(node.properties ?? {}).some((child) => containsOneOf(child, seen));
};

const representative = (node, seen = new Set()) => {
  node = resolve(node);
  if (seen.has(node)) fail("recursive representative instance is unsupported");
  seen.add(node);
  if (node.const !== undefined) return node.const;
  if (node.oneOf !== undefined) return representative(node.oneOf[0], seen);
  if (node.type === "object") {
    return Object.fromEntries((node.required ?? []).map((name) => {
      const property = node.properties?.[name];
      if (property === undefined) fail(`required property ${name} has no schema`);
      return [name, representative(property, new Set(seen))];
    }));
  }
  if (node.type === "array") return [];
  if (node.type === "integer" || node.type === "number") return 0;
  if (node.type === "boolean") return false;
  return "";
};

const definitions = Object.entries(schema.$defs);
if (definitions.length === 0) fail("schema has no representative definitions");
const selected = kind === "sum"
  ? definitions.find(([, definition]) => containsOneOf(definition))
  : definitions.find(([, definition]) => resolve(definition).type === "object");
if (selected === undefined) fail(`no ${kind} definition`);

const [definitionName, definition] = selected;
const instance = representative(definition);
const validate = ajv.compile({
  $schema: schema.$schema,
  $defs: schema.$defs,
  $ref: `#/$defs/${definitionName}`,
});
if (!validate(instance)) fail(`representative ${kind} instance failed: ${ajv.errorsText(validate.errors)}`);

console.log(`SCHEMA METASCHEMA PASS; INSTANCE PASS ${kind} (${definitionName})`);
