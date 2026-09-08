-- Load the compiled extension with the CLI .load command before this script.
PRAGMA recursive_triggers=ON;
PRAGMA trusted_schema=ON;
CREATE TABLE fact(id INTEGER PRIMARY KEY,group_id INTEGER,amount INTEGER);
CREATE TABLE dimension(group_id INTEGER PRIMARY KEY,factor INTEGER);
INSERT INTO fact VALUES(1,1,4),(2,1,-1);
INSERT INTO dimension VALUES(1,2);
SELECT sqlite_ivm_create('totals',
 'SELECT f.group_id,COUNT(*) AS row_count,SUM(f.amount*d.factor) AS weighted_sum
  FROM fact f JOIN dimension d ON f.group_id=d.group_id GROUP BY f.group_id');
SELECT * FROM totals;
UPDATE dimension SET factor=-3;
SELECT * FROM totals;
BEGIN;
DELETE FROM fact;
SELECT * FROM totals;
ROLLBACK;
SELECT * FROM totals;
SELECT sqlite_ivm_drop('totals');
