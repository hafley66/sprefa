import { createClient } from '@libsql/client';
const c = createClient({ url: 'file:./probe_empty.db' });
await c.execute(`CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`);
await c.execute(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[""]') i`);
await c.execute(`INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[""]') i`);
let r = await c.execute(`SELECT s."content", s."__id" FROM json_each('[""]') i JOIN "__str" s ON s."content" = i.value`);
console.log('empty-string lookup:', JSON.stringify(r.rows));  // expect one row: '' with single id
r = await c.execute(`SELECT count(*), sum(length(content)) FROM "__str"`);
console.log('dupe empty-string interned once:', JSON.stringify(r.rows));
await c.close();
