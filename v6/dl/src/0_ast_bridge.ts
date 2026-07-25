/**
 * 0_ast_bridge.ts — .dl text -> ast.ts Program + HostDecl[] + minted stage rels.
 *
 * Contract (plan M1, tasks.d.ts): `bridge(dlText, builtinRels) -> BridgeOk | BridgeErr`.
 * Langium parses; this file maps the Langium AST onto the store's ast.ts constructors,
 * mints the probe timecut (`h?(inputs.., outputs..)` -> __req_h rule + __resp_h EDB ref,
 * Lloyd-Topor free-variable law), rewrites literal-binding equalities (`"warn" = severity`
 * with severity otherwise unbound) into minted single-row constant rels `__lit_<n>`, and
 * applies the diag head-default law (end_line:=line, end_col:=col, hint:=null,
 * severity:="warn", code:=null when unbound). Pure: LangiumDocument per call, discarded.
 *
 * Parser services (module-level, built once): Langium's default CORE modules
 * (createDefaultCoreModule/createDefaultSharedCoreModule) + the generated grammar
 * module, wired with EmptyFileSystem — no LSP services, no cross-reference/scope
 * services (the grammar has no [Type] references; every name is a plain string the
 * bridge resolves itself against the decl tables built below).
 *
 * The four rewrites (heart of this file, in processRuleBody / buildHeadTerms):
 *   1. literal-binding: a bare literal in a head/probe-input position, or an `eq`
 *      comparison (either textual order) whose var operand is not otherwise bound,
 *      mints a single-row constant rel `__lit_<n>(value)` and a body atom referencing
 *      it, in place of an ast.ts Compare or a literal HeadTerm (neither of which
 *      exists — HeadTerm is Var|Agg only, and Compare always filters a BOUND var).
 *   2. probe minting: `h?(in.., out..)` splits into a minted `__req_h` rule (head =
 *      the input columns; body = every non-probe atom already assembled for this rule
 *      PLUS the literal-binding atoms for any literal probe input) and a minted EDB
 *      `__resp_h` rel referenced in place of the probe. Salt-arg law (M8-alpha,
 *      IdentityWitnessLaw, tasks.d.ts): a probe may pass MORE args than `h`'s
 *      declared columns: the first k (k = |inputCols|) bind inputs, the last m
 *      (m = |columns| - k) bind outputs, anything in between is a positional-only
 *      witness salt (`salt_0`..`salt_<s-1>`). Salts join the __req_h demand key
 *      alongside the inputs (`__req_h` columns = `[...inputCols, salts..]`) and are
 *      echoed back through `__resp_h` (`[...inputCols, salts.., ...outputCols]`), so
 *      a response self-describes the witness it was minted against: the "witness
 *      must EXIST and FLOW" half of the identity-vs-witness escalation; supersession
 *      (retracting a stale response when the salt changes) is a follow-up package.
 *   3. diag head-default law: an unbound diag head var at end_line/end_col/severity/
 *      code/hint gets a default (reuse line/col, or a minted literal for the rest);
 *      an unbound path/line/col/msg stays a load error.
 *   4. named-arg resolution (NamedArgLaw, tasks.d.ts, owner scope change 2026-07-24):
 *      `rel(col: term, ...)` resolves to POSITIONAL slots against the rel's declared
 *      column order before any of the three rewrites above run — `resolveNamedArgs`
 *      is the one shared function every named-arg call site (positive body atoms,
 *      negation, probes against the HOST decl's columns, query atoms, heads) funnels
 *      through. An unfilled body-atom slot becomes `wild()` (this subsumes trailing
 *      elision: a short arg list is now "the positional prefix filled, everything
 *      else unfilled", the SAME representation named args produce). An unfilled head
 *      slot re-enters rewrite 3 (diag defaults) or rewrite 1 (nothing to bind ->
 *      load error) exactly as an unbound head var already did.
 *
 * Numbering for every minted name is deterministic first-appearance order (one
 * `__req_<host>`/`__resp_<host>` per distinct host name; one `__lit_<n>` per distinct
 * literal value, in textual order of first use), so re-bridging the same text is stable.
 */

import {
  EmptyFileSystem,
  URI,
  inject,
  createDefaultCoreModule,
  createDefaultSharedCoreModule,
} from "langium";
import type { LangiumCoreServices, LangiumSharedCoreServices } from "langium";
import { DlGeneratedModule, DlGeneratedSharedModule } from "./0_generated/module.ts";
import type * as Gen from "./0_generated/ast.ts";
import {
  variable,
  literal,
  relRef,
  notRel,
  wild,
  compare,
  headVar,
  headAgg,
  edbRel,
  derivedRel,
} from "sprefa-store-engine/src/lower/ast.ts";
import type {
  Program as AstProgram,
  Rule as AstRule,
  RelDecl as AstRelDecl,
  RelRef as AstRelRef,
  BodyPred,
  Arg,
  NegArg,
  HeadTerm,
  CmpOp,
} from "sprefa-store-engine/src/lower/ast.ts";
import { buildRuleGraph, scc, stratify, NonStratifiableError } from "sprefa-store-engine/src/lower/rulegraph.ts";
import type { AssertTrue, Bridge, BridgeResult, HostDecl, LoadDiag, Retention, Value } from "./0_types.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Parser services — module scope, built once. EmptyFileSystem: the bridge never
// reads from disk; the caller hands us the full .dl text as a string.
// ─────────────────────────────────────────────────────────────────────────────

const sharedServices: LangiumSharedCoreServices = inject(
  createDefaultSharedCoreModule(EmptyFileSystem),
  DlGeneratedSharedModule,
);
const dlServices: LangiumCoreServices = inject(
  createDefaultCoreModule({ shared: sharedServices }),
  DlGeneratedModule,
);
sharedServices.ServiceRegistry.register(dlServices);

let documentCounter = 0;

