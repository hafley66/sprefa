import { createClient } from '@libsql/client';
import { execSync } from 'node:child_process';
import { rmSync } from 'node:fs';

const lib = createClient({ url: 'file:./probe_ret.db' });

// --- libsql build (3.45.1) ---
await lib.execute(`CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`);

// Case A: fresh intern, distinct values
let r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('["a","b","c"]') i
  RETURNING "content"`);
console.log('libsql A distinct -> rows:', r.rows, 'count', r.rows.length, 'rowsAffected', r.rowsAffected);

// Case B: duplicate keys within one statement
r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('["b","b","c","d"]') i
  RETURNING length("content") as L`);
console.log('libsql B dup-in-stmt -> rows:', r.rows, 'count', r.rows.length);

// Case C: all duplicates (nothing inserted)
r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('["a","b"]') i
  RETURNING length("content") as L`);
console.log('libsql C all-dup -> rows:', JSON.stringify(r.rows), 'count', r.rows.length, 'rowsAffected', r.rowsAffected);

// Case D: empty input
r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('[]') i
  RETURNING length("content") as L`);
console.log('libsql D empty -> rows:', JSON.stringify(r.rows), 'count', r.rows.length);

await lib.close();

// --- CLI sqlite build (3.43.2) ---
rmSync('./probe_ret_cli.db', { force: true });
function cli(sql) {
  return execSync(`sqlite3 ./probe_ret_cli.db "${sql}"`, { encoding: 'utf8' }).trim();
}
cli(`CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`);
console.log('\nCLI A distinct:');
console.log(cli(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('["a","b","c"]') i RETURNING "content";`));
console.log('CLI B dup-in-stmt:');
console.log(cli(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('["b","b","c","d"]') i RETURNING length("content");`));
console.log('CLI C all-dup:');
console.log(cli(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('["a","b"]') i RETURNING length("content");`));
console.log('CLI D empty:');
console.log(cli(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[]') i RETURNING length("content");`));
