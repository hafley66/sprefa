// Classify a sample of agreed-and-missed ts call edges ((codeql2 ∩ oracle) − ours)
// with the real tsc TypeChecker. Usage:
//   node ts.codeql_agreed_missed.classify.mjs <sample.tsv> <out.tsv>
// Sample rows: src_path \t src_name \t dst_path \t dst_name
import ts from "typescript";
import fs from "node:fs";
import path from "node:path";

const corpusRoot = "/Users/chrishafley/projects/TypeScript-5.9";
const srcRoot = path.join(corpusRoot, "src");
const [samplePath, outPath] = process.argv.slice(2);

function readJson(file) {
  const text = fs.readFileSync(file, "utf8");
  return ts.parseConfigFileTextToJson(file, text).config;
}
function collectSubprojects(entry) {
  const seen = new Set();
  const queue = [entry];
  while (queue.length) {
    const cfgPath = queue.shift();
    if (seen.has(cfgPath)) continue;
    seen.add(cfgPath);
    const config = readJson(cfgPath);
    const basePath = path.dirname(cfgPath);
    for (const ref of config.references || []) {
      const refPath = path.resolve(basePath, ref.path);
      const refCfg = fs.statSync(refPath).isDirectory() ? path.join(refPath, "tsconfig.json") : refPath;
      queue.push(refCfg);
    }
  }
  seen.delete(entry);
  return [...seen];
}

const baseConfig = readJson(path.join(srcRoot, "tsconfig-base.json"));
const options = {
  ...ts.convertCompilerOptionsFromJson(baseConfig.compilerOptions, srcRoot).options,
  skipLibCheck: true, noEmit: true, composite: false, incremental: false,
  declaration: false, declarationMap: false, emitDeclarationOnly: false,
  isolatedDeclarations: false, types: [],
};
const rootNames = new Set();
for (const cfgPath of collectSubprojects(path.join(srcRoot, "tsconfig.json"))) {
  const parsed = ts.getParsedCommandLineOfConfigFile(cfgPath, {}, {
    fileExists: ts.sys.fileExists, readFile: ts.sys.readFile,
    readDirectory: ts.sys.readDirectory, useCaseSensitiveFileNames: true,
    getCurrentDirectory: () => path.dirname(cfgPath),
    onUnRecoverableConfigFileDiagnostic: (d) => {},
  });
  if (!parsed) continue;
  for (const f of parsed.fileNames) rootNames.add(f);
}
console.error("creating program...");
const t0 = Date.now();
const program = ts.createProgram([...rootNames], options);
const checker = program.getTypeChecker();
console.error(`program in ${Date.now() - t0}ms, ${program.getSourceFiles().length} files`);

function enclosingName(node) {
  let cur = node;
  while (cur) {
    if ((ts.isFunctionDeclaration(cur) || ts.isMethodDeclaration(cur) ||
         ts.isGetAccessorDeclaration(cur) || ts.isSetAccessorDeclaration(cur) ||
         ts.isFunctionExpression(cur)) && cur.name) return cur.name.getText();
    if ((ts.isArrowFunction(cur) || ts.isFunctionExpression(cur)) &&
        (ts.isVariableDeclaration(cur.parent) || ts.isPropertyAssignment(cur.parent) ||
         ts.isPropertyDeclaration(cur.parent)) && ts.isIdentifier(cur.parent.name)) {
      return cur.parent.name.text;
    }
    if (ts.isConstructorDeclaration(cur)) {
      const cls = cur.parent;
      return ts.isClassDeclaration(cls) && cls.name ? `${cls.name.text}.constructor` : "constructor";
    }
    if (ts.isClassDeclaration(cur) && cur.name) return cur.name.text;
    cur = cur.parent;
  }
  return "<module>";
}

function calleeNameOf(call) {
  const e = call.expression;
  if (ts.isPropertyAccessExpression(e) || ts.isElementAccessExpression(e)) return e.name?.text;
  if (ts.isIdentifier(e)) return ts.isNewExpression(call) ? `${e.text}.constructor` : e.text;
  return null;
}

function typeLabel(t) {
  return checker.typeToString(t, undefined, ts.TypeFormatFlags.NoTruncation).slice(0, 120);
}