/** Parse `dlText` into a fresh, throwaway LangiumDocument (per bridge() call, per
 *  the instance timeline pinned in the plan — no incremental doc services here). */
function parseDlDocument(dlText: string): { program: Gen.Program; diags: LoadDiag[] } {
  const uri = URI.parse(`memory://dl-bridge/${documentCounter++}.dl`);
  const langiumDocument = sharedServices.workspace.LangiumDocumentFactory.fromString<Gen.Program>(dlText, uri);
  const diags: LoadDiag[] = [];
  for (const lexErr of langiumDocument.parseResult.lexerErrors) {
    diags.push({ code: "parse", message: lexErr.message, line: lexErr.line ?? 0, col: lexErr.column ?? 0 });
  }
  for (const parseErr of langiumDocument.parseResult.parserErrors) {
    diags.push({
      code: "parse",
      message: parseErr.message,
      line: parseErr.token.startLine ?? 0,
      col: parseErr.token.startColumn ?? 0,
    });
  }
  return { program: langiumDocument.parseResult.value, diags };
}

// ─────────────────────────────────────────────────────────────────────────────
// Source position helper: every generated AST node has a $cstNode holding the
// exact text range it came from — real positions for diags cost nothing extra.
// ─────────────────────────────────────────────────────────────────────────────

interface Positioned {
  readonly $cstNode?: { readonly range: { readonly start: { readonly line: number; readonly character: number } } };
}

/** 1-based {line, col}, matching chevrotain's ILexingError/IToken convention (the
 *  "parse" diag code already uses 1-based positions from the lexer/parser). */
function nodePosition(node: Positioned): { line: number; col: number } {
  const start = node.$cstNode?.range.start;
  return { line: (start?.line ?? -1) + 1, col: (start?.character ?? -1) + 1 };
}

// ─────────────────────────────────────────────────────────────────────────────
// Declared-rel bookkeeping. A DeclInfo is enough to arity-check and origin-check
// any reference regardless of whether it came from the user's `rel` decls or the
// caller's builtinRels.
// ─────────────────────────────────────────────────────────────────────────────

interface DeclInfo {
  readonly columns: readonly string[];
}

/** Column type as declared: base primitive plus which wrapper (if any) wraps it. */
interface ColumnTypeInfo {
  readonly prim: "text" | "int";
  readonly wrapper: "Key" | "Min" | "Max" | undefined;
}

/** `prim` parses as a plain ID (grammar note: making "text"/"int" global keywords
 *  would forbid ever naming a column `text`), so validate it here; anything other
 *  than the two known primitives falls back to "text" (permissive — not a listed
 *  LoadDiag code this slice, and no fixture exercises a bogus type name). */
function readColumnType(type: Gen.ColumnType): ColumnTypeInfo {
  const prim = type.prim === "int" ? "int" : "text";
  if (type.$type === "WrapperType") return { prim, wrapper: type.wrapper };
  return { prim, wrapper: undefined };
}

/** A resolved column affinity (BridgeOk.columnTypes, M9 columnType flow). */
type ColumnPrim = "text" | "int";

/** Base-case column affinities for the builtin rels (spine + diag). These rels arrive
 *  as `edbRel(name, columns)` with NO declared types (5_diag.ts), so their affinity is
 *  known here from the ExtractRecord shapes (4_ingest.ts) / the v5 diag schema, not
 *  inferred. Orders match 5_diag.ts's column lists exactly. `text` columns intern to a
 *  `strings` id at storage; `int` columns store raw. */
const SPINE_COLUMN_TYPES: Readonly<Record<string, readonly ColumnPrim[]>> = {
  file: ["text", "text"],
  node: ["text", "text", "int", "int", "text", "text"],
  edge: ["text", "text", "text", "int", "int", "int", "int"],
  sig: ["text", "int", "int", "text", "int", "text"],
  site: ["text", "int", "int", "text", "text"],
  const: ["text", "int", "int", "text", "text", "text"],
  span_line: ["text", "int", "int", "int"],
  diag: ["text", "int", "int", "int", "int", "text", "text", "text", "text"],
};

/** A literal value's column affinity (tie-break, M9): string -> text, number/boolean
 *  -> int, null -> text (a null literal seed binds a nullable text column in this
 *  slice; if it ever binds a numeric position that position's own resolved type wins,
 *  handled by declared/base types taking precedence over a __lit fallback). */
function primOfValue(value: Value): ColumnPrim {
  if (typeof value === "string") return "text";
  if (typeof value === "number" || typeof value === "boolean") return "int";
  return "text";
}

function isLiteralNode(node: Gen.HeadArg): node is Gen.Literal {
  return node.$type === "StrLit" || node.$type === "IntLit" || node.$type === "BoolLit" || node.$type === "NullLit";
}

function literalValue(node: Gen.Literal): Value {
  switch (node.$type) {
    case "StrLit":
      return node.value; // Langium's STRING value-converter already stripped quotes/escapes
    case "IntLit":
      return node.value;
    case "BoolLit":
      return node.raw === "true";
    case "NullLit":
      return null;
  }
}

/** Maps an ArgTerm (Var | Literal | Wildcard) into a positive body/head position:
 *  `Arg` today is Var|Lit only — a positive `_` needs the `Arg = Var | Lit | Wild`
 *  extension landing in a parallel package (see the worktree instructions this
 *  package was launched with). Emitting `wild()` here is deliberate; typecheck fails
 *  at exactly this seam until that merge lands, and nowhere else. */
function toPositiveArg(node: Gen.ArgTerm): Arg {
  if (node.$type === "Var") return variable(node.name);
  if (node.$type === "Wildcard") return wild();
  return literal(literalValue(node));
}

/** Maps an ArgTerm into a negated-ref position: `NegArg` already includes `Wild`
 *  (legal there today — no cross-branch dependency for negation). */
