/** The G9 classifier: canonicalize away the §10.2 interning classes, then
 *  demand byte equality. What the canonicalizer cannot explain is the finding. */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

type IClassName =
  | "dictionary-ddl"
  | "decode-view"
  | "decode-read"
  | "decode-subquery"
  | "column-storage"
  | "ir-encoding"
  | "door-plan"
  | "door-call"
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

const counts = new Map<IClassName, IInternClassCount>();

function count(name: IClassName, by = 1): void {
  const row = counts.get(name) ?? { name, count: 0 };
  row.count += by;
  counts.set(name, row);
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
  for (const line of text.split("\n")) {
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
