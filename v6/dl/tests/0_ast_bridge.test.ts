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

test("arity: more args than declared arity is a load error", () => {
  const result = bridge("rel out(path: text).\nout(path) <- file(path, extra).\n", builtinRelsForTests());
  assert.equal(result.kind, "err");
  if (result.kind === "err") assert.ok(result.diags.some((diag) => diag.code === "arity-mismatch"));
});

test("arity: fewer args than declared arity elides (the ref keeps fewer args, not an error)", () => {
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
  if (consoleHitRef.kind === "rel") assert.equal(consoleHitRef.args.length, 1);
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