function toNegArg(node: Gen.ArgTerm): NegArg {
  if (node.$type === "Var") return variable(node.name);
  if (node.$type === "Wildcard") return wild();
  return literal(literalValue(node));
}

// ─────────────────────────────────────────────────────────────────────────────
// Named-arg resolution (NamedArgLaw, tasks.d.ts). One shared function: every
// named-arg call site (positive body atoms, negation, probes against the HOST
// decl's columns, query atoms, heads) resolves its raw arg list through this
// before doing anything kind-specific with the result.
// ─────────────────────────────────────────────────────────────────────────────

/** Resolves a mixed positional/named argument list against an ordered slot list
 *  into one slot per position (unfilled slots are `undefined`). Mixing law
 *  (python law, owner-set): positional args fill left-to-right; once a named
 *  arg appears, no further positional arg is legal. Collects EVERY violation as
 *  a "named-arg" LoadDiag instead of stopping at the first: positional-after-
 *  named, duplicate name, name+position slot collision, unknown column name.
 *  `args` is typed at the widest call-site shape (HeadArg = Member|ArgTerm|
 *  AggCall); body/negation/probe/query call sites pass the narrower AtomArg
 *  (Member|ArgTerm) list, which is structurally a subtype and never actually
 *  contains an AggCall at runtime.
 *
 *  `columns` entries may be `undefined` (salt-arg law, IdentityWitnessLaw,
 *  tasks.d.ts): a probe with more args than its host's declared columns
 *  splices synthetic, UNNAMED salt slots into the middle of the order. An
 *  `undefined` entry can only ever be filled positionally: it has no name a
 *  `Member` arg could target, so a named arg aimed at a salt position falls
 *  straight into the same "not a declared column" diag as any other unknown
 *  name (see the probe branch in processRuleBody for how `columns` gets
 *  built with salts spliced in). */
function resolveNamedArgs(
  context: BridgeContext,
  columns: readonly (string | undefined)[],
  args: readonly Gen.HeadArg[],
): (Gen.ArgTerm | Gen.AggCall | undefined)[] {
  const slots: (Gen.ArgTerm | Gen.AggCall | undefined)[] = new Array(columns.length).fill(undefined);
  const columnIndex = new Map<string, number>();
  columns.forEach((name, index) => {
    if (name !== undefined) columnIndex.set(name, index);
  });
  let sawNamed = false;
  let positionalIndex = 0;

  for (const arg of args) {
    if (arg.$type === "Member") {
      sawNamed = true;
      const slotIndex = columnIndex.get(arg.key);
      if (slotIndex === undefined) {
        context.diags.push({ code: "named-arg", message: `\`${arg.key}\` is not a declared column`, ...nodePosition(arg) });
        continue;
      }
      if (slots[slotIndex] !== undefined) {
        context.diags.push({
          code: "named-arg",
          message: `\`${arg.key}\`: slot already filled (duplicate name, or the name collides with a positional arg)`,
          ...nodePosition(arg),
        });
        continue;
      }
      slots[slotIndex] = arg.value;
      continue;
    }
    if (sawNamed) {
      context.diags.push({ code: "named-arg", message: "positional argument follows a named argument", ...nodePosition(arg) });
      continue;
    }
    if (positionalIndex < slots.length) slots[positionalIndex] = arg;
    positionalIndex++;
  }
  return slots;
}

/** A resolved slot (undefined = unfilled) in a positive body/probe/query position:
 *  unfilled -> `wild()` (subsumes trailing elision, see file header note 4). The
 *  AggCall case can't occur here (grammar-guaranteed: only HeadArg allows it) —
 *  the cast is structural, not a runtime check. */
function slotToPositiveArg(slot: Gen.ArgTerm | Gen.AggCall | undefined): Arg {
  return slot === undefined ? wild() : toPositiveArg(slot as Gen.ArgTerm);
}

/** Same as `slotToPositiveArg`, for a negated ref's args. */
function slotToNegArg(slot: Gen.ArgTerm | Gen.AggCall | undefined): NegArg {
  return slot === undefined ? wild() : toNegArg(slot as Gen.ArgTerm);
}

const CMP_OP_BY_SYMBOL: Record<string, CmpOp> = {
  "=": "eq",
  "!=": "ne",
  "<": "lt",
  "<=": "le",
  ">": "gt",
  ">=": "ge",
};

/** `fn` parses as a plain ID (same identifier-collision reasoning as PlainType.prim);
 *  validate it here against the four supported aggregates. An unrecognized name
 *  falls back to "count" (permissive — not a listed LoadDiag code this slice). */
const AGG_FN_NAMES = new Set(["count", "sum", "min", "max"]);
function aggFnOf(name: string): "count" | "sum" | "min" | "max" {
  return AGG_FN_NAMES.has(name) ? (name as "count" | "sum" | "min" | "max") : "count";
}

// ─────────────────────────────────────────────────────────────────────────────
// Bridge context: mutable state threaded through one bridge() call. A fresh one is
// built per call (bridge() is otherwise pure).
// ─────────────────────────────────────────────────────────────────────────────

