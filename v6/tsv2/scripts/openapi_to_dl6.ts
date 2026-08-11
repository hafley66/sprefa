// openapi_to_dl6.ts — OpenAPI 3.x document -> .dl6 text program.
// COMPONENTS only; settled mapping rows are in POKEAPI_ROUNDTRIP_REPORT.md.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// v6/tsv2/scripts/openapi_to_dl6.ts -> v6, the same two-levels-up shape
// openapi_roundtrip_check.ts uses to find prolog/.
const HERE = path.dirname(fileURLToPath(import.meta.url));
const V6 = path.resolve(HERE, "..", "..");
const COMPILE_PL = path.join(V6, "prolog", "compile.pl");

export interface IOpenApiV3 {
  openapi?: string;
  components?: { schemas?: Record<string, IJsonSchema> };
}

export interface IJsonSchema {
  type?: string | string[];
  properties?: Record<string, IJsonSchema>;
  items?: IJsonSchema;
  required?: string[];
  nullable?: boolean;
  $ref?: string;
  allOf?: IJsonSchema[];
  anyOf?: IJsonSchema[];
  oneOf?: IJsonSchema[];
}

export interface IRelColumn {
  readonly name: string;
  readonly type: string;
}

export interface IRelDecl {
  name: string;
  columns: IRelColumn[];
  /** When set, the rel is a payload enum spelled `variant(payload: rel)` entries. */
  enumVariants?: readonly string[];
}

const SCALAR: Record<string, string> = {
  string: "text",
  integer: "int",
  number: "float",
  boolean: "bool",
};

function isScalarType(t: string): boolean {
  return t === "text" || t === "int" || t === "float" || t === "bool" || t === "json";
}

// PascalCase -> snake_case; digits never start a boundary, so iso639 passes.
export function snakeCase(name: string): string {
  const out: string[] = [];
  for (let i = 0; i < name.length; i += 1) {
    const c = name[i]!;
    if (/[A-Z]/.test(c)) {
      const prev = name[i - 1];
      const next = name[i + 1];
      const boundary =
        i > 0 &&
        prev !== undefined &&
        (/[a-z0-9]/.test(prev) || (prev !== undefined && /[A-Z]/.test(prev) && next !== undefined && /[a-z]/.test(next)));
      if (boundary) out.push("_");
      out.push(c.toLowerCase());
    } else if (/[a-z0-9]/.test(c)) {
      out.push(c);
    } else {
      out.push("_");
    }
  }
  return out.join("").replace(/_+/g, "_").replace(/^_|_$/g, "");
}

/** A valid dl6 identifier for a property: snake_case, never empty. */
export function columnName(property: string): string {
  const s = snakeCase(property);
  return /^[a-z_]/.test(s) ? s : `_${s}`;
}

// The target component name for a $ref, or null when not a component ref.
export function refTarget(s: IJsonSchema): string | null {
  if (s.$ref && s.$ref.startsWith("#/components/schemas/")) {
    return s.$ref;
  }
  if (s.allOf && s.allOf.length === 1 && s.allOf[0]!.$ref) {
    return s.allOf[0]!.$ref!;
  }
  if (Array.isArray(s.type)) {
    const refs = s.type.filter((x) => typeof x === "string" && x.startsWith("#/components/schemas/"));
    if (refs.length === 1) {
      return refs[0]!;
    }
  }
  return null;
}

function isNullSchema(s: IJsonSchema | undefined): boolean {
  if (s === undefined) return false;
  if (s.type === "null") return true;
  if (Array.isArray(s.type) && s.type.length === 1 && s.type[0] === "null") return true;
  return s.type === undefined && s.nullable === true && s.$ref === undefined && s.properties === undefined && s.items === undefined;
}

