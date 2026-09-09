// Adapt authored nested wire shapes to SQL core and rusqlite value writers.
// Spans stay models in TypeSpec; a nested path becomes e.g. span__start in SQL.
export function emitFacts(program, emitSQL, emitRusqliteValueWriters, rustTarget, emitAll) {
  const ns = program.getGlobalNamespaceType().namespaces.get("ExtractSql");
  const scalars = program.getGlobalNamespaceType().namespaces.get("TypeSpec").scalars;
  const tables = [];
  const rustModels = [];
  const quote = name => `"${name.replaceAll('"', '""')}"`;
  const supported = new Set(["string", "boolean", "uint32", "uint64", "int32", "int64"]);

  function jsonType(type) {
    switch (type.kind) {
      case "Scalar":
        if (!supported.has(type.name)) throw new Error(`Unsupported JSON scalar: ${type.name}`);
        return { kind: type.name };
      case "Tuple": return { kind: "tuple", items: type.values.map(jsonType) };
      case "Union": return { kind: "union", items: [...type.variants.values()].map(v => jsonType(v.type)) };
      case "Model": return type.indexer
        ? { kind: "array", items: [jsonType(type.indexer.value)] }
        : { kind: "object", properties: [...type.properties.values()].map(p => ({ name: p.name, type: jsonType(p.type) })) };
      default: throw new Error(`Unsupported JSON type: ${type.kind}`);
    }
  }

  function fields(model, prefix = [], optional = false, nullable = false) {
    return [...model.properties.values()].flatMap(property => {
      const path = [...prefix, property.name];
      let type = property.type;
      let allowsNull = nullable;
      const allowsMissing = optional || property.optional;
      if (type.kind === "Union") {
        const variants = [...type.variants.values()].map(v => v.type);
        const nonNull = variants.filter(t => !(t.kind === "Intrinsic" && t.name === "null"));
        if (nonNull.length !== 1 || nonNull.length === variants.length) {
          throw new Error(`Unsupported SQL union: ${path.join(".")}`);
        }
        type = nonNull[0];
        allowsNull = true;
      }
      const json = type.kind === "Model" && type.indexer ? jsonType(type) : undefined;
      if (type.kind === "Model" && !json) return fields(type, path, allowsMissing, allowsNull);
      const literal = type.kind === "String" ? type.value : undefined;
      const values = type.kind === "Enum" ? [...type.members.values()].map(m => m.value ?? m.name) : undefined;
      const kind = literal !== undefined || values ? "string" : type.name === "Json" || json ? "json" : type.name;
      if (literal === undefined && !values && !json && type.kind !== "Scalar" || !supported.has(kind) && kind !== "json") {
        throw new Error(`Unsupported SQL field: ${path.join(".")} (${type.kind})`);
      }
      return [{ name: path.join("__"), path, kind, optional: allowsMissing,
        nullable: allowsNull, ...(literal !== undefined ? { literal } : {}),
        ...(values ? { values } : {}), ...(json ? { json_type: json } : {}), property }];
    });
  }

  for (const [name, model] of [...ns.models]) {
    const record = model.properties.get("record")?.type;
    if (record?.kind !== "String") {
      ns.models.delete(name);
      continue;
    }
    const columns = fields(model);
    if (new Set(columns.map(c => c.name)).size !== columns.length) {
      throw new Error(`SQL column collision in ${name}`);
    }
    tables.push({ record: record.value, table: record.value,
      columns: columns.map(({ property, ...column }) => column) });
    rustModels.push({ kind: "model", name, fields: columns.map(c => ({
      name: c.name,
      type: { kind: "scalar", name: c.kind === "json" ? "string" : c.kind },
      optional: c.optional || c.nullable,
    })) });

    // This program is private to the generation pass. Decorator state stays on
    // the original PK property; flattened leaves need no new decorator state.
    model.name = record.value;
    model.properties.clear();
    for (const c of columns) {
      const prop = c.path.length === 1 ? c.property : { ...c.property };
      prop.name = c.name;
      prop.type = scalars.get(c.kind === "json" ? "string" : c.kind);
      if (c.optional || c.nullable) {
        prop.type = { kind: "Union", variants: new Map([
          ["value", { type: prop.type }],
          ["null", { type: { kind: "Intrinsic", name: "null" } }],
        ]) };
      }
      model.properties.set(prop.name, prop);
    }
  }
  const writers = emitRusqliteValueWriters(program);
  for (const [, model] of [...ns.models]) {
    model.name = quote(model.name);
    const properties = [...model.properties.values()];
    model.properties.clear();
    for (const prop of properties) {
      prop.name = quote(prop.name);
      const replaceUint64 = type => {
        if (type.kind === "Scalar" && type.name === "uint64") return scalars.get("bytes");
        if (type.kind !== "Union") return type;
        return { ...type, variants: new Map([...type.variants].map(([name, variant]) => [
          name, { ...variant, type: replaceUint64(variant.type) },
        ])) };
      };
      prop.type = replaceUint64(prop.type);
      model.properties.set(prop.name, prop);
    }
  }
  const rawIdentifiers = new Set(["type", "ref", "const", "fn", "impl", "loop", "trait", "mod", "use", "match", "self", "in", "move", "where", "as", "async", "await"]);
  const target = { ...rustTarget, fieldName(name) {
    const mapped = rustTarget.fieldName(name);
    return rawIdentifiers.has(mapped) ? `r#${mapped}` : mapped;
  } };
  const rust = emitAll(rustModels, target).map(d => d.code.replace(/^use .*;\n/gm, "").trim()).join("\n\n");
  return new Map([
    ["4_facts.sql", "-- Generated from schema/1_facts.tsp by just gen.\n" + emitSQL(program)],
    ["5_facts.json", JSON.stringify(tables, null, 2) + "\n"],
    ["6_facts.rs", "// Generated SQLite row types from schema/1_facts.tsp.\nuse serde::{Serialize, Deserialize};\n\n" + rust + "\n"],
    ["7_writers_auto.rs", writers],
  ]);
}