function classifySite(call, file) {
  const e = call.expression;
  if (!ts.isPropertyAccessExpression(e)) {
    const sym = ts.isIdentifier(e) ? checker.getSymbolAtLocation(e) : undefined;
    let aliased = sym;
    try {
      if (sym && (sym.flags & ts.SymbolFlags.Alias)) aliased = checker.getAliasedSymbol(sym);
    } catch { aliased = sym; }
    const dfile = aliased?.declarations?.[0]?.getSourceFile();
    const local = dfile && dfile === file;
    if (aliased && (aliased.flags & ts.SymbolFlags.Class)) {
      return { cls: "other: constructor call on a local class", detail: `decl=${dfile ? path.relative(corpusRoot, dfile.fileName) : "none"}` };
    }
    if (aliased && (aliased.flags & ts.SymbolFlags.Function)) {
      return { cls: local ? "other: bare call, local function" : "bare call, callee imported", detail: `decl=${dfile ? path.relative(corpusRoot, dfile.fileName) : "none"}` };
    }
    if (aliased && (aliased.flags & ts.SymbolFlags.Variable)) {
      const vt = checker.getTypeOfSymbolAtLocation(aliased, e);
      return { cls: vt.getCallSignatures().length ? "bare call, callee imported" : "other: bare call, variable callee", detail: `decl=${dfile ? path.relative(corpusRoot, dfile.fileName) : "none"}` };
    }
    return { cls: "other: bare call (no receiver)", detail: "" };
  }
  const recv = e.expression;
  const recvText = recv.getText(file);
  const type = checker.getTypeAtLocation(recv);
  const sig = checker.getResolvedSignature(call);
  const propSym = checker.getPropertySymbolOfType ?
    null : null;

  // this.field receiver
  if (ts.isPropertyAccessExpression(recv) && recv.expression.kind === ts.SyntaxKind.ThisKeyword) {
    return { cls: "receiver from a `this.` field", detail: `recv=${recvText} type=${typeLabel(type)}` };
  }

  // union / intersection
  if (type.isUnion && type.isUnion()) {
    return { cls: "receiver typed through a union or intersection", detail: `recv=${recvText} type=${typeLabel(type)}` };
  }
  if (type.isIntersection && type.isIntersection()) {
    return { cls: "receiver typed through a union or intersection", detail: `recv=${recvText} type=${typeLabel(type)}` };
  }

  // namespace member through a (nested) namespace
  let sym = checker.getSymbolAtLocation(recv);
  if (sym && sym.flags & (ts.SymbolFlags.Namespace | ts.SymbolFlags.Module)) {
    const target = sym.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(sym) : sym;
    if (target && target.flags & (ts.SymbolFlags.Namespace | ts.SymbolFlags.Module)) {
      return { cls: "namespace member through a (nested) namespace", detail: `recv=${recvText}` };
    }
  }
  // property of a namespace object (import * as ns / namespace-qualified)
  if (recvText.includes(".") || (sym && (sym.flags & ts.SymbolFlags.Alias))) {
    const aliased = sym && checker.getAliasedSymbol(sym);
    if (aliased && (aliased.flags & (ts.SymbolFlags.Namespace | ts.SymbolFlags.Module))) {
      return { cls: "namespace member through a (nested) namespace", detail: `recv=${recvText}` };
    }
  }

  // callback param typed by the callee's signature: receiver decl is a
  // parameter / field whose type has call signatures
  const declSym = sym && (sym.flags & ts.SymbolFlags.Alias) ? checker.getAliasedSymbol(sym) : sym;
  const decl = declSym?.valueDeclaration;
  if (decl && (ts.isParameter(decl) || ts.isPropertyDeclaration(decl) || ts.isVariableDeclaration(decl))) {
    const dt = checker.getTypeAtLocation(decl.name ?? decl);
    if (dt.getCallSignatures().length > 0) {
      return { cls: "callback param typed by the callee's signature", detail: `recv=${recvText} type=${typeLabel(dt)}` };
    }
  }

  // method on a class instance created in another file: decl is a
  // variable/class-field with an initializer call or new expression whose
  // resolved signature lives in another file
  if (decl && (ts.isVariableDeclaration(decl) || ts.isPropertyDeclaration(decl)) && decl.initializer) {
    const init = decl.initializer;
    if (ts.isCallExpression(init) || ts.isNewExpression(init)) {
      const isig = checker.getResolvedSignature(init);
      const idecl = isig && isig.declaration;
      const crossFile = idecl && idecl.getSourceFile() !== file;
      if (crossFile) {
        return { cls: "method on a class instance created in another file", detail: `recv=${recvText} init=${init.getText(file).slice(0, 80)}` };
      }
    }
  }

  // interface receiver whose method signature lives in another file
  if (type.symbol && type.symbol.declarations?.some(d => ts.isInterfaceDeclaration(d))) {
    return { cls: "interface receiver (signature in another file)", detail: `recv=${recvText} type=${typeLabel(type)}` };
  }

  // generic instantiation: receiver type carries type parameters (T extends X)
  if (type.aliasTypeArguments?.length || type.typeParameters?.length ||
      (type.symbol && type.symbol.declarations?.some(d => d.typeParameters?.length && !ts.isTypeAliasDeclaration(d)))) {
    const tps = type.aliasTypeArguments?.length ? "alias" : (type.typeParameters?.length ? "type" : "decl");
    return { cls: "generic instantiation", detail: `recv=${recvText} type=${typeLabel(type)} tp=${tps}` };
  }

  // overload set: resolved member symbol has multiple declarations
  if (ts.isPropertyAccessExpression(e)) {
    const prop = checker.getSymbolAtLocation(e.name);
    const resolved = sig?.declaration;
    if (prop && prop.declarations && prop.declarations.length > 1 && resolved) {
      const names = new Set(prop.declarations.map(d => d.getSourceFile().fileName + ":" + (d.name?.getText() ?? "")));
      if (names.size > 1 || prop.declarations.length > 1) {
        return { cls: "overload set", detail: `recv=${recvText} decls=${prop.declarations.length}` };
      }
    }
  }

  if (type.symbol && type.symbol.declarations?.some(d => ts.isClassDeclaration(d))) {
    return { cls: "concrete class receiver, method in another file", detail: `recv=${recvText} type=${typeLabel(type)}` };
  }
  const declFile = sig?.declaration?.getSourceFile();
  return {
    cls: "other",
    detail: `recv=${recvText} type=${typeLabel(type)} sigDecl=${declFile ? path.relative(corpusRoot, declFile.fileName) : "none"}`,
  };
}

