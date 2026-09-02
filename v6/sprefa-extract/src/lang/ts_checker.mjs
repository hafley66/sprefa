// stdin `{"root","files":[["<supplied>","<abs>"],...],"tsi":<bool>}`; stdout one
// `{"path","calls","types"}` line per answered file then one `{"stats"}` line.
// @comment-ok: the seam's whole wire format, stated where both ends read it.
// Every offset is a UTF-8 BYTE offset, the unit `to_span` writes; TypeScript
// counts UTF-16 code units, so positions cross `byteMapper` before emission.
// Row: [start, end, name, dstPath, dstName, dstOffset]; dstPath "" = external.
// @comment-ok: `tsi: true` adds a `tsi` key per file line, `[relation, arg, ...]`
// in the wire's tagged argument shape, and `coverage` on the stats line,
// `[relation, complete, diagnostic|null]`, a claim about the whole run.

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
function compilerOptions(ts, root, near) {
    // Searched from a SUPPLIED FILE first: a repo whose projects each carry
    // their own tsconfig has none at its root, and default options resolve no
    // ESM import in it.
    const found =
        (near && ts.findConfigFile(near, ts.sys.fileExists, "tsconfig.json")) ||
        ts.findConfigFile(root, ts.sys.fileExists, "tsconfig.json");
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

/// Ids are run-local across the whole program, minted on first sight, so a type
/// seen twice is one id and a recursive type closes through it.
function tsiState() {
    return {
        next: 0,
        typeIds: new Map(),
        symbolIds: new Map(),
        walked: new Set(),
        emitted: new Set(),
        rows: [],
    };
}

/// The relation set a whole run enumerates, and the ones it only samples. A
/// claim is written only for a relation that actually got a row.
const TSI_COMPLETE = [
    "tsi.type",
    "tsi.symbol",
    "tsi.denotes",
    "tsi.has_type",
    "tsi.origin",
    "tsi.product",
    "tsi.sum",
    "tsi.callable",
    "tsi.primitive",
    "tsi.parameter",
    "tsi.called",
    "tsi.argument",
    "tsi.input",
    "tsi.output",
    "ts.interface",
    "ts.optional",
    "ts.readonly",
    "ts.mapped",
    "ts.conditional",
];

const TSI_PARTIAL = [
    ["tsi.edge", "edges are enumerated for corpus-declared owners; a lib or dependency type is a leaf"],
    ["tsi.conforms", "declared heritage only; structural conformance not enumerated"],
    ["tsi.subtype", "not enumerated"],
    ["tsi.assignable", "not enumerated"],
    ["tsi.equivalent", "not enumerated"],
];

/// One signature's parameters occupy one stride of the `tsi.input` position
/// space; a signature with more parameters than the stride is a named stop.
const SIGNATURE_STRIDE = 1000;

function tsiRow(tsi, relation, ...args) {
    tsi.emitted.add(relation);
    tsi.rows.push([relation, ...args]);
}

/// A wanted file writes its SUPPLIED path and byte offsets, which the Rust side
/// swaps for the file's content digest; any other file writes its own name.
function spanArg(state, node) {
    const sourceFile = node.getSourceFile();
    const entry = state.byFileName.get(sourceFile.fileName);
    const start = node.getStart(sourceFile);
    const end = node.getEnd();
    if (!entry) return { span: [sourceFile.fileName, start, end] };
    return { span: [entry.supplied, entry.toByte(start), entry.toByte(end)] };
}

/// The declaration a type's origin names: the alias wins, so `type Q = ...`
/// origins at `Q` rather than at whatever the compiler interned underneath.
function typeDeclaration(type) {
    const symbol = type.aliasSymbol ?? type.symbol;
    return symbol?.declarations?.[0];
}

/// Literal types carry the atom of their widened class, so a consumer matching
/// on `string` sees every string-shaped type.
function primitiveClass(ts, type) {
    const flags = ts.TypeFlags;
    const classes = [
        [flags.StringLiteral, "string"],
        [flags.NumberLiteral, "number"],
        [flags.BooleanLiteral, "boolean"],
        [flags.BigIntLiteral, "bigint"],
        [flags.String, "string"],
        [flags.Number, "number"],
        [flags.Boolean, "boolean"],
        [flags.BigInt, "bigint"],
        [flags.ESSymbol, "symbol"],
        [flags.UniqueESSymbol, "symbol"],
        [flags.Void, "void"],
        [flags.Undefined, "undefined"],
        [flags.Null, "null"],
        [flags.Never, "never"],
        [flags.Unknown, "unknown"],
        [flags.Any, "any"],
    ];
    for (const [flag, word] of classes) {
        if (flag && type.flags & flag) return word;
    }
    return undefined;
}

function typeId(ts, state, tsi, type) {
    const known = tsi.typeIds.get(type);
    if (known !== undefined) return known;
    const ownerTypeId = tsi.next;
    tsi.next += 1;
    tsi.typeIds.set(type, ownerTypeId);
    tsiRow(tsi, "tsi.type", { id: ownerTypeId });
    const declaration = typeDeclaration(type);
    if (declaration) {
        const named = declaration.name ?? declaration;
        tsiRow(tsi, "tsi.origin", { id: ownerTypeId }, { atom: "ts" }, spanArg(state, named));
    }
    const primitive = primitiveClass(ts, type);
    if (primitive) tsiRow(tsi, "tsi.primitive", { id: ownerTypeId }, { atom: primitive });
    return ownerTypeId;
}

function symbolId(state, tsi, symbol) {
    const known = tsi.symbolIds.get(symbol);
    if (known !== undefined) return known;
    const id = tsi.next;
    tsi.next += 1;
    tsi.symbolIds.set(symbol, id);
    tsiRow(tsi, "tsi.symbol", { id });
    const named = symbol.declarations?.[0]?.name;
    if (named) tsiRow(tsi, "tsi.origin", { id }, { atom: "ts" }, spanArg(state, named));
    return id;
}

/// A type whose declaration is in the resolve universe. The cost cap: only such
/// a type has its properties and signatures walked.
function corpusDeclared(state, type) {
    const symbol = type.aliasSymbol ?? type.symbol;
    const declarations = symbol?.declarations ?? [];
    return declarations.some((decl) => state.wanted.has(decl.getSourceFile().fileName));
}

/// Mint the target's id, and walk its own shape only when the corpus declared
/// it. A lib or dependency type stops here with an id and an origin.
function reachType(ts, checker, state, tsi, type) {
    const targetTypeId = typeId(ts, state, tsi, type);
    if (corpusDeclared(state, type)) walkType(ts, checker, state, tsi, type);
    return targetTypeId;
}

function propertyPosition(prop) {
    const declaration = prop.valueDeclaration ?? prop.declarations?.[0];
    return declaration ? declaration.pos : Number.MAX_SAFE_INTEGER;
}

function readonlyProperty(ts, checker, prop) {
    if (typeof checker.isReadonlySymbol === "function") {
        try {
            if (checker.isReadonlySymbol(prop)) return true;
        } catch {
            // an internal the loaded compiler does not expose the same way
        }
    }
    const declaration = prop.valueDeclaration ?? prop.declarations?.[0];
    const modifiers = declaration && ts.canHaveModifiers?.(declaration)
        ? ts.getModifiers(declaration)
        : declaration?.modifiers;
    return (modifiers ?? []).some((modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword);
}

function walkProperties(ts, checker, state, tsi, ownerTypeId, type) {
    const properties = checker.getPropertiesOfType(type).slice();
    properties.sort((left, right) => propertyPosition(left) - propertyPosition(right));
    properties.forEach((prop, position) => {
        const at = prop.valueDeclaration ?? prop.declarations?.[0];
        const propertyType = at
            ? checker.getTypeOfSymbolAtLocation(prop, at)
            : checker.getTypeOfSymbol(prop);
        const targetTypeId = reachType(ts, checker, state, tsi, propertyType);
        const edgeId = tsi.next;
        tsi.next += 1;
        tsiRow(
            tsi,
            "tsi.edge",
            { id: edgeId },
            { id: ownerTypeId },
            { text: prop.name },
            { id: targetTypeId },
            { int: position },
        );
        if (prop.flags & ts.SymbolFlags.Optional) tsiRow(tsi, "ts.optional", { id: edgeId });
        if (readonlyProperty(ts, checker, prop)) tsiRow(tsi, "ts.readonly", { id: edgeId });
    });
}

function walkParameters(ts, checker, state, tsi, calleeTypeId, parameters) {
    (parameters ?? []).forEach((parameter, position) => {
        const parameterTypeId = typeId(ts, state, tsi, parameter);
        tsiRow(
            tsi,
            "tsi.parameter",
            { id: parameterTypeId },
            { id: calleeTypeId },
            { int: position },
            { atom: "unspecified" },
        );
        const constraint = checker.getBaseConstraintOfType(parameter);
        if (!constraint || constraint === parameter) return;
        const boundTypeId = reachType(ts, checker, state, tsi, constraint);
        const edgeId = tsi.next;
        tsi.next += 1;
        tsiRow(
            tsi,
            "tsi.edge",
            { id: edgeId },
            { id: parameterTypeId },
            { text: "bound" },
            { id: boundTypeId },
            { int: 0 },
        );
    });
}

function walkSignatures(ts, checker, state, tsi, ownerTypeId, type) {
    let callable = false;
    for (const kind of [ts.SignatureKind.Call, ts.SignatureKind.Construct]) {
        const signatures = checker.getSignaturesOfType(type, kind);
        if (signatures.length === 0) continue;
        if (!callable) {
            tsiRow(tsi, "tsi.callable", { id: ownerTypeId });
            callable = true;
        }
        signatures.forEach((signature, index) => {
            walkParameters(ts, checker, state, tsi, ownerTypeId, signature.typeParameters);
            if (signature.parameters.length > SIGNATURE_STRIDE) {
                throw new Error(`a signature with ${signature.parameters.length} parameters exceeds the tsi.input stride`);
            }
            signature.parameters.forEach((parameter, position) => {
                const at = parameter.valueDeclaration ?? parameter.declarations?.[0];
                const parameterType = at
                    ? checker.getTypeOfSymbolAtLocation(parameter, at)
                    : checker.getTypeOfSymbol(parameter);
                tsiRow(
                    tsi,
                    "tsi.input",
                    { id: ownerTypeId },
                    { int: index * SIGNATURE_STRIDE + position },
                    { id: reachType(ts, checker, state, tsi, parameterType) },
                );
            });
            const returned = checker.getReturnTypeOfSignature(signature);
            tsiRow(
                tsi,
                "tsi.output",
                { id: ownerTypeId },
                { int: index },
                { id: reachType(ts, checker, state, tsi, returned) },
            );
        });
    }
}

/// An `extends` or `implements` clause is the one conformance a compiler states
/// rather than derives, which is why the relation stays partial.
function walkHeritage(ts, checker, state, tsi, ownerTypeId, type) {
    const declaration = typeDeclaration(type);
    const clauses = declaration?.heritageClauses ?? [];
    for (const clause of clauses) {
        for (const expression of clause.types) {
            const baseType = checker.getTypeAtLocation(expression);
            if (!baseType) continue;
            tsiRow(
                tsi,
                "tsi.conforms",
                { id: ownerTypeId },
                { id: reachType(ts, checker, state, tsi, baseType) },
                { atom: "declared" },
            );
        }
    }
}

/// An instantiation caches the template on the generic mapped type it came
/// from, so each part is read from the instantiation and then from its target.
function walkMapped(ts, checker, state, tsi, ownerTypeId, type) {
    const generic = type.target ?? type;
    const parts = [
        type.typeParameter ?? generic.typeParameter,
        type.constraintType ?? generic.constraintType,
        type.templateType ?? generic.templateType,
    ];
    if (parts.some((part) => !part)) return;
    tsiRow(
        tsi,
        "ts.mapped",
        { id: ownerTypeId },
        ...parts.map((part) => ({ id: reachType(ts, checker, state, tsi, part) })),
    );
}

function walkConditional(ts, checker, state, tsi, ownerTypeId, type) {
    const node = type.root?.node;
    if (!node) return;
    const parts = [node.checkType, node.extendsType, node.trueType, node.falseType];
    if (parts.some((part) => !part)) return;
    tsiRow(
        tsi,
        "ts.conditional",
        { id: ownerTypeId },
        ...parts.map((part) => ({ id: reachType(ts, checker, state, tsi, checker.getTypeFromTypeNode(part)) })),
    );
}

/// The shape of one type. The `walked` guard is what closes a recursive type:
/// the second visit returns the id the first minted and stops.
function walkType(ts, checker, state, tsi, type) {
    const ownerTypeId = typeId(ts, state, tsi, type);
    if (tsi.walked.has(ownerTypeId)) return ownerTypeId;
    tsi.walked.add(ownerTypeId);

    const objectFlags = type.flags & ts.TypeFlags.Object ? type.objectFlags : 0;
    const shaped = ts.ObjectFlags.Class | ts.ObjectFlags.Interface | ts.ObjectFlags.Anonymous | ts.ObjectFlags.Mapped;
    if (objectFlags & shaped) tsiRow(tsi, "tsi.product", { id: ownerTypeId });
    if (objectFlags & ts.ObjectFlags.Interface) tsiRow(tsi, "ts.interface", { id: ownerTypeId });

    // `boolean` is a union of two literals and a primitive at once; the
    // primitive class is the reading a consumer matches on.
    if (type.flags & ts.TypeFlags.Union && !primitiveClass(ts, type)) {
        tsiRow(tsi, "tsi.sum", { id: ownerTypeId });
        (type.types ?? []).forEach((member, position) => {
            const memberTypeId = reachType(ts, checker, state, tsi, member);
            const edgeId = tsi.next;
            tsi.next += 1;
            tsiRow(
                tsi,
                "tsi.edge",
                { id: edgeId },
                { id: ownerTypeId },
                { text: "" },
                { id: memberTypeId },
                { int: position },
            );
        });
    }

    // A generic type is its own target, so only a REFERENCE with arguments of
    // its own is a type application. `this` rides the argument list; it is not written.
    const target = type.target;
    const written = (target?.localTypeParameters ?? target?.typeParameters ?? []).length;
    const arguments_ =
        target && target !== type ? (checker.getTypeArguments(type) ?? []).slice(0, written) : [];
    if (arguments_.length > 0) {
        const listId = tsi.next;
        tsi.next += 1;
        tsiRow(
            tsi,
            "tsi.called",
            { id: ownerTypeId },
            { id: reachType(ts, checker, state, tsi, target) },
            { id: listId },
        );
        arguments_.forEach((argument, position) => {
            tsiRow(
                tsi,
                "tsi.argument",
                { id: listId },
                { int: position },
                { id: reachType(ts, checker, state, tsi, argument) },
            );
        });
    } else {
        walkParameters(ts, checker, state, tsi, ownerTypeId, type.typeParameters);
    }

    walkProperties(ts, checker, state, tsi, ownerTypeId, type);
    walkSignatures(ts, checker, state, tsi, ownerTypeId, type);
    walkHeritage(ts, checker, state, tsi, ownerTypeId, type);
    // A mapped type's key parameter, constraint and template are computed when
    // its members resolve, so the property walk runs first or they read empty.
    if (objectFlags & ts.ObjectFlags.Mapped) walkMapped(ts, checker, state, tsi, ownerTypeId, type);
    if (type.flags & ts.TypeFlags.Conditional) walkConditional(ts, checker, state, tsi, ownerTypeId, type);
    return ownerTypeId;
}

/// A declaration the walk enters from: the symbol is declared, and the type it
/// denotes is walked whole. An ALIAS mints no type id of its own.
function walkDeclaration(ts, checker, state, tsi, node) {
    const named = node.name && ts.isIdentifier(node.name) ? node.name : undefined;
    if (!named) return;
    const symbol = checker.getSymbolAtLocation(named);
    if (!symbol) return;
    const declared =
        ts.isInterfaceDeclaration(node) ||
        ts.isClassDeclaration(node) ||
        ts.isTypeAliasDeclaration(node) ||
        ts.isEnumDeclaration(node);
    const type = declared
        ? checker.getDeclaredTypeOfSymbol(symbol)
        : checker.getTypeOfSymbolAtLocation(symbol, named);
    if (!type) return;
    const denotedTypeId = walkType(ts, checker, state, tsi, type);
    tsiRow(tsi, "tsi.denotes", { id: symbolId(state, tsi, symbol) }, { id: denotedTypeId });
}

/// The type at one written range. An error type is the compiler saying it does
/// not know, which is not a fact.
function walkOccurrence(ts, checker, state, tsi, node, type) {
    if (!type || type.intrinsicName === "error") return;
    const occurrenceTypeId = reachType(ts, checker, state, tsi, type);
    tsiRow(tsi, "tsi.has_type", spanArg(state, node), { id: occurrenceTypeId });
}

/// A written `Name<Args>` names the APPLICATION, where the identifier inside it
/// names the generic being applied; both are occurrences and they differ.
function typeOf(ts, checker, node) {
    try {
        return ts.isTypeReferenceNode(node) || ts.isExpressionWithTypeArguments(node)
            ? checker.getTypeFromTypeNode(node)
            : checker.getTypeAtLocation(node);
    } catch {
        return undefined;
    }
}

function emitTsi(ts, checker, state) {
    const tsi = state.tsi;
    const before = tsi.rows.length;
    const sourceFile = state.file.sourceFile;
    const entered = (node) =>
        ts.isInterfaceDeclaration(node) ||
        ts.isClassDeclaration(node) ||
        ts.isTypeAliasDeclaration(node) ||
        ts.isEnumDeclaration(node) ||
        ts.isFunctionDeclaration(node) ||
        ts.isMethodDeclaration(node) ||
        ts.isMethodSignature(node) ||
        (ts.isVariableDeclaration(node) &&
            node.initializer &&
            (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer)));
    const occurrence = (node) =>
        ts.isIdentifier(node) ||
        ((ts.isTypeReferenceNode(node) || ts.isExpressionWithTypeArguments(node)) &&
            (node.typeArguments ?? []).length > 0);
    const visit = (node) => {
        if (entered(node)) walkDeclaration(ts, checker, state, tsi, node);
        if (occurrence(node)) {
            walkOccurrence(ts, checker, state, tsi, node, typeOf(ts, checker, node));
        }
        ts.forEachChild(node, visit);
    };
    ts.forEachChild(sourceFile, visit);
    return tsi.rows.slice(before);
}

/// A claim is written only for a relation that got a row: `complete` over an
/// empty relation is a producer defect the reverse door rejects.
function tsiCoverage(tsi) {
    const claims = [];
    for (const relation of TSI_COMPLETE) {
        if (tsi.emitted.has(relation)) claims.push([relation, true, null]);
    }
    for (const [relation, detail] of TSI_PARTIAL) {
        claims.push([relation, false, detail]);
    }
    return claims;
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
    if (!state.tsi) return { path: file.supplied, calls, types };
    return { path: file.supplied, calls, types, tsi: emitTsi(ts, checker, state) };
}

function main() {
    const request = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
    const root = request.root;
    const { ts, from } = loadTypeScript(root);
    process.stderr.write(`ts checker: typescript from ${from}\n`);

    const loadStarted = Date.now();
    const near = request.files.length > 0 ? path.dirname(request.files[0][1]) : undefined;
    const options = compilerOptions(ts, root, near);
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
    const tsi = request.tsi === true ? tsiState() : undefined;
    const out = [];
    for (const file of answered) {
        out.push(JSON.stringify(answerFile(ts, checker, { file, wanted, byFileName, tsi })));
        if (out.length >= 64) {
            process.stdout.write(out.join("\n") + "\n");
            out.length = 0;
        }
    }
    if (out.length > 0) process.stdout.write(out.join("\n") + "\n");
    const stats = { loadMs, walkMs: Date.now() - walkStarted, files: answered.length };
    const closing = tsi ? { stats, coverage: tsiCoverage(tsi) } : { stats };
    process.stdout.write(JSON.stringify(closing) + "\n");
}

main();
