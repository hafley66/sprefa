#!/usr/bin/env node
// TypeScript checker oracle: walk every CallExpression, resolve it through
// the real checker. Emits normal-form tsvs: src_path src_name dst_path dst_name.
import ts from "typescript";
import fs from "node:fs";
import path from "node:path";

const corpusRoot = process.argv[2] || "/Users/chrishafley/projects/TypeScript-5.9";
const outDir = process.argv[3] || path.dirname(new URL(import.meta.url).pathname);
const srcRoot = path.join(corpusRoot, "src");

function readJson(file) {
  const text = fs.readFileSync(file, "utf8");
  const result = ts.parseConfigFileTextToJson(file, text);
  return result.config;
}

// BFS the project-reference graph starting at src/tsconfig.json, collecting
// every subproject's tsconfig.json path.
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
      const refCfg = fs.statSync(refPath).isDirectory()
        ? path.join(refPath, "tsconfig.json")
        : refPath;
      queue.push(refCfg);
    }
  }
  seen.delete(entry);
  return [...seen];
}

const entryConfig = path.join(srcRoot, "tsconfig.json");
const subprojects = collectSubprojects(entryConfig);
console.error(`subprojects: ${subprojects.length}`);

const rootNames = new Set();
for (const cfgPath of subprojects) {
  const parsed = ts.getParsedCommandLineOfConfigFile(cfgPath, {}, {
    fileExists: ts.sys.fileExists,
    readFile: ts.sys.readFile,
    readDirectory: ts.sys.readDirectory,
    useCaseSensitiveFileNames: true,
    getCurrentDirectory: () => path.dirname(cfgPath),
    onUnRecoverableConfigFileDiagnostic: (d) => console.error(ts.flattenDiagnosticMessageText(d.messageText, "\n")),
  });
  if (!parsed) continue;
  for (const f of parsed.fileNames) rootNames.add(f);
}
console.error(`root files: ${rootNames.size}`);

// One merged compiler-options object over the whole src tree so the checker
// resolves calls that cross subproject boundaries (compiler -> services etc).
const baseConfig = readJson(path.join(srcRoot, "tsconfig-base.json"));
const baseOptions = ts.convertCompilerOptionsFromJson(
  baseConfig.compilerOptions,
  srcRoot,
).options;
const options = {
  ...baseOptions,
  skipLibCheck: true,
  noEmit: true,
  composite: false,
  incremental: false,
  declaration: false,
  declarationMap: false,
  emitDeclarationOnly: false,
  isolatedDeclarations: false,
  types: [],
};

console.error("creating program...");
const t0 = Date.now();
const program = ts.createProgram([...rootNames], options);
const checker = program.getTypeChecker();
console.error(`program created in ${Date.now() - t0}ms, ${program.getSourceFiles().length} source files`);

function relPath(absPath) {
  return path.relative(corpusRoot, absPath);
}

// Nearest named enclosing function/method/class, or a named var bound to one.
// Falls back to "<module>".
function enclosingName(node) {
  let cur = node;
  while (cur) {
    if (
      ts.isFunctionDeclaration(cur) ||
      ts.isMethodDeclaration(cur) ||
      ts.isGetAccessorDeclaration(cur) ||
      ts.isSetAccessorDeclaration(cur) ||
      ts.isFunctionExpression(cur)
    ) {
      if (cur.name) return cur.name.getText();
      if (ts.isVariableDeclaration(cur.parent) && ts.isIdentifier(cur.parent.name)) {
        return cur.parent.name.text;
      }
      if (ts.isPropertyAssignment(cur.parent) && ts.isIdentifier(cur.parent.name)) {
        return cur.parent.name.text;
      }
    }
    if (ts.isArrowFunction(cur)) {
      if (ts.isVariableDeclaration(cur.parent) && ts.isIdentifier(cur.parent.name)) {
        return cur.parent.name.text;
      }
      if (ts.isPropertyAssignment(cur.parent) && ts.isIdentifier(cur.parent.name)) {
        return cur.parent.name.text;
      }
      if (ts.isPropertyDeclaration(cur.parent) && ts.isIdentifier(cur.parent.name)) {
        return cur.parent.name.text;
      }
    }
    if (ts.isConstructorDeclaration(cur)) {
      const cls = cur.parent;
      if (ts.isClassDeclaration(cls) && cls.name) return `${cls.name.text}.constructor`;
      return "constructor";
    }
    if (ts.isClassDeclaration(cur) && cur.name) {
      return cur.name.text;
    }
    cur = cur.parent;
  }
  return "<module>";
}

