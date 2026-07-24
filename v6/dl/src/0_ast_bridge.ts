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
 * The three rewrites (heart of this file, in processRuleBody / buildHeadTerms):
 *   1. literal-binding: a bare literal in a head/probe-input position, or an `eq`
 *      comparison (either textual order) whose var operand is not otherwise bound,
 *      mints a single-row constant rel `__lit_<n>(value)` and a body atom referencing
 *      it, in place of an ast.ts Compare or a literal HeadTerm (neither of which
 *      exists — HeadTerm is Var|Agg only, and Compare always filters a BOUND var).
 *   2. probe minting: `h?(in.., out..)` splits into a minted `__req_h` rule (head =
 *      the input columns; body = every non-probe atom already assembled for this rule
 *      PLUS the literal-binding atoms for any literal probe input) and a minted EDB
 *      `__resp_h` rel referenced in place of the probe.
 *   3. diag head-default law: an unbound diag head var at end_line/end_col/severity/
 *      code/hint gets a default (reuse line/col, or a minted literal for the rest);
 *      an unbound path/line/col/msg stays a load error.
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
import type { BridgeResult, HostDecl, LoadDiag, Retention, Value } from "../tasks.d.ts";

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
  const doc = sharedServices.workspace.LangiumDocumentFactory.fromString<Gen.Program>(dlText, uri);
  const diags: LoadDiag[] = [];
  for (const lexErr of doc.parseResult.lexerErrors) {
    diags.push({ code: "parse", message: lexErr.message, line: lexErr.line ?? 0, col: lexErr.column ?? 0 });
  }
  for (const parseErr of doc.parseResult.parserErrors) {
    diags.push({
      code: "parse",
      message: parseErr.message,
      line: parseErr.token.startLine ?? 0,
      col: parseErr.token.startColumn ?? 0,
    });
  }
  return { program: doc.parseResult.value, diags };
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

const CMP_OP_BY_SYMBOL: Record<string, CmpOp> = {
  "=": "eq",
  "!=": "ne",
  "<": "lt",
  "<=": "le",
  ">": "gt",
  ">=": "ge",
};

/** The diag rel's fixed column order (v5 schema verbatim, src/engine/decls.rs:263,
 *  mirrored in tasks.d.ts DiagRow) — positional index is what the head-default law
 *  keys off of. */
const DIAG_COLUMNS = ["path", "line", "col", "end_line", "end_col", "severity", "code", "msg", "hint"] as const;

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
  readonly headedRelNames: Set<string>; // rel names that appear as SOME user rule's head (-> IDB)
  readonly hostsByName: Map<string, HostDecl>;
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
    headedRelNames: new Set(),
    hostsByName: new Map(),
    retention: new Map(),
    literalSeeds: new Map(),
    literalNameByValueKey: new Map(),
    minted: [],
    mintedNames: new Set(),
    mintedRules: [],
    literalMintCounter: 0,
  };
}

function recordMinted(ctx: BridgeContext, name: string): void {
  if (ctx.mintedNames.has(name)) return;
  ctx.mintedNames.add(name);
  ctx.minted.push(name);
}

/** Mint (or reuse, deduped by value) a single-row constant rel `__lit_<n>(value)`. */
function mintLiteral(ctx: BridgeContext, value: Value): string {
  const key = JSON.stringify(value);
  const existing = ctx.literalNameByValueKey.get(key);
  if (existing !== undefined) return existing;
  const name = `__lit_${ctx.literalMintCounter++}`;
  ctx.literalNameByValueKey.set(key, name);
  ctx.literalSeeds.set(name, value);
  ctx.retention.set(name, "all");
  recordMinted(ctx, name);
  return name;
}

function checkArity(ctx: BridgeContext, rel: string, argCount: number, pos: { line: number; col: number }): void {
  const decl = ctx.knownRelColumns.get(rel);
  if (decl && argCount > decl.columns.length) {
    ctx.diags.push({
      code: "arity-mismatch",
      message: `\`${rel}\` is declared with ${decl.columns.length} column(s), but this reference passes ${argCount}`,
      ...pos,
    });
  }
}

