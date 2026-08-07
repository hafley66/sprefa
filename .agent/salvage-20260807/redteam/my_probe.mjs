import { createClient } from "@libsql/client";
const db = createClient({ url: "file:my_probe_fresh.db" });
await db.execute("CREATE TABLE s (id INTEGER PRIMARY KEY, content TEXT NOT NULL UNIQUE)");
await db.execute("INSERT INTO s (content) VALUES ('a')");
const plain = await db.execute("INSERT OR IGNORE INTO s (content) VALUES ('a'), ('b'), ('c')");
console.log("plain OR IGNORE, 2 new:", "rowsAffected=", plain.rowsAffected);
const ret = await db.execute("INSERT OR IGNORE INTO s (content) VALUES ('c'), ('d'), ('e') RETURNING content, length(content) AS n");
console.log("OR IGNORE + RETURNING, 2 new:", "rowsAffected=", ret.rowsAffected, "rows=", JSON.stringify(ret.rows));
