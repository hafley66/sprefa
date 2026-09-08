import { createHash } from "node:crypto";

export const crossoverSeed = "0x706f737467726573";

export const crossoverSummaryQuery = `
  SELECT dimension.group_id,
         count(*) AS row_count,
         sum(fact.amount::bigint * dimension.factor::bigint) AS weighted_sum
    FROM fact
    JOIN dimension USING (group_id)
   GROUP BY dimension.group_id
`;

export const crossoverStates = [
  "initial",
  "insert_batch",
  "delete_batch",
  "update_batch",
  "dimension_fanout",
];

export function crossoverGroupsFor(rowCount) {
  return Math.max(8, Math.min(128, Math.ceil(Math.sqrt(rowCount))));
}

export function validateCrossoverCase(rowCount, batchSize, fanout) {
  for (const [name, value] of [["rows", rowCount], ["batch", batchSize], ["fanout", fanout]]) {
    if (!Number.isSafeInteger(value) || value < 1) throw new Error(`${name} must be a positive integer: ${value}`);
  }
  if (fanout + batchSize * 2 > rowCount) {
    throw new Error(`rows must cover fanout plus two mutation batches: ${rowCount} < ${fanout + batchSize * 2}`);
  }
}

function amountFor(id) {
  return (id * 37) % 1000 - 500;
}

function insertedAmountFor(id) {
  return (id * 53) % 1200 - 600;
}

function initialGroupFor(id, fanout, groupCount) {
  return id <= fanout ? 0 : 1 + ((id - fanout - 1) % (groupCount - 1));
}

function checksumSummary(summary) {
  const rows = [...summary]
    .filter(([, value]) => value.count > 0n)
    .sort(([left], [right]) => left - right)
    .map(([groupId, value]) => [String(groupId), String(value.count), String(value.sum)]);
  const canonical = rows.map((row) => `S\t${row.join("\t")}`).join("\n");
  return {
    checksum: createHash("sha256").update(canonical).digest("hex"),
    output_rows: rows.length,
    output_bytes: Buffer.byteLength(canonical),
  };
}

export function makeCrossoverOracle(rowCount, batchSize, fanout) {
  validateCrossoverCase(rowCount, batchSize, fanout);
  const groupCount = crossoverGroupsFor(rowCount);
  const dimensions = new Map();
  const facts = new Map();
  for (let groupId = 0; groupId < groupCount; groupId += 1) dimensions.set(groupId, groupId % 7 + 1);
  for (let id = 1; id <= rowCount; id += 1) {
    facts.set(id, { groupId: initialGroupFor(id, fanout, groupCount), amount: amountFor(id) });
  }

  const apply = {
    initial() {},
    insert_batch() {
      for (let offset = 1; offset <= batchSize; offset += 1) {
        const id = rowCount + offset;
        facts.set(id, { groupId: 1 + ((offset - 1) % (groupCount - 1)), amount: insertedAmountFor(id) });
      }
    },
    delete_batch() {
      for (let id = fanout + 1; id <= fanout + batchSize; id += 1) facts.delete(id);
    },
    update_batch() {
      for (let id = fanout + batchSize + 1; id <= fanout + batchSize * 2; id += 1) {
        const fact = facts.get(id);
        facts.set(id, {
          groupId: 1 + (fact.groupId % (groupCount - 1)),
          amount: fact.amount + 17,
        });
      }
    },
    dimension_fanout() {
      dimensions.set(0, dimensions.get(0) + 3);
    },
  };

  function snapshot() {
    const summary = new Map();
    for (const { groupId, amount } of facts.values()) {
      const value = summary.get(groupId) ?? { count: 0n, sum: 0n };
      value.count += 1n;
      value.sum += BigInt(amount) * BigInt(dimensions.get(groupId));
      summary.set(groupId, value);
    }
    return checksumSummary(summary);
  }

  function inputRows() {
    return {
      dimension: [...dimensions].sort(([a], [b]) => a - b),
      fact: [...facts].sort(([a], [b]) => a - b)
        .map(([id, { groupId, amount }]) => [id, groupId, amount]),
    };
  }

  return { apply, snapshot, groupCount, inputRows };
}

export function crossoverMutationSql(rowCount, batchSize, fanout, groupCount) {
  return {
    initial: "SELECT 1",
    insert_batch: `
      INSERT INTO fact(id, group_id, amount)
      SELECT id,
             1 + ((id - ${rowCount + 1}) % ${groupCount - 1}),
             (id * 53) % 1200 - 600
        FROM generate_series(${rowCount + 1}, ${rowCount + batchSize}) AS id
    `,
    delete_batch: `DELETE FROM fact WHERE id BETWEEN ${fanout + 1} AND ${fanout + batchSize}`,
    update_batch: `
      UPDATE fact
         SET group_id = 1 + (group_id % ${groupCount - 1}), amount = amount + 17
       WHERE id BETWEEN ${fanout + batchSize + 1} AND ${fanout + batchSize * 2}
    `,
    dimension_fanout: "UPDATE dimension SET factor = factor + 3 WHERE group_id = 0",
  };
}

export function expectedAffectedRows(state, batchSize) {
  if (state === "initial") return 0;
  if (state === "dimension_fanout") return 1;
  return batchSize;
}
