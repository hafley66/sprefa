import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { checkGenerated, generate, repository, schemaDirectory } from "./2_gen.mjs";

const files = await generate();

test("generation is deterministic and checked-in artifacts are current", async () => {
  assert.deepEqual(await generate(), files);
  await checkGenerated(files);
});

test("TypeSpec span fields match the existing Rust contracts", async () => {
  const source = await readFile(join(repository, "v6/sprefa-extract/src/types.rs"), "utf8");
  const models = JSON.parse(files.get("3_models.json"));
  const rustScalars = { uint32: "u32" };
  for (const model of models) {
    const declaration = source.match(new RegExp(`pub struct ${model.name} \\{([^}]+)\\}`));
    assert.ok(declaration, `Existing Rust type ${model.name}`);
    const actual = [...declaration[1].matchAll(/pub (\w+): (\w+)/g)].map(([, name, type]) => [name, type]);
    assert.deepEqual(actual, model.fields.map(field => [field.name, rustScalars[field.type.name]]));
  }
});

test("generated SQLite tables accept span fixtures and enforce their row keys", () => {
  const sql = files.get("2_sql_trial.sql");
  const result = execFileSync("sqlite3", ["-batch", "-bail", ":memory:"], {
    encoding: "utf8", timeout: 10_000,
    input: `${sql}
      INSERT INTO span_row VALUES (1, 0, 0), (2, 7, 3), (3, 4294967295, 0);
      INSERT INTO span_out_row VALUES (1, 0, 0), (2, 7, 10), (3, 4294967295, 4294967295);
      SELECT 'local', id, start, len FROM span_row ORDER BY id;
      SELECT 'wire', id, start, end FROM span_out_row ORDER BY id;
      PRAGMA integrity_check;
    `,
  });
  assert.equal(result, "local|1|0|0\nlocal|2|7|3\nlocal|3|4294967295|0\nwire|1|0|0\nwire|2|7|10\nwire|3|4294967295|4294967295\nok\n");
  assert.throws(() => execFileSync("sqlite3", ["-batch", "-bail", ":memory:"], {
    encoding: "utf8", timeout: 10_000, stdio: ["pipe", "pipe", "pipe"],
    input: `${sql}\nINSERT INTO span_row VALUES (1, 0, 0), (1, 7, 3);`,
  }), /UNIQUE constraint failed/);
});

test("malformed or unsupported TypeSpec fails before artifact publication", async () => {
  const scratch = await mkdtemp(join(tmpdir(), "sprefa-schema-errors-"));
  try {
    const entry = join(scratch, "invalid.tsp");
    await writeFile(entry, "namespace Extract; model Broken { value: MissingType; }");
    await assert.rejects(generate(entry), /MissingType/);
    await writeFile(entry, "namespace Extract; model Broken { value: string; }");
    await assert.rejects(generate(entry), /Unsupported span field/);
    await checkGenerated(files);
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
});

test("generated Rust compiles and preserves u32 values", async () => {
  const scratch = await mkdtemp(join(tmpdir(), "sprefa-schema-rust-"));
  try {
    for (const [name, content] of files) if (name.endsWith(".rs")) await writeFile(join(scratch, name), content);
    // Reuse the version already locked by extract. No production dependency change.
    const lock = await readFile(join(repository, "v6/sprefa-extract/Cargo.lock"), "utf8");
    const version = lock.match(/name = "serde"\nversion = "([^"]+)"/)[1];
    await writeFile(join(scratch, "Cargo.toml"), `[package]\nname = "sprefa-schema-trial"\nversion = "0.0.0"\nedition = "2021"\n[lib]\npath = "lib.rs"\n[dependencies]\nserde = { version = "=${version}", features = ["derive"] }\n[workspace]\n`);
    await writeFile(join(scratch, "lib.rs"), `
      #[path = "0_span.rs"] pub mod local;
      #[path = "1_span_out.rs"] pub mod wire;
      #[path = "6_facts.rs"] pub mod facts;
      #[test] fn values() {
        let local = local::Span { start: u32::MAX, len: 0 };
        let wire = wire::SpanOut { start: local.start, end: local.start + local.len };
        assert_eq!((wire.start, wire.end), (u32::MAX, u32::MAX));
      }
    `);
    const result = execFileSync("cargo", ["test", "--offline", "--manifest-path", join(scratch, "Cargo.toml")], {
      cwd: scratch, encoding: "utf8", timeout: 90_000,
      env: { ...process.env, CARGO_TARGET_DIR: join(scratch, "target") },
      stdio: ["ignore", "pipe", "pipe"],
    });
    assert.match(result, /1 passed; 0 failed/);
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
});