interface BridgeContext {
  readonly diags: LoadDiag[];
  readonly knownRelColumns: Map<string, DeclInfo>; // user decls + builtin decls (arity/unknown-rel universe)
  /** M9 columnType flow: a rel's DECLARED column affinities, positional. Populated for
   *  user `rel`/`sh` decls (the grammar's `col: text|int`); builtin rels arrive
   *  type-less and resolve from SPINE_COLUMN_TYPES instead. Declared types win over
   *  every inference (peer ruling: they are declared, not inferred). */
  readonly declaredColumnTypes: Map<string, readonly ColumnPrim[]>;
  readonly headedRelNames: Set<string>; // rel names that appear as SOME user rule's head (-> IDB)
  readonly hostsByName: Map<string, HostDecl>;
  /** Salt-arg law (IdentityWitnessLaw, tasks.d.ts): the number of witness-salt args
   *  a host's probe(s) actually used, keyed by host name; absent/0 means "no salts,
   *  __req_h/__resp_h keep the pre-M8 declared-column shape" (regression: "zero-salt
   *  probes unchanged"). One value per host this slice: multiple probes of the SAME
   *  host with DIFFERING salt counts within one program are unchecked; the last
   *  probe processed wins. */
  readonly hostSaltCount: Map<string, number>;
  readonly retention: Map<string, Retention>;
  readonly literalSeeds: Map<string, Value>;
  readonly literalNameByValueKey: Map<string, string>; // JSON.stringify(value) -> minted rel name
  readonly minted: string[];
  readonly mintedNames: Set<string>; // dedupe guard for `minted` (one entry per host / literal)
  readonly mintedRules: AstRule[]; // __req_<host> rules, appended in mint order
  literalMintCounter: number;
}

function newContext(): BridgeContext {
  return {
    diags: [],
    knownRelColumns: new Map(),
    declaredColumnTypes: new Map(),
    headedRelNames: new Set(),
    hostsByName: new Map(),
    hostSaltCount: new Map(),
    retention: new Map(),
    literalSeeds: new Map(),
    literalNameByValueKey: new Map(),
    minted: [],
    mintedNames: new Set(),
    mintedRules: [],
    literalMintCounter: 0,
  };
}

function recordMinted(context: BridgeContext, name: string): void {
  if (context.mintedNames.has(name)) return;
  context.mintedNames.add(name);
  context.minted.push(name);
}

/** Mint (or reuse, deduped by value) a single-row constant rel `__lit_<n>(value)`. */
function mintLiteral(context: BridgeContext, value: Value): string {
  const key = JSON.stringify(value);
  const existing = context.literalNameByValueKey.get(key);
  if (existing !== undefined) return existing;
  const name = `__lit_${context.literalMintCounter++}`;
  context.literalNameByValueKey.set(key, name);
  context.literalSeeds.set(name, value);
  context.retention.set(name, "all");
  recordMinted(context, name);
  return name;
}

function checkArity(context: BridgeContext, rel: string, argCount: number, pos: { line: number; col: number }): void {
  const decl = context.knownRelColumns.get(rel);
  if (decl && argCount > decl.columns.length) {
    context.diags.push({
      code: "arity-mismatch",
      message: `\`${rel}\` is declared with ${decl.columns.length} column(s), but this reference passes ${argCount}`,
      ...pos,
    });
  }
}