function checkKnownRel(ctx: BridgeContext, rel: string, pos: { line: number; col: number }): void {
  if (!ctx.knownRelColumns.has(rel)) {
    ctx.diags.push({ code: "unknown-rel", message: `\`${rel}\` is not declared (not a user rel, builtin, or minted rel)`, ...pos });
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass 1: rel decls, host decls, retention.
// ─────────────────────────────────────────────────────────────────────────────

function checkColumnsForFrontierWrappers(ctx: BridgeContext, columns: readonly Gen.ColumnDecl[]): void {
  for (const col of columns) {
    const type = readColumnType(col.type);
    if (type.wrapper === "Min" || type.wrapper === "Max") {
      ctx.diags.push({
        code: "minmax-frontier",
        message: `column \`${col.name}\`: \`${type.wrapper}(...)\` parses but its lattice/lowering semantics are a frontier — not this slice`,
        ...nodePosition(col),
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

function processRelDecl(ctx: BridgeContext, decl: Gen.RelDecl): void {
  const columnNames = decl.columns.map((column) => column.name);
  ctx.knownRelColumns.set(decl.name, { columns: columnNames });
  ctx.retention.set(decl.name, readRetention(decl.retention));
  checkColumnsForFrontierWrappers(ctx, decl.columns);
}

function processShDecl(ctx: BridgeContext, decl: Gen.ShDecl): void {
  checkColumnsForFrontierWrappers(ctx, decl.columns);
  const columns = decl.columns.map((column) => ({ name: column.name, ty: readColumnType(column.type).prim }));
  const template = decl.template.slice(1, -1); // strip the backtick delimiters
  const inputCols = columns
    .map((column) => column.name)
    .filter((name) => template.includes(`{${name}}`) || template.includes(`$${name}`));
  const hostDecl: HostDecl = { name: decl.name, columns, template, inputCols };
  ctx.hostsByName.set(decl.name, hostDecl);
  ctx.retention.set(`__req_${decl.name}`, "all");
  ctx.retention.set(`__resp_${decl.name}`, "all");
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
      for (const arg of item.args) if (arg.$type === "Var") bound.add(arg.name);
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

function processRuleBody(ctx: BridgeContext, body: readonly Gen.BodyItem[]): RuleBodyResult {
  const boundVars = collectBoundVars(body);
  const outBody: BodyPred[] = [];

  for (const item of body) {
    switch (item.$type) {
      case "RelRefItem": {
        checkKnownRel(ctx, item.rel, nodePosition(item));
        checkArity(ctx, item.rel, item.args.length, nodePosition(item));
        outBody.push(relRef(item.rel, ...item.args.map(toPositiveArg)));
        break;
      }
      case "NegItem": {
        checkKnownRel(ctx, item.rel, nodePosition(item));
        checkArity(ctx, item.rel, item.args.length, nodePosition(item));
        outBody.push(notRel(item.rel, ...item.args.map(toNegArg)));
        break;
      }
      case "CompareItem": {
        const { varNode, litNode } = splitCompare(item);
        const op = CMP_OP_BY_SYMBOL[item.op]!;
        const value = literalValue(litNode);
        if (op === "eq" && !boundVars.has(varNode.name)) {
          const mintedName = mintLiteral(ctx, value);
          outBody.push(relRef(mintedName, variable(varNode.name)));
          boundVars.add(varNode.name);
        } else {
          outBody.push(compare(op, varNode.name, value));
        }
        break;
      }
      case "ProbeItem": {
        const host = ctx.hostsByName.get(item.rel);
        if (!host) {
          ctx.diags.push({ code: "unknown-rel", message: `\`${item.rel}\` is not a declared host (\`sh\` decl)`, ...nodePosition(item) });
          break;
        }
        const totalCols = host.columns.length;
        if (item.args.length > totalCols) {
          ctx.diags.push({
            code: "arity-mismatch",
            message: `\`${item.rel}?\` is declared with ${totalCols} column(s), but this probe passes ${item.args.length}`,
            ...nodePosition(item),
          });
        }
        const inputCount = host.inputCols.length;
        if (item.args.length < inputCount) {
          ctx.diags.push({
            code: "arity-mismatch",
            message: `\`${item.rel}?\` needs all ${inputCount} input column(s) bound; only ${item.args.length} arg(s) given`,
            ...nodePosition(item),
          });
        }
        const inputNodes = item.args.slice(0, inputCount);
        const outputNodes = item.args.slice(inputCount);

        // Literal-binding rewrite for literal probe INPUT args: __req_h's demand key
        // must be all-vars (it groups by the request tuple). The freshly-bound var
        // reuses the HOST'S declared input column name at that position (there is no
        // user-written var name for a bare literal probe arg to reuse; the column
        // name is the next-best meaningful name, and it's what the plan's worked
        // example pins: `sg?("console.log($$$ARGS)", path, ...)` binds through a
        // var named `pattern`, matching the host's first inputCol).
        const mintedInputAtoms: BodyPred[] = [];
        const inputArgs: Arg[] = inputNodes.map((node, index) => {
          if (node.$type === "Var") return variable(node.name);
          if (node.$type === "Wildcard") {
            ctx.diags.push({
              code: "arity-mismatch",
              message: `\`${item.rel}?\` input position cannot be a wildcard (the demand key needs a concrete or bound value)`,
              ...nodePosition(node),
            });
            return wild();
          }
          const mintedName = mintLiteral(ctx, literalValue(node));
          const boundVarName = host.inputCols[index]!;
          mintedInputAtoms.push(relRef(mintedName, variable(boundVarName)));
          return variable(boundVarName);
        });
        const outputArgs: Arg[] = outputNodes.map(toPositiveArg);

        const reqRelName = `__req_${host.name}`;
        const respRelName = `__resp_${host.name}`;
        recordMinted(ctx, reqRelName);
        recordMinted(ctx, respRelName);

        // The literal-binding atoms land in THIS rule's own body too (same as any
        // other literal-binding mint) — not only in the minted request rule's body.
        outBody.push(...mintedInputAtoms);

        // Request rule: head = the input columns; body = every non-probe atom
        // already assembled for THIS rule (textually before this probe, now
        // including the literal-binding atoms just pushed above).
        const reqHeadTerms: HeadTerm[] = inputArgs.map((arg) => headVar(arg.kind === "var" ? arg.name : "_probe_input_error"));
        ctx.mintedRules.push({ head: reqRelName, headTerms: reqHeadTerms, body: [...outBody] });

        outBody.push(relRef(respRelName, ...inputArgs, ...outputArgs));
        break;
      }
      case "MutationItem": {
        ctx.diags.push({
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

function buildHeadTerms(
  ctx: BridgeContext,
  headRel: string,
  headArgs: readonly Gen.HeadArg[],
  boundVars: Set<string>,
  outBody: BodyPred[],
): readonly HeadTerm[] {
  const diagColumns = headRel === "diag" ? DIAG_COLUMNS : undefined;

  return headArgs.map((argNode, position) => {
    if (argNode.$type === "AggCall") return headAgg(aggFnOf(argNode.fn), argNode.arg.name);

    if (argNode.$type === "Var" && boundVars.has(argNode.name)) return headVar(argNode.name);

    // A bare literal in head position (the "fact" shape, or a literal mixed into an
    // otherwise-var head): always literal-bind, regardless of which rel this is —
    // this is the general form of the orchestrator-pinned rewrite, not diag-specific.
    if (isLiteralNode(argNode)) {
      const mintedName = mintLiteral(ctx, literalValue(argNode));
      outBody.push(relRef(mintedName, variable(mintedName)));
      return headVar(mintedName);
    }

    // From here: a Var that's never bound in the body, or a Wildcard in head
    // position. diag gets the position-based default law; everything else is a
    // binding-arity load error.
    const diagColumnName = diagColumns?.[position];
    if (diagColumnName === "end_line") return headVar("line");
    if (diagColumnName === "end_col") return headVar("col");
    if (diagColumnName === "severity" || diagColumnName === "code" || diagColumnName === "hint") {
      const defaultValue: Value = diagColumnName === "severity" ? "warn" : null;
      const mintedName = mintLiteral(ctx, defaultValue);
      const boundName = argNode.$type === "Var" ? argNode.name : mintedName;
      outBody.push(relRef(mintedName, variable(boundName)));
      return headVar(boundName);
    }
    // path/line/col/msg (or any non-diag head): unbound is a load error.
    const label = argNode.$type === "Var" ? `\`${argNode.name}\`` : "this position";
    ctx.diags.push({
      code: "arity-mismatch",
      message: `head of \`${headRel}\`: ${label} is not bound by the rule's body`,
      ...nodePosition(argNode),
    });
    return headVar(argNode.$type === "Var" ? argNode.name : `__unbound_${position}`);
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass 3: rules (facts included — a fact is a DlRule with an empty body).
// ─────────────────────────────────────────────────────────────────────────────

function processDlRule(ctx: BridgeContext, dlRule: Gen.DlRule): AstRule {
  const headRel = dlRule.head.rel;
  ctx.headedRelNames.add(headRel);
  checkKnownRel(ctx, headRel, nodePosition(dlRule.head));
  checkArity(ctx, headRel, dlRule.head.args.length, nodePosition(dlRule.head));

  const { body: outBody, boundVars } = processRuleBody(ctx, dlRule.body);
  const headTerms = buildHeadTerms(ctx, headRel, dlRule.head.args, boundVars, outBody);

  return { head: headRel, headTerms, body: outBody };
}

function processQueryStmt(ctx: BridgeContext, queryStmt: Gen.QueryStmt): AstRelRef {
  checkKnownRel(ctx, queryStmt.rel, nodePosition(queryStmt));
  checkArity(ctx, queryStmt.rel, queryStmt.args.length, nodePosition(queryStmt));
  return relRef(queryStmt.rel, ...queryStmt.args.map(toPositiveArg));
}

// ─────────────────────────────────────────────────────────────────────────────
// bridge() — the public entry point.
// ─────────────────────────────────────────────────────────────────────────────

export function bridge(dlText: string, builtinRels: readonly AstRelDecl[]): BridgeResult {
  const { program: parsedProgram, diags: parseDiags } = parseDlDocument(dlText);
  if (parseDiags.length > 0) return { kind: "err", diags: parseDiags };

  const ctx = newContext();
  for (const builtin of builtinRels) {
    ctx.knownRelColumns.set(builtin.name, { columns: [...builtin.columns] });
    ctx.retention.set(builtin.name, "all");
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

  for (const decl of relDecls) processRelDecl(ctx, decl);
  for (const decl of shDecls) processShDecl(ctx, decl);

  const userRules = dlRules.map((dlRule) => processDlRule(ctx, dlRule));
  const queries = queryStmts.map((queryStmt) => processQueryStmt(ctx, queryStmt));

  const rules: AstRule[] = [...userRules, ...ctx.mintedRules];

  if (ctx.diags.length > 0) return { kind: "err", diags: ctx.diags };

  // Origin: headed (by a USER rule) -> IDB; everything else stays EDB. Minted
  // __req_<host> rules always head their own rel, so those are always IDB too.
  const rels: AstRelDecl[] = [];
  for (const decl of relDecls) {
    const info = ctx.knownRelColumns.get(decl.name)!;
    rels.push(ctx.headedRelNames.has(decl.name) ? derivedRel(decl.name, info.columns) : edbRel(decl.name, info.columns));
  }
  for (const builtin of builtinRels) {
    rels.push(ctx.headedRelNames.has(builtin.name) ? derivedRel(builtin.name, [...builtin.columns]) : builtin);
  }
  for (const host of ctx.hostsByName.values()) {
    rels.push(edbRel(`__resp_${host.name}`, host.columns.map((column) => column.name)));
    rels.push(derivedRel(`__req_${host.name}`, [...host.inputCols]));
  }
  for (const name of ctx.literalSeeds.keys()) {
    rels.push(edbRel(name, ["value"]));
  }

  const builtProgram: AstProgram = { rels, rules };

  try {
    const graph = buildRuleGraph(builtProgram);
    stratify(graph, scc(graph));
  } catch (err) {
    if (err instanceof NonStratifiableError) {
      return { kind: "err", diags: [{ code: "non-stratifiable", message: err.message, line: 0, col: 0 }] };
    }
    throw err;
  }

  return {
    kind: "ok",
    program: builtProgram,
    hosts: [...ctx.hostsByName.values()],
    retention: ctx.retention,
    queries,
    minted: ctx.minted,
    literalSeeds: ctx.literalSeeds,
  };
}
