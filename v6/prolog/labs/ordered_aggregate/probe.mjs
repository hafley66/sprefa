import { createClient } from "@libsql/client";

const client = createClient({ url: ":memory:" });

async function execute(sql, args = []) {
  return client.execute({ sql, args });
}

async function rows(sql, args = []) {
  return (await execute(sql, args)).rows;
}

async function orderedAggregateProbe() {
  await execute("CREATE TABLE store (store_name TEXT, ordinal INTEGER, item_name TEXT, payload TEXT)");
  await execute("CREATE INDEX store_by_store_name ON store(store_name)");
  await execute("INSERT INTO store VALUES ('north', 1, 'pear', '{\"z\":1,\"a\":2}')");
  await execute("INSERT INTO store VALUES ('north', 3, 'apple', '{\"a\":2,\"z\":1}')");
  await execute("INSERT INTO store VALUES ('north', 2, 'orange', '{\"z\":1,\"a\":2}')");
  await execute("INSERT INTO store VALUES ('south', 1, 'banana', '{\"b\":1}')");
  const valueRows = await rows("SELECT store_name, json_group_array(item_name ORDER BY item_name) AS items FROM store GROUP BY store_name ORDER BY store_name");
  const ordinalRows = await rows("SELECT store_name, json_group_array(item_name ORDER BY ordinal) AS items FROM store GROUP BY store_name ORDER BY store_name");
  const stringRows = await rows("SELECT store_name, group_concat(item_name, ' > ' ORDER BY ordinal) AS items FROM store GROUP BY store_name ORDER BY store_name");
  const nestedRows = await rows("SELECT json_group_array(json(payload) ORDER BY ordinal) AS payloads FROM store WHERE store_name = 'north'");
  const valueText = valueRows[0].items;
  const ordinalText = ordinalRows[0].items;
  if (valueText !== '["apple","orange","pear"]') throw new Error(`value axis mismatch: ${valueText}`);
  if (ordinalText !== '["pear","orange","apple"]') throw new Error(`ordinal axis mismatch: ${ordinalText}`);
  return { valueRows, ordinalRows, stringRows, nestedRows };
}

async function incrementalShapeProbe() {
  await execute("CREATE TABLE star_row (store_name TEXT, ordinal INTEGER, item_name TEXT)");
  await execute("CREATE INDEX star_row_by_store_name ON star_row(store_name)");
  await execute("CREATE TABLE aggregate_head (store_name TEXT PRIMARY KEY, items TEXT)");
  await execute("CREATE TABLE __delta_star_row (store_name TEXT, ordinal INTEGER, item_name TEXT, _sign INTEGER)");
  await execute("CREATE TABLE __agg_scope (store_name TEXT PRIMARY KEY)");
  for (const groupCount of [10, 1000]) {
    await execute("DELETE FROM star_row");
    await execute("DELETE FROM __delta_star_row");
    await execute("DELETE FROM __agg_scope");
    for (let groupIndex = 0; groupIndex < groupCount; groupIndex += 1) {
      await execute("INSERT INTO star_row VALUES (?, ?, ?)", [`store_${groupIndex}`, 1, `item_${groupIndex}`]);
      await execute("INSERT INTO __delta_star_row VALUES (?, ?, ?, 1)", [`store_${groupIndex}`, 1, `item_${groupIndex}`]);
    }
    await execute("INSERT OR IGNORE INTO __agg_scope(store_name) SELECT DISTINCT store_name FROM __delta_star_row WHERE _sign IN (-1, 1)");
    const explain = await rows("EXPLAIN QUERY PLAN SELECT b0.store_name, json_group_array(b0.item_name ORDER BY b0.ordinal) FROM star_row b0 WHERE b0.store_name IN (SELECT store_name FROM __agg_scope) GROUP BY b0.store_name");
    const statementShape = [
      "DELETE FROM aggregate_head WHERE store_name IN (SELECT store_name FROM __agg_scope)",
      "INSERT OR IGNORE INTO aggregate_head SELECT b0.store_name, json_group_array(b0.item_name ORDER BY b0.ordinal) FROM star_row b0 WHERE b0.store_name IN (SELECT store_name FROM __agg_scope) GROUP BY b0.store_name",
    ];
    console.log(JSON.stringify({ groupCount, explain, statementCount: statementShape.length, statementShape }));
  }
}

async function main() {
  console.log(JSON.stringify(await orderedAggregateProbe()));
  await incrementalShapeProbe();
}

await main();