function checkKnownRel(context: BridgeContext, rel: string, pos: { line: number; col: number }): void {
  if (!context.knownRelColumns.has(rel)) {
    context.diags.push({ code: "unknown-rel", message: `\`${rel}\` is not declared (not a user rel, builtin, or minted rel)`, ...pos });
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass 1: rel decls, host decls, retention.
// ─────────────────────────────────────────────────────────────────────────────

function checkColumnsForFrontierWrappers(context: BridgeContext, columns: readonly Gen.ColumnDecl[]): void {
  for (const columnDecl of columns) {
    const type = readColumnType(columnDecl.type);
    if (type.wrapper === "Min" || type.wrapper === "Max") {
      context.diags.push({
        code: "minmax-frontier",
        message: `column \`${columnDecl.name}\`: \`${type.wrapper}(...)\` parses but its lattice/lowering semantics are a frontier — not this slice`,
        ...nodePosition(columnDecl),
      });
    }
  }
}

/** `retention` parses as the INT terminal (grammar note: '0'/'1' as keywords would
 *  break every ordinary integer literal `0`/`1` elsewhere in the language) — reduce
 *  it to the Retention union here: 0 -> 0, 1 -> 1, anything else (including no
 *  paren group at all) -> "all". */
function readRetention(retention: number | undefined): Retention {
  if (retention === 0) return 0;
  if (retention === 1) return 1;
  return "all";
}

function processRelDecl(context: BridgeContext, decl: Gen.RelDecl): void {
  const columnNames = decl.columns.map((column) => column.name);
  context.knownRelColumns.set(decl.name, { columns: columnNames });
  context.declaredColumnTypes.set(decl.name, decl.columns.map((column) => readColumnType(column.type).prim));
  context.retention.set(decl.name, readRetention(decl.retention));
  checkColumnsForFrontierWrappers(context, decl.columns);
}

function processShDecl(context: BridgeContext, decl: Gen.ShDecl): void {
  checkColumnsForFrontierWrappers(context, decl.columns);
  const columns = decl.columns.map((column) => ({ name: column.name, ty: readColumnType(column.type).prim }));
  const template = decl.template.slice(1, -1); // strip the backtick delimiters
  const inputCols = columns
    .map((column) => column.name)
    .filter((name) => template.includes(`{${name}}`) || template.includes(`$${name}`));
  const hostDecl: HostDecl = { name: decl.name, columns, template, inputCols };
  context.hostsByName.set(decl.name, hostDecl);
  context.retention.set(`__req_${decl.name}`, "all");
  context.retention.set(`__resp_${decl.name}`, "all");
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass 2: body items. `collectBoundVars` is the static, order-independent scan
// (the "comma unordered" ruling) — a var is bound if SOME positive RelRef/probe
// atom anywhere in the body names it, regardless of textual position relative to a
// comparison that reads it. Probe minting's "atoms textually before" clause is a
// separate, genuinely-ordered mechanism handled inline in the forward pass below.
// ─────────────────────────────────────────────────────────────────────────────

function collectBoundVars(body: readonly Gen.BodyItem[]): Set<string> {
  const bound = new Set<string>();
  for (const item of body) {
    if (item.$type === "RelRefItem" || item.$type === "ProbeItem") {
      for (const arg of item.args) {
        // A named arg's Var lives one level down, at `arg.value` — the arg node
        // itself is a Member, not a Var (`console_hit(path: p)` binds `p`, not
        // `path`, the column name).
        if (arg.$type === "Var") bound.add(arg.name);
        else if (arg.$type === "Member" && arg.value.$type === "Var") bound.add(arg.value.name);
      }
    }
  }
  return bound;
}

/** Which of a CompareItem's two operands is the Var and which is the Literal — the
 *  grammar's two alternatives guarantee exactly one of each per parse. */
function splitCompare(item: Gen.CompareItem): { varNode: Gen.Var; litNode: Gen.Literal } {
  if (item.lhs.$type === "Var") return { varNode: item.lhs, litNode: item.rhs as Gen.Literal };
  return { varNode: item.rhs as Gen.Var, litNode: item.lhs as Gen.Literal };
}

interface RuleBodyResult {
  readonly body: BodyPred[];
  readonly boundVars: Set<string>;
}

function processRuleBody(context: BridgeContext, body: readonly Gen.BodyItem[]): RuleBodyResult {
  const boundVars = collectBoundVars(body);
  const outBody: BodyPred[] = [];

  for (const item of body) {
    switch (item.$type) {
      case "RelRefItem": {
        checkKnownRel(context, item.rel, nodePosition(item));
        checkArity(context, item.rel, item.args.length, nodePosition(item));
        const columns = context.knownRelColumns.get(item.rel)?.columns ?? [];
        const slots = resolveNamedArgs(context, columns, item.args);
        outBody.push(relRef(item.rel, ...slots.map(slotToPositiveArg)));
        break;
      }
      case "NegItem": {
        checkKnownRel(context, item.rel, nodePosition(item));
        checkArity(context, item.rel, item.args.length, nodePosition(item));
        const columns = context.knownRelColumns.get(item.rel)?.columns ?? [];
        const slots = resolveNamedArgs(context, columns, item.args);
        outBody.push(notRel(item.rel, ...slots.map(slotToNegArg)));
        break;
      }
      case "CompareItem": {
        const { varNode, litNode } = splitCompare(item);
        const comparisonOperator = CMP_OP_BY_SYMBOL[item.op]!;
        const value = literalValue(litNode);
        if (comparisonOperator === "eq" && !boundVars.has(varNode.name)) {
          const mintedName = mintLiteral(context, value);
          outBody.push(relRef(mintedName, variable(varNode.name)));
          boundVars.add(varNode.name);
        } else {
          outBody.push(compare(comparisonOperator, varNode.name, value));
        }
        break;
      }
      case "ProbeItem": {
        const host = context.hostsByName.get(item.rel);
        if (!host) {
          context.diags.push({ code: "unknown-rel", message: `\`${item.rel}\` is not a declared host (\`sh\` decl)`, ...nodePosition(item) });
          break;
        }
        const hostColumnNames = host.columns.map((column) => column.name);
        const inputColSet = new Set(host.inputCols);
        const outputColumnNames = hostColumnNames.filter((name) => !inputColSet.has(name));

        // Salt-arg law (IdentityWitnessLaw, tasks.d.ts): a probe may pass MORE args
        // than the host's declared columns. The excess (saltCount) are positional-
        // only witness salts, spliced between the input args and the output args:
        // more args than columns is now legal (arity-mismatch for probes fires only
        // when there aren't enough to fill the inputs, caught below per-input-column
        // exactly as before). len <= declared columns (saltCount 0) keeps the pre-M8
        // declared-column order untouched (regression: "zero-salt probes unchanged").
        const saltCount = Math.max(0, item.args.length - hostColumnNames.length);
        const saltSlotNames: readonly string[] = Array.from({ length: saltCount }, (_, index) => `salt_${index}`);
        const slotOrder: readonly (string | undefined)[] =
          saltCount > 0 ? [...host.inputCols, ...saltSlotNames.map(() => undefined), ...outputColumnNames] : hostColumnNames;
        if (saltCount > 0) context.hostSaltCount.set(host.name, saltCount);

        // Resolve the FULL mixed positional/named arg list against slotOrder (named
        // args resolve against the HOST decl's REAL columns only: a salt slot has
        // no name in slotOrder, so it can never be targeted by a Member arg; it
        // falls into the ordinary "not a declared column" named-arg diag instead).
        const slots = resolveNamedArgs(context, slotOrder, item.args);
        const slotIndexByName = new Map<string, number>();
        slotOrder.forEach((name, index) => {
          if (name !== undefined) slotIndexByName.set(name, index);
        });

        // Literal-binding rewrite for literal probe INPUT (and salt) args: __req_h's
        // demand key must be all-vars (it groups by the request tuple). The freshly-
        // bound var reuses the HOST'S declared column name (or the synthetic
        // `salt_<n>` name) at that position; there is no user-written var name for
        // a bare literal probe arg to reuse.
        const mintedInputAtoms: BodyPred[] = [];
        const inputArgs: Arg[] = [];
        const saltArgs: Arg[] = [];
        const outputArgs: Arg[] = [];

        host.inputCols.forEach((columnName) => {
          const slot = slots[slotIndexByName.get(columnName)!];
          if (slot === undefined) {
            context.diags.push({
              code: "arity-mismatch",
              message: `\`${item.rel}?\` needs input column \`${columnName}\` bound; it was not provided`,
              ...nodePosition(item),
            });
            inputArgs.push(wild());
            return;
          }
          if (slot.$type === "Wildcard") {
            context.diags.push({
              code: "arity-mismatch",
              message: `\`${item.rel}?\` input position cannot be a wildcard (the demand key needs a concrete or bound value)`,
              ...nodePosition(slot),
            });
            inputArgs.push(wild());
            return;
          }
          if (slot.$type === "Var") {
            inputArgs.push(variable(slot.name));
            return;
          }
          const mintedName = mintLiteral(context, literalValue(slot as Gen.Literal));
          mintedInputAtoms.push(relRef(mintedName, variable(columnName)));
          inputArgs.push(variable(columnName));
        });

        // Salts join the __req_h demand key exactly like an input does: a wild or
        // omitted salt binds nothing, a meaningless witness, so it is a load error
        // rather than silently padding to wild() the way a missing OUTPUT does.
        saltSlotNames.forEach((saltName, index) => {
          const slot = slots[host.inputCols.length + index];
          if (slot === undefined || slot.$type === "Wildcard") {
            context.diags.push({
              code: "named-arg",
              message: `\`${item.rel}?\` salt argument \`${saltName}\` must be a Var or a literal, not a wildcard`,
              ...(slot !== undefined ? nodePosition(slot) : nodePosition(item)),
            });
            saltArgs.push(wild());
            return;
          }
          if (slot.$type === "Var") {
            saltArgs.push(variable(slot.name));
            return;
          }
          const mintedName = mintLiteral(context, literalValue(slot as Gen.Literal));
          mintedInputAtoms.push(relRef(mintedName, variable(saltName)));
          saltArgs.push(variable(saltName));
        });

        // Outputs: no literal-binding (an output is what the host call PRODUCES,
        // never a demand-key input); an omitted output slot wild-pads (existing
        // elision law), a Lit output arg stays a literal term via toPositiveArg.
        outputColumnNames.forEach((columnName) => {
          const slot = slots[slotIndexByName.get(columnName)!];
          outputArgs.push(slotToPositiveArg(slot));
        });

        const reqRelName = `__req_${host.name}`;
        const respRelName = `__resp_${host.name}`;
        recordMinted(context, reqRelName);
        recordMinted(context, respRelName);

        // The literal-binding atoms land in THIS rule's own body too (same as any
        // other literal-binding mint) — not only in the minted request rule's body.
        outBody.push(...mintedInputAtoms);

        // Request rule: head = the input columns + salt columns (IdentityWitnessLaw:
        // "__req_h columns = [...inputCols, salts...]"); body = every non-probe atom
        // already assembled for THIS rule (textually before this probe, now
        // including the literal-binding atoms just pushed above).
        const reqHeadArgs: readonly Arg[] = [...inputArgs, ...saltArgs];
        const reqHeadTerms: HeadTerm[] = reqHeadArgs.map((arg) => headVar(arg.kind === "var" ? arg.name : "_probe_input_error"));
        context.mintedRules.push({ head: reqRelName, headTerms: reqHeadTerms, body: [...outBody] });

        outBody.push(relRef(respRelName, ...inputArgs, ...saltArgs, ...outputArgs));
        break;
      }
      case "MutationItem": {
        context.diags.push({
          code: "mutation-frontier",
          message: `\`${item.rel}!(...)\` parses but mutations land with a later slice`,
          ...nodePosition(item),
        });
        break;
      }
    }
  }

  return { body: outBody, boundVars };
}

// ─────────────────────────────────────────────────────────────────────────────
// Head terms + the diag head-default law.
// ─────────────────────────────────────────────────────────────────────────────

/** `slots` has one entry per DECLARED column of `headRel` (resolveNamedArgs already
 *  folded positional + named head args into this shape) — `undefined` means the
 *  slot was never filled at all (named-arg omission subsumes the old "unbound
 *  head var" case; a Wildcard or an unbound Var reaches the same fallback below).
 *  `headNode` is only a position fallback for an omitted slot (there is no textual
 *  arg node to read a line/col off of). */
function buildHeadTerms(
  context: BridgeContext,
  headRel: string,
  headNode: Gen.HeadAtom,
  columns: readonly string[],
  slots: readonly (Gen.ArgTerm | Gen.AggCall | undefined)[],
  boundVars: Set<string>,
  outBody: BodyPred[],
): readonly HeadTerm[] {
  const isDiag = headRel === "diag";

  return slots.map((slot, position) => {
    if (slot !== undefined && slot.$type === "AggCall") return headAgg(aggFnOf(slot.fn), slot.arg.name);

    if (slot !== undefined && slot.$type === "Var" && boundVars.has(slot.name)) return headVar(slot.name);

    const columnName = columns[position];

    // A bare literal in head position (the "fact" shape, a literal mixed into an
    // otherwise-var head, or a named literal head arg like `severity: "warn"`):
    // always literal-bind, regardless of which rel this is — the general form of
    // the orchestrator-pinned rewrite, not diag-specific. The freshly-bound var
    // reuses the DECLARED COLUMN name at this position (same reuse-the-declared-
    // name law the probe literal-input rewrite uses below) — a literal has no
    // user-written var name of its own to reuse, and reusing the column name is
    // what keeps a re-pinned golden reading `severity`/`code`/`msg`, not a raw
    // minted rel name.
    if (slot !== undefined && isLiteralNode(slot)) {
      const mintedName = mintLiteral(context, literalValue(slot));
      const boundName = columnName ?? mintedName;
      outBody.push(relRef(mintedName, variable(boundName)));
      return headVar(boundName);
    }

    // From here: `slot` is undefined (the arg was omitted entirely — a named-arg
    // partial head), a Wildcard, or a Var never bound in the body. diag gets the
    // position-based default law; everything else is a binding-arity load error.
    if (isDiag) {
      if (columnName === "end_line") return headVar("line");
      if (columnName === "end_col") return headVar("col");
      if (columnName === "severity" || columnName === "code" || columnName === "hint") {
        const defaultValue: Value = columnName === "severity" ? "warn" : null;
        const mintedName = mintLiteral(context, defaultValue);
        // A textually-present-but-unbound Var keeps ITS OWN name; an omitted slot
        // (no Var at all) reuses the declared column name (== columnName here).
        const boundName = slot?.$type === "Var" ? slot.name : columnName;
        outBody.push(relRef(mintedName, variable(boundName)));
        return headVar(boundName);
      }
    }
    // path/line/col/msg (or any non-diag head): unbound/omitted is a load error.
    const label = slot?.$type === "Var" ? `\`${slot.name}\`` : "this position";
    context.diags.push({
      code: "arity-mismatch",
      message: `head of \`${headRel}\`: ${label} is not bound by the rule's body`,
      ...(slot !== undefined ? nodePosition(slot) : nodePosition(headNode)),
    });
    return headVar(slot?.$type === "Var" ? slot.name : `__unbound_${position}`);
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass 3: rules (facts included — a fact is a DlRule with an empty body).
// ─────────────────────────────────────────────────────────────────────────────

function processDlRule(context: BridgeContext, dlRule: Gen.DlRule): AstRule {
  const headRel = dlRule.head.rel;
  context.headedRelNames.add(headRel);
  checkKnownRel(context, headRel, nodePosition(dlRule.head));
  checkArity(context, headRel, dlRule.head.args.length, nodePosition(dlRule.head));

  const { body: outBody, boundVars } = processRuleBody(context, dlRule.body);
  const headColumns = context.knownRelColumns.get(headRel)?.columns ?? [];
  const headSlots = resolveNamedArgs(context, headColumns, dlRule.head.args);
  const headTerms = buildHeadTerms(context, headRel, dlRule.head, headColumns, headSlots, boundVars, outBody);

  return { head: headRel, headTerms, body: outBody };
}

function processQueryStmt(context: BridgeContext, queryStmt: Gen.QueryStmt): AstRelRef {
  checkKnownRel(context, queryStmt.rel, nodePosition(queryStmt));
  checkArity(context, queryStmt.rel, queryStmt.args.length, nodePosition(queryStmt));
  const columns = context.knownRelColumns.get(queryStmt.rel)?.columns ?? [];
  const slots = resolveNamedArgs(context, columns, queryStmt.args);
  return relRef(queryStmt.rel, ...slots.map(slotToPositiveArg));
}

// ─────────────────────────────────────────────────────────────────────────────
// Column-type resolution (M9 columnType flow, tasks.d.ts EngineStorageLaw). Every
// rel in the built program gets one affinity per column. Precedence (peer ruling):
//   1. DECLARED types (user `rel`/`sh` decls `col: text|int`) — declared, not inferred.
//   2. Builtin base types (SPINE_COLUMN_TYPES) for the spine/diag rels.
//   3. Host rels __resp_<h>/__req_<h>: each column is the host's declared column type
//      by name; a synthetic `salt_<n>` witness column is `text` (the witness in this
//      slice is a content hash — a text value; a numeric salt would need its source
//      column's type, a follow-up if one ever appears).
//   4. __lit_<n>: primOfValue of its one seeded literal.
//   5. Derived (headed) rels with no declared/base type: TRACE each head var to the
//      body-atom source column that binds it. Tie-breaks (peer ruling): a head var
//      bound by multiple body sources must AGREE (disagreement keeps the first
//      resolved and would surface as a conflict in a stricter slice); count/sum -> int;
//      min/max -> the arg's source type; unresolved position -> text.
// ─────────────────────────────────────────────────────────────────────────────

/** Trace one head-var / agg-arg name to the affinity of the body column that binds it,
 *  using the affinities resolved so far. Returns undefined if no typed source binds it. */
function traceVarType(
  varName: string,
  body: readonly BodyPred[],
  resolved: ReadonlyMap<string, readonly ColumnPrim[]>,
): ColumnPrim | undefined {
  for (const pred of body) {
    if (pred.kind !== "rel") continue;
    const sourceTypes = resolved.get(pred.rel);
    if (!sourceTypes) continue;
    for (let position = 0; position < pred.args.length; position++) {
      const arg = pred.args[position]!;
      if (arg.kind === "var" && arg.name === varName && sourceTypes[position] !== undefined) {
        return sourceTypes[position];
      }
    }
  }
  return undefined;
}

function buildColumnTypes(
  rels: readonly AstRelDecl[],
  rules: readonly AstRule[],
  context: BridgeContext,
): Map<string, readonly ColumnPrim[]> {
  const resolved = new Map<string, readonly ColumnPrim[]>();
  const derivedPending: AstRelDecl[] = [];

  // Passes 1-4: everything resolvable without tracing a rule body.
  for (const decl of rels) {
    const declared = context.declaredColumnTypes.get(decl.name);
    if (declared) {
      resolved.set(decl.name, declared);
      continue;
    }
    const base = SPINE_COLUMN_TYPES[decl.name];
    if (base) {
      resolved.set(decl.name, base);
      continue;
    }
    if (decl.name.startsWith("__lit_")) {
      resolved.set(decl.name, [primOfValue(context.literalSeeds.get(decl.name) ?? null)]);
      continue;
    }
    const hostName = hostNameOfMinted(decl.name);
    const host = hostName ? context.hostsByName.get(hostName) : undefined;
    if (host) {
      const byName = new Map(host.columns.map((column) => [column.name, column.ty] as const));
      resolved.set(
        decl.name,
        decl.columns.map((column) => byName.get(column) ?? "text"), // salt_<n> -> text
      );
      continue;
    }
    derivedPending.push(decl);
  }

  // Pass 5: derived head-var trace, iterated to a fixpoint (a derived rel may read
  // another derived rel). Bounded by rel count; unresolved positions default to text.
  const rulesByHead = new Map<string, AstRule[]>();
  for (const rule of rules) (rulesByHead.get(rule.head) ?? rulesByHead.set(rule.head, []).get(rule.head)!).push(rule);
  for (let pass = 0; pass < derivedPending.length + 1; pass++) {
    let moved = false;
    for (const decl of derivedPending) {
      if (resolved.has(decl.name)) continue;
      const headingRules = rulesByHead.get(decl.name) ?? [];
      const types: (ColumnPrim | undefined)[] = decl.columns.map(() => undefined);
      let anyKnown = false;
      for (const rule of headingRules) {
        rule.headTerms.forEach((term, position) => {
          if (types[position] !== undefined) return;
          const traced =
            term.kind === "hagg"
              ? term.fn === "count" || term.fn === "sum"
                ? "int"
                : traceVarType(term.arg.name, rule.body, resolved)
              : traceVarType(term.name, rule.body, resolved);
          if (traced !== undefined) {
            types[position] = traced;
            anyKnown = true;
          }
        });
      }
      if (anyKnown && types.every((type) => type !== undefined)) {
        resolved.set(decl.name, types as ColumnPrim[]);
        moved = true;
      }
    }
    if (!moved) break;
  }

  // Fallback: any still-unresolved rel (no typed source reached it) is all-text.
  for (const decl of rels) {
    if (!resolved.has(decl.name)) resolved.set(decl.name, decl.columns.map(() => "text"));
  }
  return resolved;
}

/** `__req_<host>` / `__resp_<host>` -> `<host>`; anything else -> undefined. */
function hostNameOfMinted(relName: string): string | undefined {
  if (relName.startsWith("__req_")) return relName.slice("__req_".length);
  if (relName.startsWith("__resp_")) return relName.slice("__resp_".length);
  return undefined;
}

// ─────────────────────────────────────────────────────────────────────────────
// bridge() — the public entry point.
// ─────────────────────────────────────────────────────────────────────────────

export function bridge(dlText: string, builtinRels: readonly AstRelDecl[]): BridgeResult {
  const { program: parsedProgram, diags: parseDiags } = parseDlDocument(dlText);
  if (parseDiags.length > 0) return { kind: "err", diags: parseDiags };

  const context = newContext();
  for (const builtin of builtinRels) {
    context.knownRelColumns.set(builtin.name, { columns: [...builtin.columns] });
    context.retention.set(builtin.name, "all");
  }

  const relDecls: Gen.RelDecl[] = [];
  const shDecls: Gen.ShDecl[] = [];
  const dlRules: Gen.DlRule[] = [];
  const queryStmts: Gen.QueryStmt[] = [];
  for (const statement of parsedProgram.statements) {
    if (statement.$type === "RelDecl") relDecls.push(statement);
    else if (statement.$type === "ShDecl") shDecls.push(statement);
    else if (statement.$type === "DlRule") dlRules.push(statement);
    else queryStmts.push(statement);
  }

  for (const decl of relDecls) processRelDecl(context, decl);
  for (const decl of shDecls) processShDecl(context, decl);

  const userRules = dlRules.map((dlRule) => processDlRule(context, dlRule));
  const queries = queryStmts.map((queryStmt) => processQueryStmt(context, queryStmt));

  const rules: AstRule[] = [...userRules, ...context.mintedRules];

  if (context.diags.length > 0) return { kind: "err", diags: context.diags };

  // Origin: headed (by a USER rule) -> IDB; everything else stays EDB. Minted
  // __req_<host> rules always head their own rel, so those are always IDB too.
  const rels: AstRelDecl[] = [];
  for (const decl of relDecls) {
    const info = context.knownRelColumns.get(decl.name)!;
    rels.push(context.headedRelNames.has(decl.name) ? derivedRel(decl.name, info.columns) : edbRel(decl.name, info.columns));
  }
  for (const builtin of builtinRels) {
    rels.push(context.headedRelNames.has(builtin.name) ? derivedRel(builtin.name, [...builtin.columns]) : builtin);
  }
  for (const host of context.hostsByName.values()) {
    // Salt-arg law (IdentityWitnessLaw, tasks.d.ts): __resp_h/__req_h's column shape
    // depends on whether any probe of this host actually used salts this program
    // (context.hostSaltCount, set in the ProbeItem branch above). Zero salts keeps the
    // pre-M8 shape byte-for-byte (declared-column order for __resp_h, plain
    // inputCols for __req_h): the "zero-salt probes unchanged" regression.
    const saltCount = context.hostSaltCount.get(host.name) ?? 0;
    const hostColumnNames = host.columns.map((column) => column.name);
    if (saltCount === 0) {
      rels.push(edbRel(`__resp_${host.name}`, hostColumnNames));
      rels.push(derivedRel(`__req_${host.name}`, [...host.inputCols]));
      continue;
    }
    const inputColSet = new Set(host.inputCols);
    const outputColumnNames = hostColumnNames.filter((name) => !inputColSet.has(name));
    const saltColumns = Array.from({ length: saltCount }, (_, index) => `salt_${index}`);
    rels.push(edbRel(`__resp_${host.name}`, [...host.inputCols, ...saltColumns, ...outputColumnNames]));
    rels.push(derivedRel(`__req_${host.name}`, [...host.inputCols, ...saltColumns]));
  }
  for (const name of context.literalSeeds.keys()) {
    rels.push(edbRel(name, ["value"]));
  }

  const builtProgram: AstProgram = { rels, rules };

  try {
    const graph = buildRuleGraph(builtProgram);
    stratify(graph, scc(graph));
  } catch (failure) {
    if (failure instanceof NonStratifiableError) {
      return { kind: "err", diags: [{ code: "non-stratifiable", message: failure.message, line: 0, col: 0 }] };
    }
    throw failure;
  }

  return {
    kind: "ok",
    program: builtProgram,
    hosts: [...context.hostsByName.values()],
    retention: context.retention,
    queries,
    minted: context.minted,
    literalSeeds: context.literalSeeds,
    columnTypes: buildColumnTypes(rels, rules, context),
  };
}

// ---- dataflow proof (src/0_types.ts) -----------------------------------------
export type BridgeHolds = AssertTrue<typeof bridge extends Bridge ? true : false>;
