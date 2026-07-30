/**
 * 2_parity.mjs — the HTTP parity gate: the emitted spec's route inventory is
 * the server's actual route inventory, checked four ways.
 *
 * Run:
 *   node --test v6/prolog/labs/openapi_codegen/2_parity.mjs
 *   OPENAPI_LAB_SPEC=/tmp/lying.json node --test .../2_parity.mjs   (sabotage)
 *   OPENAPI_LAB_NO_LIVE=1 node --test .../2_parity.mjs              (no server boot)
 *
 * THE PATTERN THIS LIFTS: tests/bopCommandInventory.test.ts asserts that
 * registry.pl's cli_command/3 rows and bop.ts's `.command("...")` lines name
 * the same verb set, by reading each side in its own native form rather than
 * inventing a shared file both sides must remember to update. Same idea here,
 * with two more sources available because HTTP routes, unlike CLI verbs, can
 * be interrogated from a running process.
 *
 *   SPEC       the emitted openapi.json's paths x methods.
 *   ROUTE_LIST serve/4_http.ts's exported literal (what the 404 body claims).
 *   DISPATCH   the routing CONDITIONS parsed out of 4_http.ts. This is the
 *              only one of the four that cannot lie by omission: a route that
 *              is served but listed nowhere still has a dispatch branch.
 *   LIVE       a real `serve/main.ts` process, asked for a path it does not
 *              have; the 404 body carries `routes`. Catches the case where
 *              the source parse is right and the built/running thing differs.
 *
 * NORMAL FORM: `METHOD /seg/seg`, with every path PARAMETER collapsed to `*`.
 * The dispatch conditions compare `segments.length` and `segments[0]` and
 * never name the parameter (`segments[1]!` is passed positionally), so a
 * parameter's NAME is not something the server source proves. Comparing on
 * `*` keeps the gate honest about what each source actually knows; the name
 * is pinned by the spec and by the generated cli/0_inventory.ts instead.
 */

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const LAB_DIR = fileURLToPath(new URL(".", import.meta.url));
const REPO_ROOT = fileURLToPath(new URL("../../../../", import.meta.url));
const HTTP_TS = `${REPO_ROOT}v6/tsv2/serve/4_http.ts`;
const SERVE_MAIN = `${REPO_ROOT}v6/tsv2/serve/main.ts`;
const SPEC_PATH = process.env.OPENAPI_LAB_SPEC ?? `${LAB_DIR}openapi.json`;

/** `/idb/{rel}` and `/idb/:rel` both normalize to `/idb/*`. */
function normalize(method, path) {
  const segments = path
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => (segment.startsWith("{") || segment.startsWith(":") ? "*" : segment));
  return `${method.toUpperCase()} /${segments.join("/")}`;
}

function sorted(values) {
  return [...new Set(values)].sort();
}

// ── the four sources ─────────────────────────────────────────────────────────

function fromSpec() {
  const spec = JSON.parse(readFileSync(SPEC_PATH, "utf8"));
  const routes = [];
  for (const [path, item] of Object.entries(spec.paths ?? {})) {
    for (const method of Object.keys(item)) routes.push(normalize(method, path));
  }
  return sorted(routes);
}

function fromRouteList() {
  const source = readFileSync(HTTP_TS, "utf8");
  const block = /export const ROUTE_LIST[^=]*=\s*\[([^\]]*)\]/.exec(source);
  assert.ok(block, "4_http.ts no longer exports a ROUTE_LIST array literal");
  return sorted(
    [...block[1].matchAll(/"([A-Z]+) ([^"]+)"/g)].map((match) => normalize(match[1], match[2])),
  );
}

/**
 * The dispatch conditions themselves. Every branch in this server is spelled
 * `method === "M" && segments.length === N && segments[0] === "first"`
 * (isProgramLoad writes `request.method`, which the regex still matches).
 * A branch with a different shape would go UNSEEN here, so the count is
 * asserted too: a new route added in a new spelling fails the count check
 * rather than silently passing the set check.
 */
function fromDispatch() {
  const source = readFileSync(HTTP_TS, "utf8");
  const matches = [
    ...source.matchAll(
      /method === "([A-Z]+)" && segments\.length === (\d+) && segments\[0\] === "([a-z_]+)"/g,
    ),
  ];
  return sorted(
    matches.map(([, method, count, first]) => {
      const tail = Array.from({ length: Number(count) - 1 }, () => "*");
      return normalize(method, `/${[first, ...tail].join("/")}`);
    }),
  );
}

/** Boot serve/main.ts on an ephemeral port and ask it for a path it refuses;
 *  the catch-all 404 body carries `routes`. Nothing is loaded, so no program
 *  is needed -- the route table exists before any program does. */
async function fromLiveServer() {
  const child = spawn("node", ["--experimental-transform-types", SERVE_MAIN], {
    env: { ...process.env, TSV2_PORT: "0", TSV2_DB: ":memory:" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  try {
    const port = await new Promise((resolve, reject) => {
      let buffered = "";
      const timer = setTimeout(() => reject(new Error(`server never announced a port: ${buffered}`)), 30_000);
      child.stdout.on("data", (chunk) => {
        buffered += String(chunk);
        const announced = /tsv2 serving on (\d+)/.exec(buffered);
        if (announced) {
          clearTimeout(timer);
          resolve(Number(announced[1]));
        }
      });
      child.stderr.on("data", (chunk) => {
        buffered += String(chunk);
      });
      child.once("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`server exited ${code} before listening: ${buffered}`));
      });
    });
    const response = await fetch(`http://127.0.0.1:${port}/__parity_probe__`);
    assert.equal(response.status, 404);
    const body = await response.json();
    assert.ok(Array.isArray(body.routes), "the 404 body no longer carries a routes array");
    return sorted(body.routes.map((entry) => normalize(...entry.split(" "))));
  } finally {
    child.kill("SIGKILL");
  }
}

// ── the gate ─────────────────────────────────────────────────────────────────

test("emitted spec route inventory == 4_http.ts ROUTE_LIST", () => {
  assert.deepEqual(fromSpec(), fromRouteList());
});

test("emitted spec route inventory == 4_http.ts dispatch branches", () => {
  const dispatch = fromDispatch();
  assert.equal(dispatch.length, 5, `expected 5 dispatch branches, parsed ${dispatch.length}: ${dispatch.join(", ")}`);
  assert.deepEqual(fromSpec(), dispatch);
});

test("emitted spec route inventory == a live server's own 404 route list", async (t) => {
  if (process.env.OPENAPI_LAB_NO_LIVE === "1") {
    t.skip("OPENAPI_LAB_NO_LIVE=1");
    return;
  }
  assert.deepEqual(fromSpec(), await fromLiveServer());
});
