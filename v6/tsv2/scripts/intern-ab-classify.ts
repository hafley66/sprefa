/** The G9 classifier: canonicalize away the §10.2 interning classes, then
 *  demand byte equality. What the canonicalizer cannot explain is the finding. */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

type IClassName =
  | "dictionary-ddl"
  | "decode-view"
  | "decode-read"
  | "decode-subquery"
  | "value-decode"
  | "column-storage"
  | "ir-encoding"
  | "door-plan"
  | "door-call"
  | "literal-id"
  | "built-id"
  | "intern-write"
  | "literal-seed"
  | "seed-id"
  | "mode-stamp";

interface IInternClassCount {
  readonly name: IClassName;
  count: number;
}

interface IModuleVerdict {
  readonly module: string;
  readonly unexplained: readonly string[];
}

const DROP_LINE: readonly (readonly [RegExp, IClassName])[] = [
  [/CREATE TABLE "__str" \(/, "dictionary-ddl"],
  [/CREATE TEMP VIEW "__txt_/, "decode-view"],
  [/^import \{ TextPlane \} from/, "door-plan"],
  [/^ {2}ITextInternPlan,$/, "door-plan"],
  [/TextPlane\.intern\(/, "door-call"],
  [/^ +\.pipe\(map\(\(interned\) =>/, "door-call"],
];

/** The emitted plan constant is a BLOCK, so it needs a state machine rather
 *  than a line pattern: one class, however many lines it spans. */
const DOOR_PLAN_OPEN = /^export const TEXT_INTERN_PLAN/;
const DOOR_PLAN_CLOSE = /^\};$/;

/** A seeded literal may itself contain a newline, so the DDL entry spans as
 *  many lines as the literal does and ends at the template's own close. */
const LITERAL_SEED = /INSERT OR IGNORE INTO "__str" \("content"\) VALUES/;

/** §5.7.1 statement one, emitted beside the arm's own insert. `json_each` is the
 *  door's plan line, which the door-plan state machine has already taken. */
const INTERN_WRITE = /INSERT OR IGNORE INTO "__str" \("content"\) SELECT/;

/** An unbalanced backtick count means the template is still open. */
function templateStaysOpen(line: string): boolean {
  return ((line.match(/`/g) ?? []).length & 1) === 1;
}

const counts = new Map<IClassName, IInternClassCount>();

function count(name: IClassName, by = 1): void {
  const row = counts.get(name) ?? { name, count: 0 };
  row.count += by;
  counts.set(name, row);
}

const ID_LOOKUP_OPEN = `(SELECT s."__id" FROM "__str" s WHERE s."content" = `;

/** A `'` never appears inside an emitted literal (lower.pl:sql_literal/2 refuses
 *  one), so a quoted run is delimited and its parens are not structure. */
function matchingClose(text: string, open: number): number {
  let depth = 0;
  let quoted = false;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (character === "'") quoted = !quoted;
    else if (!quoted && character === "(") depth += 1;
    else if (!quoted && character === ")") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

/** The id lookup wraps a balanced expression (a literal for a constant, an
 *  arbitrary one for a built string), so a regex cannot find its end. */
function stripIdLookups(text: string): string {
  let out = "";
  let index = 0;
  for (;;) {
    const start = text.indexOf(ID_LOOKUP_OPEN, index);
    if (start === -1) return out + text.slice(index);
    const end = matchingClose(text, start);
    if (end === -1) return out + text.slice(index);
    const inner = text.slice(start + ID_LOOKUP_OPEN.length, end);
    count(inner.startsWith("'") ? "literal-id" : "built-id");
    out += text.slice(index, start) + stripIdLookups(inner);
    index = end + 1;
  }
}

/** A seeded literal may carry a newline, so the id lookup that names it is not
 *  a line-shaped thing; these run over the whole module first. */
function canonicalDocument(text: string): string {
  let out = stripIdLookups(text);

  const seedSlots = out.match(/\(SELECT "__id" FROM "__str" WHERE "content" = \?\)/g);
  if (seedSlots !== null) count("seed-id", seedSlots.length);
  return out.replace(/\(SELECT "__id" FROM "__str" WHERE "content" = \?\)/g, "?");
}

/** Every rewrite here is one named class; a line that still differs after all
 *  of them is a change interning does not explain. */
function canonicalLine(line: string): string {
  let out = line;
  const readSwaps = out.match(/FROM "__txt_/g);
  if (readSwaps !== null) count("decode-read", readSwaps.length);
  out = out.replace(/FROM "__txt_([^"]+)"/g, 'FROM "$1"');

  const subqueries = out.match(/\(SELECT s\."content" FROM "__str" s WHERE s\."__id" = t\."[^"]+"\)/g);
  if (subqueries !== null) count("decode-subquery", subqueries.length);
  out = out.replace(/\(SELECT s\."content" FROM "__str" s WHERE s\."__id" = (t\."[^"]+")\)/g, "$1");

  // Rule ONE's other half: a text column read under `value` demand, so the
  // operand is a body alias or a trigger placeholder rather than the view's `t`.
  const valueDecodes = out.match(/\(SELECT s\."content" FROM "__str" s WHERE s\."__id" = [^()']+?\)/g);
  if (valueDecodes !== null) count("value-decode", valueDecodes.length);
  out = out.replace(/\(SELECT s\."content" FROM "__str" s WHERE s\."__id" = ([^()']+?)\)/g, "$1");

  const IR_TEXT_DICT = /type: "text", storage: "integer", collation: null, encoding: \{ kind: "dict", rel: "__str" \}/g;
  const IR_TEXT_DIRECT = /type: "text", storage: "text", collation: "binary", encoding: \{ kind: "direct" \}/g;
  const IR_TEXT_CANONICAL = 'type: "text", storage: SCALAR, collation: COLLATION, encoding: ENCODING';
  for (const pattern of [IR_TEXT_DICT, IR_TEXT_DIRECT]) {
    const hits = out.match(pattern);
    if (hits !== null) count("ir-encoding", hits.length);
    out = out.replace(pattern, IR_TEXT_CANONICAL);
  }

  if (/internMode: "(dict|direct)"/.test(out)) count("mode-stamp");
  out = out.replace(/internMode: "(dict|direct)"/, 'internMode: "MODE"');

  // A stored column is INTEGER or TEXT; the flip IS the interning, so both
  // collapse to one token and any OTHER storage change survives.
  out = out.replace(/"([^"]+)" (INTEGER|TEXT) NOT NULL/g, '"$1" SCALAR NOT NULL');
  return out;
}

function canonicalText(text: string): readonly string[] {
  const kept: string[] = [];
  let insideDoorPlan = false;
  let insideLiteralSeed = false;
  let insideInternWrite = false;
  for (const line of canonicalDocument(text).split("\n")) {
    if (insideLiteralSeed) {
      count("literal-seed");
      if (templateStaysOpen(line)) insideLiteralSeed = false;
      continue;
    }
    if (LITERAL_SEED.test(line)) {
      count("literal-seed");
      if (templateStaysOpen(line)) insideLiteralSeed = true;
      continue;
    }
    if (insideDoorPlan) {
      count("door-plan");
      if (DOOR_PLAN_CLOSE.test(line)) insideDoorPlan = false;
      continue;
    }
    if (DOOR_PLAN_OPEN.test(line)) {
      count("door-plan");
      insideDoorPlan = true;
      continue;
    }
    if (insideInternWrite) {
      count("intern-write");
      if (templateStaysOpen(line)) insideInternWrite = false;
      continue;
    }
    if (INTERN_WRITE.test(line)) {
      count("intern-write");
      if (templateStaysOpen(line)) insideInternWrite = true;
      continue;
    }
    const matched = DROP_LINE.find(([pattern]) => pattern.test(line));
    if (matched !== undefined) {
      count(matched[1]);
      continue;
    }
    // Blank lines are section separators; dropping a whole section leaves one
    // behind, and this gate reads content, not layout.
    if (line.trim() === "") continue;
    kept.push(canonicalLine(line));
  }
  return kept;
}

function verdictFor(module: string, dictText: string, directText: string): IModuleVerdict {
  const dictLines = canonicalText(dictText);
  const directLines = canonicalText(directText);
  const unexplained: string[] = [];
  const rows = Math.max(dictLines.length, directLines.length);
  for (let index = 0; index < rows; index += 1) {
    const dictLine = dictLines[index] ?? "<missing>";
    const directLine = directLines[index] ?? "<missing>";
    if (dictLine !== directLine) unexplained.push(`${module}:${index + 1}\n  dict:   ${dictLine}\n  direct: ${directLine}`);
  }
  return { module, unexplained };
}

/** G11: at intern(dict) EVERY text column is an id. A `TEXT NOT NULL` column
 *  def outside json, or an IR text column that is not `dict("__str")`, is a
 *  finding, whatever the A/B says. */
function uniformityFindings(module: string, dictText: string): readonly string[] {
  const findings: string[] = [];
  // The dictionary's own `content` column is the one TEXT storage interning
  // creates; scanning it would report the mechanism as its own violation.
  const withoutDictionary = dictText.split("\n").filter((line) => !line.includes(`CREATE TABLE "__str" (`)).join("\n");
  for (const match of withoutDictionary.matchAll(/"([^"]+)" TEXT NOT NULL(?! CHECK \(json_valid)/g)) {
    findings.push(`${module}: text-storage column "${match[1]}" survived intern(dict)`);
  }
  for (const match of dictText.matchAll(/type: "text", storage: "([a-z]+)", collation: ([^,]+), encoding: (\{[^}]*\})/g)) {
    if (match[1] === "integer" && match[2] === "null" && match[3] === `{ kind: "dict", rel: "__str" }`) continue;
    findings.push(`${module}: IR text column reports ${match[1]}/${match[2]}/${match[3]}`);
  }
  return findings;
}

function main(): number {
  const [dictDir, directDir] = process.argv.slice(2);
  if (dictDir === undefined || directDir === undefined) {
    process.stdout.write("usage: intern-ab-classify.ts <dictDir> <directDir>\n");
    return 2;
  }
  const dictModules = readdirSync(dictDir).filter((name) => name.endsWith(".ts")).sort();
  const directModules = readdirSync(directDir).filter((name) => name.endsWith(".ts")).sort();
  const findings: string[] = [];
  if (dictModules.join(",") !== directModules.join(",")) {
    findings.push(`module SET differs: dict=${dictModules.length} direct=${directModules.length}`);
  }
  for (const module of dictModules) {
    if (!directModules.includes(module)) continue;
    const verdict = verdictFor(
      module,
      readFileSync(join(dictDir, module), "utf8"),
      readFileSync(join(directDir, module), "utf8"),
    );
    findings.push(...verdict.unexplained);
    findings.push(...uniformityFindings(module, readFileSync(join(dictDir, module), "utf8")));
  }
  const classLine = [...counts.values()]
    .sort((left, right) => left.name.localeCompare(right.name))
    .map((row) => `${row.name}=${row.count}`)
    .join(" ");
  process.stdout.write(`INTERN_AB modules=${dictModules.length} ${classLine} unexplained=${findings.length}\n`);
  for (const finding of findings.slice(0, 20)) process.stdout.write(`  ${finding}\n`);
  return findings.length === 0 ? 0 : 1;
}

process.exitCode = main();
