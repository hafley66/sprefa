import fs from "node:fs";

const [schemaPath, kind] = process.argv.slice(2);
const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const fail = (message) => {
  throw new Error(`${schemaPath}: ${message}`);
};

const isRecord = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function checkSchemaShape(node, path) {
  if (!isRecord(node)) fail(`${path} must be an object`);
  if (node.$ref !== undefined && (typeof node.$ref !== "string" || !node.$ref.startsWith("#/$defs/"))) {
    fail(`${path} has an invalid local $ref`);
  }
  if (node.type !== undefined && !["object", "array", "string", "integer", "number", "boolean"].includes(node.type)) {
    fail(`${path}.type is outside the emitted metaschema subset`);
  }
  if (node.oneOf !== undefined) {
    if (!Array.isArray(node.oneOf) || node.oneOf.length === 0) fail(`${path}.oneOf must be non-empty`);
    node.oneOf.forEach((item, index) => checkSchemaShape(item, `${path}.oneOf[${index}]`));
  }
  if (node.const !== undefined && ["object", "function", "symbol"].includes(typeof node.const)) {
    fail(`${path}.const must be JSON data`);
  }
  if (node.properties !== undefined) {
    if (!isRecord(node.properties)) fail(`${path}.properties must be an object`);
    for (const [name, property] of Object.entries(node.properties)) checkSchemaShape(property, `${path}.properties.${name}`);
  }
  if (node.required !== undefined) {
    if (!Array.isArray(node.required) || node.required.some((name) => typeof name !== "string")) fail(`${path}.required must be strings`);
  }
  if (node.items !== undefined) checkSchemaShape(node.items, `${path}.items`);
}

function containsKeyword(node, keyword) {
  node = resolveForSearch(node);
  if (keyword === "oneOf" && node.oneOf !== undefined) return true;
  if (node.oneOf?.some((item) => containsKeyword(item, keyword))) return true;
  if (node.items !== undefined && containsKeyword(node.items, keyword)) return true;
  return Object.values(node.properties ?? {}).some((item) => containsKeyword(item, keyword));
}

function resolveForSearch(node) {
  if (node.$ref === undefined) return node;
  const prefix = "#/$defs/";
  return node.$ref.startsWith(prefix) ? schema.$defs[node.$ref.slice(prefix.length)] ?? {} : {};
}

if (!isRecord(schema) || schema.$schema !== "https://json-schema.org/draft/2020-12/schema") fail("missing draft 2020-12 $schema");
if (typeof schema.$id !== "string") fail("$id must be a string");
if (!isRecord(schema.$defs)) fail("$defs must be an object");
for (const [name, definition] of Object.entries(schema.$defs)) checkSchemaShape(definition, `$defs.${name}`);

const resolve = (node) => {
  if (node.$ref === undefined) return node;
  const prefix = "#/$defs/";
  const target = node.$ref.startsWith(prefix) ? schema.$defs[node.$ref.slice(prefix.length)] : undefined;
  if (target === undefined) fail(`dangling $ref ${node.$ref}`);
  return target;
};

function representative(node) {
  node = resolve(node);
  if (node.const !== undefined) return node.const;
  if (node.oneOf !== undefined) return representative(node.oneOf[0]);
  if (node.type === "object") {
    const out = {};
    for (const name of node.required ?? []) out[name] = representative(node.properties[name]);
    return out;
  }
  if (node.type === "array") return [];
  if (node.type === "integer" || node.type === "number") return 0;
  if (node.type === "boolean") return false;
  return "";
}

function validate(node, value, path) {
  node = resolve(node);
  if (node.const !== undefined && value !== node.const) fail(`${path} does not match const`);
  if (node.oneOf !== undefined) {
    const matches = node.oneOf.filter((candidate) => {
      try { validate(candidate, value, path); return true; } catch { return false; }
    });
    if (matches.length !== 1) fail(`${path} matches ${matches.length} oneOf branches`);
    return;
  }
  if (node.type === "object") {
    if (!isRecord(value)) fail(`${path} must be an object`);
    for (const name of node.required ?? []) if (!(name in value)) fail(`${path} is missing ${name}`);
    for (const [name, child] of Object.entries(value)) {
      if (node.properties?.[name] === undefined) {
        if (node.additionalProperties === false) fail(`${path} has additional property ${name}`);
      } else validate(node.properties[name], child, `${path}.${name}`);
    }
  } else if (node.type === "array") {
    if (!Array.isArray(value)) fail(`${path} must be an array`);
    for (const [index, child] of value.entries()) validate(node.items, child, `${path}[${index}]`);
  } else if (node.type === "string" && typeof value !== "string") fail(`${path} must be a string`);
  else if (node.type === "integer" && (!Number.isInteger(value) || typeof value !== "number")) fail(`${path} must be an integer`);
  else if (node.type === "number" && (typeof value !== "number" || !Number.isFinite(value))) fail(`${path} must be a number`);
  else if (node.type === "boolean" && typeof value !== "boolean") fail(`${path} must be a boolean`);
}

const definitions = Object.values(schema.$defs);
if (definitions.length === 0) fail("schema has no representative definitions");
if (kind === "product" && !definitions.some((node) => resolve(node).type === "object")) fail("no product definition");
if (kind === "sum" && !definitions.some((node) => containsKeyword(node, "oneOf"))) fail("no sum definition");
const root = definitions.find((node) => resolve(node).type === "object") ?? definitions[0];
validate(root, representative(root), "$defs");
console.log(`SCHEMA METASCHEMA PASS; INSTANCE PASS ${kind}`);
