import { createClient } from '@libsql/client';
import { execSync } from 'node:child_process';
import { rmSync } from 'node:fs';

const lib = createClient({ url: 'file:./probe_ret2.db' });
await lib.execute(`CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`);

// NEW value appearing twice within one statement
let r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('["z","z"]') i
  RETURNING "content"`);
console.log('libsql new-twice -> rows:', JSON.stringify(r.rows), 'count', r.rows.length);

// A new value + an existing value interleaved
r = await lib.execute(`INSERT OR IGNORE INTO "__str" ("content")
  SELECT i.value FROM json_each('["z","a","y","a","y"]') i
  RETURNING "content"`);
console.log('libsql interleaved -> rows:', JSON.stringify(r.rows), 'count', r.rows.length);
await lib.close();

// CLI via file to avoid quoting
rmSync('./probe_ret2_cli.db', { force: true });
rmSync('./probe_ret2.sql', { force: true });
const wrap = (label, inner) => `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE);\n.headers on\n.mode list\n${inner}\n`;
const sqlA = wrap('A', `INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[\\"z\\",\\"z\\"]') i RETURNING "content";`);
rmSync('./probe_ret2_cli.db', { force: true });
await import('node:fs').then(m=>m.writeFileSync('./probe_ret2.sql', sqlA));
let out = execSync(`sqlite3 ./probe_ret2_cli.db < ./probe_ret2.sql`, { encoding: 'utf8' });
console.log('\nCLI new-twice:\n' + out);

const sqlB = wrap('B', `INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[\\"z\\",\\"a\\",\\"y\\",\\"a\\",\\"y\\"]') i RETURNING "content";`);
await import('node:fs').then(m=>m.writeFileSync('./probe_ret2.sql', sqlB));
out = execSync(`sqlite3 ./probe_ret2_cli.db < ./probe_ret2.sql`, { encoding: 'utf8' });
console.log('CLI interleaved:\n' + out);
