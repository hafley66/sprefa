import { createClient } from '@libsql/client';
const c = createClient({ url: 'file:./probe_dbstat.db' });
await c.execute(`CREATE TABLE t(a TEXT)`);
try {
  const r = await c.execute(`SELECT name, pgsize FROM dbstat WHERE name='t' LIMIT 3`);
  console.log('libsql dbstat OK, rows=', JSON.stringify(r.rows));
} catch (e) { console.log('libsql dbstat FAIL:', String(e.message).split('\n')[0]); }
await c.close();