// Split a property schema into its nullable flag and inner value shape.
export function unwrapNullable(s: IJsonSchema): { inner: IJsonSchema; nullable: boolean } {
  if (s.nullable === true) {
    return { inner: s, nullable: true };
  }
  if (Array.isArray(s.anyOf) && s.anyOf.some(isNullSchema)) {
    return { inner: { ...s, anyOf: undefined, nullable: undefined }, nullable: true };
  }
  if (Array.isArray(s.oneOf)) {
    const hasNull = s.oneOf.some(isNullSchema);
    if (hasNull) {
      const rest = s.oneOf.filter((m) => !isNullSchema(m));
      if (rest.length === 1) {
        return { inner: { ...rest[0]!, anyOf: undefined, oneOf: undefined, nullable: undefined }, nullable: true };
      }
      return { inner: { ...s, anyOf: undefined, nullable: undefined }, nullable: true };
    }
  }
  if (Array.isArray(s.type)) {
    const withNull = s.type.includes("null");
    if (withNull) {
      const nonNull = s.type.filter((x) => x !== "null");
      if (nonNull.length === 1) {
        return { inner: { ...s, type: nonNull[0]!, nullable: undefined }, nullable: true };
      }
      return { inner: { ...s, type: nonNull, nullable: undefined }, nullable: true };
    }
  }
  return { inner: s, nullable: false };
}

// Named error: two OpenAPI sources snake-case to one dl6 rel name.
export class OpenapiNameCollision extends Error {
  readonly collided: string;
  constructor(first: string, second: string, collided: string) {
    super(`openapi-to-dl6: rel name collision after snake_casing: "${first}" and "${second}" both map to "${collided}"`);
    this.name = "OpenapiNameCollision";
    this.collided = collided;
  }
}

// One conversion run over doc.components.schemas in document order; throws
// OpenapiNameCollision on snake_casing collisions. See report for mode rules.
export class OpenapiToDl6 {
  private readonly schemas: Record<string, IJsonSchema>;
  private readonly relNames = new Map<string, string>();
  private readonly rels: IRelDecl[] = [];
  private readonly seenNames = new Map<string, string>();
  private readonly mode: "full" | "safe" | "strict";
  private readonly gaps: string[] = [];
  private resolved?: IRelDecl[];

  constructor(doc: IOpenApiV3, mode: "full" | "safe" | "strict" = "strict") {
    this.schemas = doc.components?.schemas ?? {};
    this.mode = mode;
    for (const pascal of Object.keys(this.schemas)) {
      const snake = snakeCase(pascal);
      this.claimName(snake, pascal);
      this.relNames.set(pascal, snake);
    }
  }

  private claimName(snake: string, source: string): void {
    const prior = this.seenNames.get(snake);
    if (prior !== undefined && prior !== source) {
      throw new OpenapiNameCollision(source, prior, snake);
    }
    this.seenNames.set(snake, source);
  }

  /** Build the complete program text. */
  convert(): string {
    const output = this.resolve();
    const order = [...output].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    const lines = order.map(relLine);
    return lines.join("\n") + (lines.length ? "\n" : "");
  }

  private resolve(): IRelDecl[] {
    if (this.resolved === undefined) {
      this.rels.length = 0;
      for (const pascal of Object.keys(this.schemas)) {
        this.rels.push(this.buildRel(this.relNameOf(pascal), this.schemas[pascal]!));
      }
      this.resolved =
        this.mode === "safe"
          ? this.applySafeFalls()
          : this.mode === "strict"
            ? this.applyStrictFalls()
            : this.rels;
    }
    return this.resolved;
  }

  /** The declared rels (component + lifted + enum), in emit order, after the
   * mode's fallbacks. */
  get declaredRels(): readonly IRelDecl[] {
    return this.resolve();
  }
  /** Compiler gap attribution rows, mode 'safe' / 'strict'. */
  get gapList(): readonly string[] {
    return [...this.gaps];
  }