function declarationName(decl) {
  if (!decl) return null;
  if (decl.name && ts.isIdentifier(decl.name)) return decl.name.text;
  if (ts.isConstructorDeclaration(decl)) {
    const cls = decl.parent;
    if (ts.isClassDeclaration(cls) && cls.name) return `${cls.name.text}.constructor`;
    return "constructor";
  }
  if (ts.isMethodDeclaration(decl) || ts.isMethodSignature(decl)) {
    if (decl.name) return decl.name.getText();
  }
  if (ts.isVariableDeclaration(decl.parent) && ts.isIdentifier(decl.parent.name)) {
    return decl.parent.name.text;
  }
  return enclosingName(decl);
}

const callRows = [];
const isCorpusFile = (fileName) => fileName.startsWith(srcRoot + path.sep) && !fileName.endsWith(".d.ts");

let visitedFiles = 0;
for (const sourceFile of program.getSourceFiles()) {
  if (!isCorpusFile(sourceFile.fileName)) continue;
  visitedFiles++;
  const srcPath = relPath(sourceFile.fileName);

  function visit(node) {
    if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const signature = checker.getResolvedSignature(node);
      const decl = signature && signature.declaration;
      if (decl && decl.getSourceFile) {
        const declFile = decl.getSourceFile();
        if (declFile && !declFile.fileName.includes("/node_modules/") && !declFile.isDeclarationFile) {
          const dstPath = relPath(declFile.fileName);
          const dstName = declarationName(decl) || "<anonymous>";
          const srcName = enclosingName(node);
          callRows.push(`${srcPath}\t${srcName}\t${dstPath}\t${dstName}`);
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
}
console.error(`visited ${visitedFiles} corpus files, ${callRows.length} resolved call edges`);

fs.writeFileSync(path.join(outDir, "ts.oracle.call.tsv"), callRows.join("\n") + "\n");

// Imports: resolve each import/export specifier through ts.resolveModuleName
// with the same compiler options, host, and per-file redirect context.
const moduleHost = {
  fileExists: ts.sys.fileExists,
  readFile: ts.sys.readFile,
  directoryExists: ts.sys.directoryExists,
  getCurrentDirectory: () => srcRoot,
  getDirectories: ts.sys.getDirectories,
  realpath: ts.sys.realpath,
  useCaseSensitiveFileNames: true,
};

const moduleRows = [];
const moduleCache = ts.createModuleResolutionCache(srcRoot, (f) => f, options);
for (const sourceFile of program.getSourceFiles()) {
  if (!isCorpusFile(sourceFile.fileName)) continue;
  const srcPath = relPath(sourceFile.fileName);
  const containingDir = path.dirname(sourceFile.fileName);

  function specifierOf(node) {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      return node.moduleSpecifier.text;
    }
    if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
      const expr = node.moduleReference.expression;
      if (ts.isStringLiteral(expr)) return expr.text;
    }
    return null;
  }

  for (const stmt of sourceFile.statements) {
    const spec = specifierOf(stmt);
    if (!spec) continue;
    const resolved = ts.resolveModuleName(spec, sourceFile.fileName, options, moduleHost, moduleCache);
    const resolvedModule = resolved.resolvedModule;
    if (resolvedModule && !resolvedModule.isExternalLibraryImport && !resolvedModule.resolvedFileName.includes("/node_modules/")) {
      const dstPath = relPath(resolvedModule.resolvedFileName);
      moduleRows.push(`${srcPath}\t\t${dstPath}\t`);
    }
  }
}
console.error(`resolved ${moduleRows.length} module edges`);
fs.writeFileSync(path.join(outDir, "ts.oracle.module.tsv"), moduleRows.join("\n") + "\n");

console.error(`total wall: ${Date.now() - t0}ms`);
