/**
 * 0_ast_bridge.test.ts — M1 gate. One golden (the whole path, byte-pinned) plus unit
 * tests only for what the golden can't reach: the individual diag codes, the
 * elision-vs-arity-error split, retention forms, the wildcard-vs-ident pitfall, and
 * the literal-binding/compare fork (min tests max coverage, per tasks.d.ts law).
 *
 * REGEN_GOLDEN=1 node --test --experimental-transform-types tests/0_ast_bridge.test.ts
 * rewrites fixtures/golden/bridge.sg-rail.json from the current bridge() output —
 * use after a deliberate change to the bridge's minting/shape, never to silence a
 * real regression.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { bridge } from "../src/0_ast_bridge.ts";
import type { BridgeOk } from "../tasks.d.ts";
import { bridgeFixture, builtinRelsForTests, stableSerialize } from "./0_helpers.ts";

const GOLDEN_PATH = fileURLToPath(new URL("../fixtures/golden/bridge.sg-rail.json", import.meta.url));

function assertOk(result: ReturnType<typeof bridge>): asserts result is BridgeOk {
  assert.equal(result.kind, "ok", result.kind === "err" ? JSON.stringify(result.diags) : undefined);
}

test("golden: fixtures/sg-rail.dl bridges to the pinned snapshot", () => {
  const result = bridgeFixture("sg-rail.dl");
  assertOk(result);
  const actual = stableSerialize(result);

  if (process.env["REGEN_GOLDEN"] === "1") {
    writeFileSync(GOLDEN_PATH, `${JSON.stringify(actual, null, 2)}\n`);
    return;
  }
  const expected: unknown = JSON.parse(readFileSync(GOLDEN_PATH, "utf8"));
  assert.deepEqual(actual, expected);
});

test("unknown rel: a body ref to an undeclared rel is a load error", () => {
  const result = bridge("rel out(x: int).\nout(x) <- nope(x).\n", builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "unknown-rel"));
});

// M8-alpha collateral: `file` (builtin) is now 2-col (path, content_hash); bumped
// from 1 arg to 3 so this still passes MORE args than the rel's declared arity
// (ordinary rel refs, unlike probes, keep the pre-M8 "more args = load error" law;
// only probes gained the salt-arg exception, see the salt tests below).
test("arity: more args than declared arity is a load error", () => {
  const result = bridge(
    "rel out(path: text).\nout(path) <- file(path, extra, extra2).\n",
    builtinRelsForTests(),
  );
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "arity-mismatch"));
});

// UPDATED for named-arg resolution (NamedArgLaw, tasks.d.ts, owner scope change
// 2026-07-24): "this subsumes trailing elision" — a body atom's unfilled slots
// are now explicit `wild()` entries padded to the rel's FULL declared arity,
// not a short args array (the pre-kwargs shape this test used to assert).
test("arity: fewer args than declared arity elides (unfilled slots pad out to wild())", () => {
  const dlText = [
    "rel console_hit(path: text, start: int, end: int, text: text).",
    "rel out(path: text).",
    "out(path) <- console_hit(path).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const consoleHitRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "console_hit");
  assert.ok(consoleHitRef);
  assert.equal(consoleHitRef.kind, "rel");
  if (consoleHitRef.kind === "rel") {
    assert.deepEqual(consoleHitRef.args, [
      { kind: "var", name: "path" },
      { kind: "wild" },
      { kind: "wild" },
      { kind: "wild" },
    ]);
  }
});

test("Min/Max frontier: Min(int)/Max(int) parse but are rejected at load", () => {
  const result = bridge("rel bad(x: Min(int)).\n", builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") {
    assert.ok(result.diags.some((diag) => diag.code === "minmax-frontier" && diag.message.includes("frontier")));
  }
});

test("Key wrapper: Key(text) parses and is accepted (semantically inert this slice)", () => {
  const result = bridge("rel good(x: Key(text)).\n", builtinRelsForTests());
  assertOk(result);
});

test("mutation probe frontier: name!(...) parses but is rejected at load", () => {
  const dlText = [
    "sh sg(pattern: text, path: text, a: int, b: int, c: text) =",
    "  `sg run --pattern '{pattern}' --json $path`.",
    "rel out(path: text).",
    'out(path) <- sg!("x", path, a, b, c).',
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") {
    assert.ok(
      result.diags.some((diag) => diag.code === "mutation-frontier" && diag.message.includes("mutations land with a later slice")),
    );
  }
});

test("literal-binding: an eq compare on an otherwise-unbound var mints a __lit rel", () => {
  const dlText = ["rel out(severity: text).", 'out(severity) <- "warn" = severity.', ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const mintedRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel.startsWith("__lit_"));
  assert.ok(mintedRef, "expected a minted __lit_* body atom");
  assert.equal(mintedRef!.kind, "rel");
  if (mintedRef!.kind === "rel") {
    assert.equal(mintedRef!.args.length, 1);
    assert.deepEqual(mintedRef!.args[0], { kind: "var", name: "severity" });
    assert.equal(result.literalSeeds.get(mintedRef!.rel), "warn");
    assert.ok(result.minted.includes(mintedRef!.rel));
  }
  // no ordinary Compare predicate should exist for this rule (the var was unbound).
  assert.ok(!outRule.body.some((pred) => pred.kind === "cmp"));
});

test("literal-binding vs compare: a var already bound elsewhere stays an ordinary Compare", () => {
  const dlText = [
    "rel thing(severity: text).",
    "rel out(severity: text).",
    'out(severity) <- thing(severity), "warn" = severity.',
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const comparePred = outRule.body.find((pred) => pred.kind === "cmp");
  assert.ok(comparePred, "expected an ordinary Compare predicate (severity was already bound)");
  assert.equal(comparePred!.kind, "cmp");
  if (comparePred!.kind === "cmp") {
    assert.equal(comparePred!.op, "eq");
    assert.equal(comparePred!.lhs.name, "severity");
    assert.equal(comparePred!.rhs.value, "warn");
  }
  assert.ok(!outRule.body.some((pred) => pred.kind === "rel" && pred.rel.startsWith("__lit_")));
});

test("retention keyword pitfall: an ordinary int literal 0/1 in a rule body still parses", () => {
  // Regression: retention is spelled `rel(0)`/`rel(1)` in the grammar. If those
  // digits were keywords (the same trap PlainType.prim/AggCall.fn dodge for
  // "text"/"count" etc), the INT terminal would never match "0" or "1" anywhere
  // else in the language — breaking a plain body comparison like `count = 0`.
  const dlText = ["rel out(count: int).", "rel thing(count: int).", "out(count) <- thing(count), count = 0.", ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const comparePred = outRule.body.find((pred) => pred.kind === "cmp");
  assert.ok(comparePred, "expected an ordinary Compare against the int literal 0");
  assert.equal(comparePred!.kind, "cmp");
  if (comparePred!.kind === "cmp") assert.equal(comparePred!.rhs.value, 0);
});

test("retention forms: rel(0) = 0, rel(1) = 1, plain rel = all", () => {
  const dlText = ["rel(0) scratch(x: int).", "rel(1) state(x: int).", "rel plain(x: int).", ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  assert.equal(result.retention.get("scratch"), 0);
  assert.equal(result.retention.get("state"), 1);
  assert.equal(result.retention.get("plain"), "all");
});

test("query line: wildcards in a query atom parse as Wild, not literal vars", () => {
  const dlText = [
    "rel console_hit(path: text, start: int, end: int, text: text).",
    "? console_hit(path, _, _, _).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  assert.equal(result.queries.length, 1);
  const query = result.queries[0]!;
  assert.equal(query.rel, "console_hit");
  assert.equal(query.args.length, 4);
  assert.equal(query.args[1]!.kind, "wild");
  assert.equal(query.args[2]!.kind, "wild");
  assert.equal(query.args[3]!.kind, "wild");
});

test("wildcard `_` parses as Wildcard, not as a Var named \"_\"", () => {
  const dlText = ["rel thing(a: text, b: text).", "rel out(a: text).", "out(a) <- thing(a, _).", ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const thingRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "thing");
  assert.ok(thingRef);
  assert.equal(thingRef!.kind, "rel");
  if (thingRef!.kind === "rel") {
    assert.equal(thingRef!.args.length, 2);
    assert.equal(thingRef!.args[1]!.kind, "wild");
  }
});

test("diag defaults: end_line/end_col reuse line/col; hint binds via a null-seeded __lit", () => {
  const result = bridgeFixture("sg-rail.dl");
  assertOk(result);
  const diagRule = result.program.rules.find((rule) => rule.head === "diag");
  assert.ok(diagRule);
  const headTerms = diagRule.headTerms;
  assert.equal(headTerms.length, 9);
  assert.deepEqual(headTerms[3], { kind: "hvar", name: "line" }); // end_line
  assert.deepEqual(headTerms[4], { kind: "hvar", name: "col" }); // end_col
  const hintTerm = headTerms[8];
  assert.equal(hintTerm!.kind, "hvar");
  const hintVarName = hintTerm!.kind === "hvar" ? hintTerm!.name : undefined;
  assert.equal(hintVarName, "hint");
  const hintBindingAtom = diagRule.body.find(
    (pred) => pred.kind === "rel" && pred.args.length === 1 && pred.args[0]!.kind === "var" && pred.args[0]!.name === "hint",
  );
  assert.ok(hintBindingAtom, "expected a body atom binding `hint`");
  assert.equal(hintBindingAtom!.kind, "rel");
  if (hintBindingAtom!.kind === "rel") assert.equal(result.literalSeeds.get(hintBindingAtom!.rel), null);
});

// ─────────────────────────────────────────────────────────────────────────────
// Named args (NamedArgLaw, tasks.d.ts, owner scope change 2026-07-24): kwargs
// resolve to POSITIONAL slots at load. Grammar-ambiguity checks first (Member
// vs ArgTerm both start with ID), then the resolution/mixing-law unit tests.
// ─────────────────────────────────────────────────────────────────────────────

test("grammar: both positional and named atom args parse (Member vs ArgTerm ambiguity)", () => {
  const dlText = [
    "rel foo(path: text).",
    "rel out1(path: text).",
    "rel out2(path: text).",
    "out1(path) <- foo(path).",
    "out2(path) <- foo(path: path).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
});

test("grammar: decl column syntax (`rel r(col: text)`) still parses unambiguously alongside Member", () => {
  const result = bridge("rel r(col: text).\n", builtinRelsForTests());
  assertOk(result);
});

test("named args resolve to positional slots (reversed order, partial)", () => {
  const dlText = [
    "rel console_hit(path: text, start: int, end: int, text: text).",
    "rel out(p: text).",
    "out(p) <- console_hit(text: t, path: p).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const consoleHitRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "console_hit");
  assert.ok(consoleHitRef);
  assert.equal(consoleHitRef!.kind, "rel");
  if (consoleHitRef!.kind === "rel") {
    assert.deepEqual(consoleHitRef!.args, [
      { kind: "var", name: "p" },
      { kind: "wild" },
      { kind: "wild" },
      { kind: "var", name: "t" },
    ]);
  }
});

test("mixing: positional then named is legal", () => {
  const dlText = ["rel out(p: text, f: text, k: text).", "out(p, f, k) <- node(p, f, kind: k).", ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const nodeRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "node");
  assert.ok(nodeRef);
  assert.equal(nodeRef!.kind, "rel");
  if (nodeRef!.kind === "rel") {
    assert.deepEqual(nodeRef!.args, [
      { kind: "var", name: "p" },
      { kind: "var", name: "f" },
      { kind: "wild" },
      { kind: "wild" },
      { kind: "var", name: "k" },
      { kind: "wild" },
    ]);
  }
});

test("mixing: positional after named errors", () => {
  const result = bridge("rel out(k: text).\nout(k) <- node(kind: k, p).\n", builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "named-arg"));
});

test("duplicate name and name+position collision both error", () => {
  const duplicateResult = bridge("rel out(a: text).\nout(a) <- file(path: a, path: b).\n", builtinRelsForTests());
  assert.equal(duplicateResult.kind, "err");
  if (duplicateResult.kind === "err") assert.ok(duplicateResult.diags.some((diag) => diag.code === "named-arg"));

  const collisionResult = bridge("rel out(a: text).\nout(a) <- file(a, path: b).\n", builtinRelsForTests());
  assert.equal(collisionResult.kind, "err");
  if (collisionResult.kind === "err") assert.ok(collisionResult.diags.some((diag) => diag.code === "named-arg"));
});

test("unknown column name errors", () => {
  const result = bridge("rel out(x: text).\nout(x) <- file(nope: x).\n", builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "named-arg"));
});

test("named args in a probe resolve against the host decl", () => {
  // Head references only `p` (bound by a textual Var in the probe's own args) —
  // `pattern` is a minted var (the literal-input rewrite reuses the host's
  // declared column name) that the head can't reach; that reuse is a separate,
  // pre-existing property of the probe rewrite, not something named args change.
  const dlText = [
    "sh sg(pattern: text, path: text, start: int, end: int, text: text) =",
    "  `sg run --pattern '{pattern}' --json $path`.",
    "rel out(p: text).",
    'out(p) <- sg?(pattern: "x", path: p).',
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);

  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const respRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "__resp_sg");
  assert.ok(respRef);
  assert.equal(respRef!.kind, "rel");
  if (respRef!.kind === "rel") {
    assert.deepEqual(respRef!.args, [
      { kind: "var", name: "pattern" },
      { kind: "var", name: "p" },
      { kind: "wild" },
      { kind: "wild" },
      { kind: "wild" },
    ]);
  }

  const reqRule = result.program.rules.find((rule) => rule.head === "__req_sg");
  assert.ok(reqRule);
  assert.deepEqual(reqRule!.headTerms, [
    { kind: "hvar", name: "pattern" },
    { kind: "hvar", name: "p" },
  ]);
  assert.ok(result.minted.includes("__req_sg"));
  assert.ok(result.minted.includes("__resp_sg"));
});

// ─────────────────────────────────────────────────────────────────────────────
// Salt-arg law (M8-alpha, IdentityWitnessLaw, tasks.d.ts): a probe with more args
// than its host's declared columns treats the excess as positional-only witness
// salts, spliced between the input args and the output args. The fixture golden
// (sg-rail.dl) covers the happy 1-salt path end to end; these are the unit facets
// it can't reach on its own.
// ─────────────────────────────────────────────────────────────────────────────

function shHeader(): string {
  return ["sh h(pattern: text, path: text, start: int, end: int, text: text) =", "  `sg run --pattern '{pattern}' --json $path`."].join(
    "\n",
  );
}

test("salt-arg law: 1 extra arg mints salt_0 between the input and output columns", () => {
  const dlText = [
    shHeader(),
    "rel out(pattern: text, path: text, salt0: text, start: int, end: int, text: text).",
    "out(pattern, path, salt0, start, end, text) <- h?(pattern, path, salt0, start, end, text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);

  const reqRel = result.program.rels.find((rel) => rel.name === "__req_h");
  assert.ok(reqRel);
  assert.deepEqual(reqRel!.columns, ["pattern", "path", "salt_0"]);

  const respRel = result.program.rels.find((rel) => rel.name === "__resp_h");
  assert.ok(respRel);
  assert.deepEqual(respRel!.columns, ["pattern", "path", "salt_0", "start", "end", "text"]);

  const outRule = result.program.rules.find((rule) => rule.head === "out");
  assert.ok(outRule);
  const respRef = outRule.body.find((pred) => pred.kind === "rel" && pred.rel === "__resp_h");
  assert.ok(respRef);
  assert.equal(respRef!.kind, "rel");
  if (respRef!.kind === "rel") {
    assert.deepEqual(respRef!.args, [
      { kind: "var", name: "pattern" },
      { kind: "var", name: "path" },
      { kind: "var", name: "salt0" },
      { kind: "var", name: "start" },
      { kind: "var", name: "end" },
      { kind: "var", name: "text" },
    ]);
  }

  const reqRule = result.program.rules.find((rule) => rule.head === "__req_h");
  assert.ok(reqRule);
  assert.deepEqual(reqRule!.headTerms, [
    { kind: "hvar", name: "pattern" },
    { kind: "hvar", name: "path" },
    { kind: "hvar", name: "salt0" },
  ]);
});

test("salt-arg law: 2 extra args mint salt_0, salt_1 in arg order", () => {
  const dlText = [
    shHeader(),
    "rel out(pattern: text, path: text, salt0: text, salt1: text, start: int, end: int, text: text).",
    "out(pattern, path, salt0, salt1, start, end, text) <- h?(pattern, path, salt0, salt1, start, end, text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);

  const reqRel = result.program.rels.find((rel) => rel.name === "__req_h");
  assert.ok(reqRel);
  assert.deepEqual(reqRel!.columns, ["pattern", "path", "salt_0", "salt_1"]);

  const respRel = result.program.rels.find((rel) => rel.name === "__resp_h");
  assert.ok(respRel);
  assert.deepEqual(respRel!.columns, ["pattern", "path", "salt_0", "salt_1", "start", "end", "text"]);
});

test("salt-arg law: a wildcard salt is a LoadDiag (a salt must bind a Var or Lit witness)", () => {
  const dlText = [
    shHeader(),
    "rel out(pattern: text, path: text, start: int, end: int, text: text).",
    "out(pattern, path, start, end, text) <- h?(pattern, path, _, start, end, text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") {
    assert.ok(result.diags.some((diag) => diag.code === "named-arg" && diag.message.includes("salt")));
  }
});

test("salt-arg law: a named arg targeting nothing still hits the existing named-arg diag", () => {
  const dlText = [
    shHeader(),
    "rel out(pattern: text, path: text, salt0: text, start: int, end: int, text: text).",
    "out(pattern, path, salt0, start, end, text) <- h?(pattern: pattern, path: path, bogus: salt0, start: start, end: end, text: text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") {
    assert.ok(result.diags.some((diag) => diag.code === "named-arg" && diag.message.includes("not a declared column")));
  }
});

test("salt-arg law: zero-salt probes are unchanged (regression, pre-M8 shapes)", () => {
  const dlText = [
    shHeader(),
    "rel out(pattern: text, path: text, start: int, end: int, text: text).",
    "out(pattern, path, start, end, text) <- h?(pattern, path, start, end, text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);

  const reqRel = result.program.rels.find((rel) => rel.name === "__req_h");
  assert.ok(reqRel);
  assert.deepEqual(reqRel!.columns, ["pattern", "path"]);

  const respRel = result.program.rels.find((rel) => rel.name === "__resp_h");
  assert.ok(respRel);
  assert.deepEqual(respRel!.columns, ["pattern", "path", "start", "end", "text"]);
});

test("named head args on a non-diag head: an unfilled slot is a binding LoadDiag", () => {
  const dlText = [
    "rel console_hit(path: text, start: int, end: int, text: text).",
    "rel out(path: text, start: int, end: int, text: text).",
    "out(path: path) <- console_hit(path, start, end, text).",
    "",
  ].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "arity-mismatch"));
});

test("named args in a query atom", () => {
  const dlText = ["rel console_hit(path: text, start: int, end: int, text: text).", "? console_hit(path: p).", ""].join("\n");
  const result = bridge(dlText, builtinRelsForTests());
  assertOk(result);
  assert.equal(result.queries.length, 1);
  const query = result.queries[0]!;
  assert.equal(query.rel, "console_hit");
  assert.deepEqual(query.args, [
    { kind: "var", name: "p" },
    { kind: "wild" },
    { kind: "wild" },
    { kind: "wild" },
  ]);
});

test("named HEAD arg whose value is an AggCall: hit_count(hits: count(path)) <- console_hit(path)", () => {
  const result = bridgeFixture("sg-rail.dl");
  assertOk(result);
  const hitCountRule = result.program.rules.find((rule) => rule.head === "hit_count");
  assert.ok(hitCountRule);
  assert.deepEqual(hitCountRule.headTerms, [{ kind: "hagg", fn: "count", arg: { kind: "var", name: "path" } }]);
  const consoleHitRef = hitCountRule.body.find((pred) => pred.kind === "rel" && pred.rel === "console_hit");
  assert.ok(consoleHitRef);
  assert.equal(consoleHitRef!.kind, "rel");
  if (consoleHitRef!.kind === "rel") {
    assert.deepEqual(consoleHitRef!.args, [
      { kind: "var", name: "path" },
      { kind: "wild" },
      { kind: "wild" },
      { kind: "wild" },
    ]);
  }
  assert.equal(result.retention.get("hit_count"), 1);
});

test("comments: `#` line comments (own-line and trailing) parse identically to the uncommented text", () => {
  const withComments = [
    "# a comment on its own line",
    "rel out(x: text). # trailing comment",
    "rel thing(x: text).",
    "out(x) <- thing(x). # another trailing comment",
    "",
  ].join("\n");
  const withoutComments = ["rel out(x: text).", "rel thing(x: text).", "out(x) <- thing(x).", ""].join("\n");
  const resultWithComments = bridge(withComments, builtinRelsForTests());
  const resultWithoutComments = bridge(withoutComments, builtinRelsForTests());
  assertOk(resultWithComments);
  assertOk(resultWithoutComments);
  assert.deepEqual(stableSerialize(resultWithComments), stableSerialize(resultWithoutComments));
});

// M9 columnType flow (EngineStorageLaw, tasks.d.ts): BridgeOk.columnTypes resolves one
// affinity per column for EVERY rel — declared (user `col:text|int`), base (spine/diag),
// host (__resp/__req cols + text salt), and __lit (typeof value). The storage plane reads
// this to declare affinity + decide which columns intern; a golden can't reach it.
test("columnTypes: sg-rail rels resolve to correct per-column affinity", () => {
  const result = bridgeFixture("sg-rail.dl");
  assertOk(result);
  const types = result.columnTypes;
  // Declared user rels win (peer ruling: declared, not inferred).
  assert.deepEqual(types.get("console_hit"), ["text", "int", "int", "text"]);
  assert.deepEqual(types.get("hit_count"), ["int"]);
  // Builtin spine/diag rels resolve from the base map (they arrive type-less).
  assert.deepEqual(types.get("file"), ["text", "text"]);
  assert.deepEqual(types.get("node"), ["text", "text", "int", "int", "text", "text"]);
  assert.deepEqual(types.get("span_line"), ["text", "int", "int", "int"]);
  assert.deepEqual(types.get("diag"), ["text", "int", "int", "int", "int", "text", "text", "text", "text"]);
  // Host response rel: pattern/path text, salt_0 (content witness) text, start/end int, text text.
  assert.deepEqual(types.get("__resp_sg"), ["text", "text", "text", "int", "int", "text"]);
  assert.deepEqual(types.get("__req_sg"), ["text", "text", "text"]);
  // __lit rels: every seed here is a string -> text.
  assert.deepEqual(types.get("__lit_0"), ["text"]);
  assert.deepEqual(types.get("__lit_1"), ["text"]);
});
