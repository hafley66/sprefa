// Create a tiny demo SQLite DB so the `sql-graph` fence has something to query.
// This is a stand-in for dl's real kernel: the running-example call graph as a
// `call_edge(caller, callee)` table. Run once: `npm run seed`.
import { createRequire } from 'node:module'
import { mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const { DatabaseSync } = createRequire(import.meta.url)('node:sqlite')
const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(path.join(root, 'data'), { recursive: true })

const db = new DatabaseSync(path.join(root, 'data/callgraph.sqlite'))
db.exec('DROP TABLE IF EXISTS call_edge; CREATE TABLE call_edge(caller TEXT, callee TEXT)')
const ins = db.prepare('INSERT INTO call_edge VALUES (?, ?)')
for (const [a, b] of [['main', 'run'], ['run', 'parse'], ['parse', 'lex'], ['lex', 'run'], ['run', 'log']]) ins.run(a, b)
db.exec('DROP TABLE IF EXISTS fn; CREATE TABLE fn(name TEXT)')
const insf = db.prepare('INSERT INTO fn VALUES (?)')
for (const n of ['main', 'run', 'parse', 'lex', 'log', 'helper']) insf.run(n)
db.close()
console.log('seeded data/callgraph.sqlite (call_edge, fn)')
