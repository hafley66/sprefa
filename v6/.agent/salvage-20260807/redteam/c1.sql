CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE);
.headers on
.mode list
INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('["z","z"]') i RETURNING "content";
INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('["z","a","y","a","y"]') i RETURNING "content";
INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each('[]') i RETURNING "content";