  // Spread the current compiler's safe subset over the full mapping output,
  // attributing each rewrite to a gap (see POKEAPI_ROUNDTRIP_REPORT.md).
  private applySafeFalls(): IRelDecl[] {
    const keep = this.rels.filter((r) => r.enumVariants === undefined);
    const optionBearer = new Set<string>();
    for (const r of keep) {
      if (r.columns.some((c) => /^option\(/.test(c.type))) {
        optionBearer.add(r.name);
      }
    }
    const out: IRelDecl[] = [];
    for (const r of keep) {
      const cols = r.columns.map((c) => {
        let t = c.type;
        const rewrite = (col: string, newT: string): string => {
          this.gaps.push(`${r.name}.${col}: ${c.type} -> ${newT}`);
          return newT;
        };
        if (t.startsWith("list(")) {
          t = rewrite(c.name, "json"); // G1
          return { name: c.name, type: t };
        }
        const mo = /^option\((.*)\)$/.exec(t);
        if (mo) {
          const inner = mo[1]!;
          // option(ref) where ref is an option-bearer: drop to json
          if (optionBearer.has(inner) && !isScalarType(inner)) {
            return { name: c.name, type: rewrite(c.name, "json") };
          }
          return { name: c.name, type: t };
        }
        // bare ref to an option-bearer: drop to json (ref-target refused)
        if (optionBearer.has(t) && !isScalarType(t)) {
          return { name: c.name, type: rewrite(c.name, "json") };
        }
        return { name: c.name, type: t };
      });
      out.push({ name: r.name, columns: cols });
    }
    return out;
  }

  // Which generic columns a ref target may keep is decided BY COMPILING, never
  // by a shape rule: probe the rel, then probe each generic column on its own.
  private applyStrictFalls(): IRelDecl[] {
    const keep = this.rels.filter((r) => r.enumVariants === undefined);
    const enums = this.rels.filter((r) => r.enumVariants !== undefined);
    const byName = new Map(keep.map((r) => [r.name, r]));
    const refTarget = new Set<string>();
    for (const r of keep) {
      for (const col of r.columns) {
        for (const ref of columnRelRefs(col.type, byName)) refTarget.add(ref);
      }
    }
    const candidates = new Set<string>();
    for (const t of refTarget) {
      const r = byName.get(t);
      if (r && r.columns.some((c) => isGenericType(c.type))) candidates.add(t);
    }
    const bad = probeRefTargets(candidates, byName);
    const drop = confirmNarrowing(probeGuiltyColumns(bad, byName), byName);
    const out = keep.map((r) => {
      const dropped = drop.get(r.name);
      const cols = r.columns.map((c) => {
        if (dropped !== undefined && dropped.has(c.name)) {
          this.gaps.push(`${r.name}.${c.name}: ${c.type} -> json (0_program_check.pl:342)`);
          return { name: c.name, type: "json" };
        }
        return c;
      });
      return { name: r.name, columns: cols };
    });
    return [...out, ...enums];
  }

  private relNameOf(pascal: string): string {
    return this.relNames.get(pascal) ?? snakeCase(pascal);
  }

  private resolveRef(target: string): string {
    const pascal = target.replace(/^#\/components\/schemas\//, "");
    const snake = this.relNames.get(pascal) ?? snakeCase(pascal);
    this.relNames.set(pascal, snake);
    return snake;
  }

  private buildRel(relName: string, schema: IJsonSchema): IRelDecl {
    const columns: IRelColumn[] = [];
    const props = schema.properties ?? {};
    for (const prop of Object.keys(props)) {
      columns.push({ name: columnName(prop), type: this.propertyType(relName, prop, props[prop]!) });
    }
    return { name: relName, columns };
  }

  private propertyType(parent: string, prop: string, schema: IJsonSchema): string {
    const { inner, nullable } = unwrapNullable(schema);
    const base = this.emitShape(parent, prop, inner);
    if (nullable) {
      if (!base.startsWith("option(")) {
        return `option(${base})`;
      }
    }
    return base;
  }

  private emitShape(parent: string, prop: string, s: IJsonSchema): string {
    const ref = refTarget(s);
    if (ref !== null) {
      return this.resolveRef(ref);
    }
    if (s.oneOf) {
      const members = s.oneOf;
      const payloads = members.map(refTarget).filter((x): x is string => x !== null);
      if (payloads.length >= 2 && payloads.length === members.length) {
        return this.payloadEnumType(parent, prop, payloads);
      }
    }
    const t = Array.isArray(s.type) ? s.type[0] : s.type;
    if (t === "array") {
      const item = s.items ?? {};
      const itemRef = refTarget(item);
      if (itemRef !== null) {
        // G1: array of component refs
        if (this.mode === "safe") {
          this.gaps.push(`${parent}.${prop}: array(ref) -> json`);
          return "json";
        }
        return `list(${this.resolveRef(itemRef)})`;
      }
      const it = Array.isArray(item.type) ? item.type[0] : item.type;
      if (typeof it === "string" && SCALAR[it] !== undefined) {
        return `json_list(${SCALAR[it]})`;
      }
      if (item.properties !== undefined) {
        if (this.mode === "safe") {
          this.gaps.push(`${parent}.${prop}: array(inline) -> json`);
          return "json";
        }
        return `list(${this.liftObject(parent, prop, item)})`;
      }
      return "json";
    }
    if ((t === "object" && s.properties !== undefined) || (s.properties !== undefined && t === undefined)) {
      if (this.mode === "safe") {
        this.gaps.push(`${parent}.${prop}: inline object -> json`);
        return "json";
      }
      return this.liftObject(parent, prop, s);
    }
    if (typeof t === "string" && SCALAR[t] !== undefined) {
      return SCALAR[t]!;
    }
    return "json";
  }

  /** Mint a lifted inline-object rel `<parent>__<prop>` carrying the object's
   * own columns, and return its name. Nested inline objects lift recursively. */
  private liftObject(parent: string, prop: string, objectSchema: IJsonSchema): string {
    const name = `${parent}__${columnName(prop)}`;
    this.claimName(name, `lifted@${parent}#${prop}`);
    const lifted = this.buildRel(name, objectSchema);
    this.rels.push(lifted);
    return name;
  }

  private payloadEnumType(parent: string, prop: string, targets: string[]): string {
    const name = `${parent}__${columnName(prop)}`;
    this.claimName(name, `enum@${parent}#${prop}`);
    const variants = targets.map((t, i) => {
      const v = `variant_${String(i + 1)}`;
      return `${v}(payload: ${this.resolveRef(t)})`;
    });
    this.rels.push({ name, columns: [], enumVariants: variants });
    return name;
  }
}

function relLine(rel: IRelDecl): string {
  if (rel.enumVariants !== undefined) {
    return `rel ${rel.name}(${rel.enumVariants.join(" ; ")}).`;
  }
  return `rel ${rel.name}(${rel.columns.map((c) => `${c.name}: ${c.type}`).join(", ")}).`;
}

// Peels option()/list() to any depth, scoped to names byName declares, so a
// probe file declares every placeholder the column under test names.
function columnRelRefs(type: string, byName: ReadonlyMap<string, IRelDecl>): string[] {
  const inner = /^(?:option|list)\((.*)\)$/.exec(type);
  if (inner && inner[1]) return columnRelRefs(inner[1], byName);
  if (/^[a-z_][a-z0-9_]*$/.test(type) && byName.has(type)) return [type];
  return [];
}

// One candidate as a ref target with its real columns; any rel its columns
// name gets a scalar stand-in, since only the target's own shape is under test.
function buildRefTargetProbe(target: IRelDecl, byName: ReadonlyMap<string, IRelDecl>): string {
  const placeholders = new Set<string>();
  for (const col of target.columns) {
    for (const ref of columnRelRefs(col.type, byName)) {
      if (ref !== target.name) placeholders.add(ref);
    }
  }
  const lines = [...placeholders].map((name) => `rel ${name}(probe_value: int).`);
  lines.push(relLine(target));
  // Double-underscore rel names throw reserved_rel_namespace before the type plane runs.
  lines.push(`rel probe_ref_holder(probe_value: ${target.name}).`);
  return lines.join("\n") + "\n";
}

interface IProbeSpec {
  readonly id: string;
  readonly source: string;
}

// One swipl process runs a forall/catch loop over compile_dl6/2, one probe per
// spec; the ids that compiled come back. An infrastructure failure returns none.
function runProbes(specs: readonly IProbeSpec[]): Set<string> {
  if (specs.length === 0) return new Set();
  let scratchDir: string;
  try {
    scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), "dl6-strict-probe-"));
  } catch {
    return new Set();
  }
  try {
    const files = specs.map((spec, index) => ({
      id: spec.id,
      file: path.join(scratchDir, `probe_${String(index)}.dl6`),
      out: path.join(scratchDir, `probe_${String(index)}.out.ts`),
    }));
    for (let index = 0; index < specs.length; index += 1) {
      fs.writeFileSync(files[index]!.file, specs[index]!.source);
    }
    const members = files.map((f, i) => `id(${String(i)},'${f.file}','${f.out}')`).join(",");
    const goal =
      `forall(member(id(Id,In,Out),[${members}]),` +
      `catch((compile_dl6(In,Out),format('PROBE_OK ~w~n',[Id])),_,format('PROBE_FAIL ~w~n',[Id])))`;
    const cp = spawnSync("swipl", ["-q", "-l", COMPILE_PL, "-g", goal, "-g", "halt"], { encoding: "utf8", timeout: 60000 });
    if (cp.error || cp.status !== 0) return new Set();
    const passed = new Set<string>();
    for (const line of (cp.stdout ?? "").split("\n")) {
      const m = /^PROBE_OK (\d+)$/.exec(line.trim());
      if (m && m[1]) passed.add(files[Number(m[1])]!.id);
    }
    return passed;
  } catch {
    return new Set();
  } finally {
    fs.rmSync(scratchDir, { recursive: true, force: true });
  }
}

