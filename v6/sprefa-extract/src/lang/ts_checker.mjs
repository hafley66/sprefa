// stdin `{"root","files":[["<supplied>","<abs>"],...]}`; stdout one
// `{"path","calls","types"}` line per answered file then one `{"stats"}` line.
// @comment-ok: the seam's whole wire format, stated where both ends read it.
// Every offset is a UTF-8 BYTE offset, the unit `to_span` writes; TypeScript
// counts UTF-16 code units, so positions cross `byteMapper` before emission.
// Row: [start, end, name, dstPath, dstName, dstOffset]; dstPath "" = external.

import { createRequire } from "node:module";
import path from "node:path";
import fs from "node:fs";

/// Resolved against the PROJECT, never against this script: the answers must
/// come from the compiler the project itself builds with.
function loadTypeScript(root) {
    const candidates = [];
    // An explicit pin wins: a monorepo whose compiler is not a dependency of
    // the checked project has no other way to name it.
    if (process.env.SPREFA_TS_CHECKER_TYPESCRIPT) {
        candidates.push(process.env.SPREFA_TS_CHECKER_TYPESCRIPT);
    }
    try {
        candidates.push(createRequire(path.join(root, "package.json")).resolve("typescript"));
    } catch {
        // no package.json, or typescript is not a dependency
    }
    // A TypeScript checkout carries no `node_modules/typescript`.
    candidates.push(path.join(root, "lib", "typescript.js"));
    candidates.push(path.join(root, "built", "local", "typescript.js"));
    try {
        candidates.push(createRequire(path.join(process.cwd(), "package.json")).resolve("typescript"));
    } catch {
        // no local package.json either
    }
    const require = createRequire(import.meta.url);
    for (const candidate of candidates) {
        try {
            if (candidate.endsWith(".js") && !fs.existsSync(candidate)) continue;
            return { ts: require(candidate), from: candidate };
        } catch {
            // try the next candidate
        }
    }
    throw new Error(`no typescript package reachable from ${root}`);
}

/// UTF-16 code-unit offset -> UTF-8 byte offset. An all-ASCII file converts by
/// identity and allocates nothing, which is nearly every file in a TS corpus.
function byteMapper(text) {
    let anyWide = false;
    for (let ix = 0; ix < text.length; ix += 1) {
        if (text.charCodeAt(ix) > 0x7f) {
            anyWide = true;
            break;
        }
    }
    if (!anyWide) return (position) => position;
    // Extra bytes one code UNIT costs beyond one: 1 for U+0080..U+07FF and for
    // each half of a surrogate pair (4 bytes over 2 units), 2 for the rest.
    const at = [];
    const prefix = [0];
    let extra = 0;
    for (let ix = 0; ix < text.length; ix += 1) {
        const unit = text.charCodeAt(ix);
        if (unit <= 0x7f) continue;
        at.push(ix);
        extra += unit <= 0x7ff || (unit >= 0xd800 && unit <= 0xdfff) ? 1 : 2;
        prefix.push(extra);
    }
    return (position) => {
        let low = 0;
        let high = at.length;
        while (low < high) {
            const mid = (low + high) >> 1;
            if (at[mid] < position) low = mid + 1;
            else high = mid;
        }
        return position + prefix[low];
    };
}

/// The project's own options, every emit-shaped constraint cleared: `composite`
/// and `isolatedDeclarations` reject a root set the tsconfig did not choose.
function compilerOptions(ts, root) {
    const found = ts.findConfigFile(root, ts.sys.fileExists, "tsconfig.json");
    let options = {};
    if (found) {
        const read = ts.readConfigFile(found, ts.sys.readFile);
        if (!read.error) {
            const parsed = ts.parseJsonConfigFileContent(read.config, ts.sys, path.dirname(found));
            options = parsed.options ?? {};
        }
    }
    return {
        ...options,
        noEmit: true,
        skipLibCheck: true,
        declaration: false,
        declarationMap: false,
        sourceMap: false,
        composite: false,
        incremental: false,
        emitDeclarationOnly: false,
        isolatedDeclarations: false,
        tsBuildInfoFile: undefined,
        outDir: undefined,
        rootDir: undefined,
    };
}

/// Corpus-first: a merged or overloaded symbol declares in several places and
/// the corpus one is the answer a resolve edge can carry.
function pickDeclaration(symbol, wanted) {
    const declarations = symbol?.declarations;
    if (!declarations || declarations.length === 0) return undefined;
    return declarations.find((decl) => wanted.has(decl.getSourceFile().fileName)) ?? declarations[0];
}

function unalias(checker, ts, symbol) {
    if (!symbol || !(symbol.flags & ts.SymbolFlags.Alias)) return symbol;
    try {
        return checker.getAliasedSymbol(symbol) ?? symbol;
    } catch {
        return symbol;
    }
}

/// An arrow function or a function expression carries no name of its own, so
/// its binding parent names it, which is how this crate's parse mints it too.
function namedNode(ts, decl) {
    if (decl.name && ts.isIdentifier(decl.name)) return decl;
    const parent = decl.parent;
    if (!parent) return undefined;
    const bound =
        ts.isVariableDeclaration(parent) ||
        ts.isPropertyDeclaration(parent) ||
        ts.isPropertyAssignment(parent) ||
        ts.isBindingElement(parent);
    return bound && parent.name && ts.isIdentifier(parent.name) ? parent : undefined;
}