const rows = fs.readFileSync(samplePath, "utf8").split("\n").filter(Boolean);
const out = [];
const counter = new Map();
for (const line of rows) {
  const [srcPath, srcName, dstPath, dstName] = line.split("\t");
  const abs = path.join(corpusRoot, srcPath);
  let cls = "MANUAL(site not locatable)", detail = "";
  if (fs.existsSync(abs)) {
    const sf = program.getSourceFile(abs);
    const matches = [];
    function visit(node) {
      if ((ts.isCallExpression(node) || ts.isNewExpression(node))) {
        const cn = calleeNameOf(node);
        const en = enclosingName(node);
        if (cn === dstName && (en === srcName || (srcName === "<module>" && en === "<module>"))) {
          matches.push(node);
        }
      }
      ts.forEachChild(node, visit);
    }
    visit(sf);
    let pick = matches[0];
    if (!pick) {
      function visitAny(node) {
        if ((ts.isCallExpression(node) || ts.isNewExpression(node)) && calleeNameOf(node) === dstName) {
          matches.push(node);
        }
        ts.forEachChild(node, visitAny);
      }
      if (!matches.length) visitAny(sf);
      pick = matches[0];
    }
    if (pick) {
      try {
        const r = classifySite(pick, sf);
        cls = r.cls; detail = r.detail;
      } catch (err) {
        cls = "MANUAL(checker threw)";
        detail = String(err?.message ?? err).slice(0, 80);
      }
    }
  }
  counter.set(cls, (counter.get(cls) ?? 0) + 1);
  out.push(`${srcName}\t${dstName}\t${cls}\t${detail.replace(/\t/g, " ")}`);
}
fs.writeFileSync(outPath, out.join("\n") + "\n");
for (const [c, n] of [...counter.entries()].sort((a, b) => b[1] - a[1])) console.error(`${n}\t${c}`);
