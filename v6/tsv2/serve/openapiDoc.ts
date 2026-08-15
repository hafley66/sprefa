import type { IRelCatalogRow, IServedProgram } from "../runtime/types.ts";

interface SchemaObject {
  readonly [key: string]: unknown;
}

interface Operation {
  readonly method: string;
  readonly path: string;
  readonly operationId: string;
  readonly summary: string;
  readonly pathParam?: string;
  readonly errorStatuses: readonly number[];
}

const OPERATIONS: readonly Operation[] = [
  { method: "post", path: "/program", operationId: "loadProgram", summary: "compile and load a DL6 program", errorStatuses: [400] },
  { method: "post", path: "/edb/events", operationId: "postArrivals", summary: "submit signed EDB arrivals", errorStatuses: [400, 409] },
  { method: "get", path: "/idb/:rel", operationId: "readRelation", summary: "read one relation snapshot", pathParam: "rel", errorStatuses: [404] },
  { method: "get", path: "/ticks", operationId: "streamTicks", summary: "stream tick events as SSE", errorStatuses: [404] },
  { method: "get", path: "/stats", operationId: "readStats", summary: "read process memory and SQLite storage statistics", errorStatuses: [404] },
  { method: "get", path: "/openapi.json", operationId: "readOpenapi", summary: "read the loaded program OpenAPI document", errorStatuses: [] },
];

function pathTemplate(path: string): string {
  return path
    .split("/")
    .map((segment) => (segment.startsWith(":") ? `{${segment.slice(1)}}` : segment))
    .join("/");
}

function primitiveSchema(kind: string): SchemaObject {
  switch (kind) {
    case "int":
      return { type: "integer" };
    case "float":
      return { type: "number" };
    case "text":
      return { type: "string" };
    case "bool":
      return { type: "boolean" };
    default:
      // json: untyped by design.
      return {};
  }
}

function relPath(row: IRelCatalogRow, byId: ReadonlyMap<number, IRelCatalogRow>): string {
  const parts: string[] = [row.local_name];
  let parent = byId.get(row.parent_id);
  while (parent !== undefined && parent.kind === "rel") {
    parts.unshift(parent.local_name);
    parent = byId.get(parent.parent_id);
  }
  return parts.join(".");
}

function schemaForTypeId(rows: readonly IRelCatalogRow[], byId: ReadonlyMap<number, IRelCatalogRow>, typeId: number): SchemaObject {
  const target = byId.get(typeId);
  if (target === undefined) return {};
  switch (target.kind) {
    case "primitive":
      return primitiveSchema(target.local_name);
    case "json_list":
    case "list":
      return { type: "array", items: schemaForTypeId(rows, byId, target.type_id) };
    case "rel":
      return { $ref: `#/components/schemas/${relPath(target, byId)}` };
    default:
      return {};
  }
}

function relObject(row: IRelCatalogRow, rows: readonly IRelCatalogRow[], byId: ReadonlyMap<number, IRelCatalogRow>): SchemaObject {
  const properties: Record<string, SchemaObject> = {};
  const required: string[] = [];
  const columns = rows
    .filter((candidate) => candidate.parent_id === row.rel_id && candidate.kind === "column")
    .sort((left, right) => left.ordinal - right.ordinal);
  for (const column of columns) {
    const schema = schemaForTypeId(rows, byId, column.type_id);
    properties[column.local_name] = schema;
    if (!("anyOf" in schema)) required.push(column.local_name);
  }
  return { type: "object", properties, required, additionalProperties: false };
}

function schemasFor(rows: readonly IRelCatalogRow[], moduleId: number | undefined): Record<string, SchemaObject> {
  const byId = new Map<number, IRelCatalogRow>();
  for (const row of rows) byId.set(row.rel_id, row);
  const schemas: Record<string, SchemaObject> = {};
  if (moduleId === undefined) return schemas;
  for (const row of rows) {
    if (row.kind === "rel" && row.module_id === moduleId) {
      schemas[relPath(row, byId)] = relObject(row, rows, byId);
    }
  }
  return schemas;
}

function pathsObject(): Record<string, Record<string, SchemaObject>> {
  const byPath = new Map<string, Record<string, SchemaObject>>();
  for (const operation of OPERATIONS) {
    const template = pathTemplate(operation.path);
    let item = byPath.get(template);
    if (item === undefined) {
      item = {};
      byPath.set(template, item);
    }
    const responses: Record<string, SchemaObject> = { 200: { description: operation.summary } };
    for (const status of operation.errorStatuses) responses[status] = { description: `HTTP ${status}` };
    const body: SchemaObject = { operationId: operation.operationId, summary: operation.summary, responses };
    item[operation.method] =
      operation.pathParam === undefined
        ? body
        : { ...body, parameters: [{ name: operation.pathParam, in: "path", required: true, schema: { type: "string" } }] };
  }
  return Object.fromEntries(byPath);
}

export function buildOpenapi(program: IServedProgram | null): SchemaObject {
  const rows = program?.rel_catalog ?? [];
  const moduleRow = rows.find((row) => row.kind === "module");
  return {
    openapi: "3.1.0",
    info: {
      title: "sprefa tsv2 served engine",
      version: "6.2.0",
      description:
        "The served tsv2 datalog engine (v6/tsv2/serve). One program at a time; POST /program swaps it. Every response body is JSON except GET /ticks, which is an SSE stream of canonical tick-log lines.",
    },
    servers: [{ url: "http://127.0.0.1:17500", description: "bop serve default port" }],
    paths: pathsObject(),
    components: { schemas: schemasFor(rows, moduleRow?.module_id) },
  };
}