/// One reference range plus one resolved declaration -> a seam row. A dst
/// outside the resolve universe is an ANSWER: the empty path is "not corpus".
function mint(ts, state, refStart, refEnd, decl) {
    if (!decl) return undefined;
    const { file, wanted, byFileName } = state;
    const target = decl.getSourceFile();
    const name = file.sourceFile.text.slice(refStart, refEnd);
    const start = file.toByte(refStart);
    const end = file.toByte(refEnd);
    if (!wanted.has(target.fileName)) return [start, end, name, "", "", 0];
    const named = namedNode(ts, decl);
    // A corpus declaration with no identifier is addressed by nothing this
    // crate's def index holds; the syntax leg answers it.
    if (!named) return undefined;
    const destination = byFileName.get(target.fileName);
    return [
        start,
        end,
        name,
        destination.supplied,
        named.name.text,
        destination.toByte(named.name.getStart(target)),
    ];
}

/// The trailing member of `a.b.c`, or a bare name. A computed callee (`a[k]()`)
/// names nothing, the same shape this crate's `callee_name` declines.
function calleeIdentifier(ts, expression) {
    if (ts.isPropertyAccessExpression(expression) && ts.isIdentifier(expression.name)) {
        return expression.name;
    }
    return ts.isIdentifier(expression) ? expression : undefined;
}

function answerFile(ts, checker, state) {
    const { file, wanted } = state;
    const sourceFile = file.sourceFile;
    const calls = [];
    const types = [];
    const visit = (node) => {
        if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
            const identifier = calleeIdentifier(ts, node.expression);
            if (identifier) {
                // The resolved SIGNATURE is the tier's whole point: it picks the
                // overload and the receiver's own member, which a name cannot.
                let decl = checker.getResolvedSignature(node)?.declaration;
                if (!decl) {
                    const symbol = unalias(checker, ts, checker.getSymbolAtLocation(identifier));
                    decl = pickDeclaration(symbol, wanted);
                }
                const row = mint(ts, state, identifier.getStart(sourceFile), identifier.getEnd(), decl);
                if (row) calls.push(row);
            }
        } else if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
            const tag = node.tagName;
            if (ts.isIdentifier(tag)) {
                const symbol = unalias(checker, ts, checker.getSymbolAtLocation(tag));
                const row = mint(ts, state, tag.getStart(sourceFile), tag.getEnd(), pickDeclaration(symbol, wanted));
                if (row) calls.push(row);
            }
        }
        if (ts.isTypeReferenceNode(node)) {
            const entity = node.typeName;
            const symbol = unalias(checker, ts, checker.getSymbolAtLocation(entity));
            const row = mint(ts, state, entity.getStart(sourceFile), entity.getEnd(), pickDeclaration(symbol, wanted));
            if (row) types.push(row);
        }
        ts.forEachChild(node, visit);
    };
    ts.forEachChild(sourceFile, visit);
    return { path: file.supplied, calls, types };
}

function main() {
    const request = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
    const root = request.root;
    const { ts, from } = loadTypeScript(root);
    process.stderr.write(`ts checker: typescript from ${from}\n`);

    const loadStarted = Date.now();
    const options = compilerOptions(ts, root);
    const supplied = new Map();
    const rootNames = [];
    for (const [name, absolute] of request.files) {
        let real = absolute;
        try {
            real = fs.realpathSync(absolute);
        } catch {
            // a path the walk listed and the filesystem no longer has
        }
        supplied.set(real, name);
        rootNames.push(real);
    }
    const program = ts.createProgram(rootNames, options);
    const checker = program.getTypeChecker();
    const loadMs = Date.now() - loadStarted;

    // Keyed on TypeScript's own `fileName`: its normalized spelling of the path
    // handed in, which a realpath key alone misses when the two disagree.
    const wanted = new Set();
    const byFileName = new Map();
    const answered = [];
    for (const sourceFile of program.getSourceFiles()) {
        const name = supplied.get(sourceFile.fileName) ?? supplied.get(path.resolve(sourceFile.fileName));
        if (name === undefined) continue;
        const entry = { supplied: name, sourceFile, toByte: byteMapper(sourceFile.text) };
        wanted.add(sourceFile.fileName);
        byFileName.set(sourceFile.fileName, entry);
        answered.push(entry);
    }

    const walkStarted = Date.now();
    const out = [];
    for (const file of answered) {
        out.push(JSON.stringify(answerFile(ts, checker, { file, wanted, byFileName })));
        if (out.length >= 64) {
            process.stdout.write(out.join("\n") + "\n");
            out.length = 0;
        }
    }
    if (out.length > 0) process.stdout.write(out.join("\n") + "\n");
    const stats = { loadMs, walkMs: Date.now() - walkStarted, files: answered.length };
    process.stdout.write(JSON.stringify({ stats }) + "\n");
}

main();