function isGenericType(type: string): boolean {
  return /^(option|list)\(/.test(type);
}

function withColumnsAsJson(rel: IRelDecl, drop: ReadonlySet<string>): IRelDecl {
  return { name: rel.name, columns: rel.columns.map((c) => (drop.has(c.name) ? { name: c.name, type: "json" } : c)) };
}

function probeRefTargets(candidates: ReadonlySet<string>, byName: ReadonlyMap<string, IRelDecl>): Set<string> {
  const rels = [...candidates].map((name) => byName.get(name)).filter((r): r is IRelDecl => r !== undefined);
  const passed = runProbes(rels.map((r) => ({ id: r.name, source: buildRefTargetProbe(r, byName) })));
  return new Set(rels.filter((r) => !passed.has(r.name)).map((r) => r.name));
}

// Which generic columns of a bad ref target actually stop the compiler: each is
// probed alone, every other generic column of the same rel rewritten to json.
function probeGuiltyColumns(bad: ReadonlySet<string>, byName: ReadonlyMap<string, IRelDecl>): Map<string, Set<string>> {
  const specs: IProbeSpec[] = [];
  const generics = new Map<string, string[]>();
  for (const name of bad) {
    const rel = byName.get(name);
    if (rel === undefined) continue;
    const cols = rel.columns.filter((c) => isGenericType(c.type)).map((c) => c.name);
    generics.set(name, cols);
    for (const col of cols) {
      const others = new Set(cols.filter((c) => c !== col));
      specs.push({ id: `${name} ${col}`, source: buildRefTargetProbe(withColumnsAsJson(rel, others), byName) });
    }
  }
  const passed = runProbes(specs);
  const guilty = new Map<string, Set<string>>();
  for (const [name, cols] of generics) {
    guilty.set(name, new Set(cols.filter((col) => !passed.has(`${name} ${col}`))));
  }
  return guilty;
}

// A narrowed rel that still fails falls back to dropping every generic column,
// so an unmodelled column interaction can never widen what the converter emits.
function confirmNarrowing(guilty: Map<string, Set<string>>, byName: ReadonlyMap<string, IRelDecl>): Map<string, Set<string>> {
  const specs: IProbeSpec[] = [];
  for (const [name, drop] of guilty) {
    const rel = byName.get(name);
    if (rel === undefined) continue;
    specs.push({ id: name, source: buildRefTargetProbe(withColumnsAsJson(rel, drop), byName) });
  }
  const passed = runProbes(specs);
  const confirmed = new Map<string, Set<string>>();
  for (const [name, drop] of guilty) {
    const rel = byName.get(name);
    if (rel === undefined) continue;
    confirmed.set(
      name,
      passed.has(name) ? drop : new Set(rel.columns.filter((c) => isGenericType(c.type)).map((c) => c.name)),
    );
  }
  return confirmed;
}
