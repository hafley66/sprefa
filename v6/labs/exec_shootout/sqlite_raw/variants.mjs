const RESULT_WITHOUT_ROWID = `
  CREATE TABLE reachable (
    source INTEGER NOT NULL,
    target INTEGER NOT NULL,
    PRIMARY KEY (source, target)
  ) WITHOUT ROWID;
`;

const RESULT_ROWID_UNIQUE = `
  CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL);
  CREATE UNIQUE INDEX reachable_pair ON reachable (source, target);
`;

const RESULT_ROWID_BARE = `
  CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL);
`;

const FRONTIER_PAIR_TEMPS = `
  CREATE TEMP TABLE frontier_ping (
    source INTEGER NOT NULL,
    target INTEGER NOT NULL,
    PRIMARY KEY (source, target)
  ) WITHOUT ROWID;
  CREATE TEMP TABLE frontier_pong (
    source INTEGER NOT NULL,
    target INTEGER NOT NULL,
    PRIMARY KEY (source, target)
  ) WITHOUT ROWID;
`;

const FRONTIER_APPEND_TEMPS = `
  CREATE TEMP TABLE frontier_ping (source INTEGER NOT NULL, target INTEGER NOT NULL);
  CREATE TEMP TABLE frontier_pong (source INTEGER NOT NULL, target INTEGER NOT NULL);
`;

const RECURSIVE_CLOSURE = `
  WITH RECURSIVE closure (source, target) AS (
    SELECT source, target FROM edge
    UNION
    SELECT closure.source, edge.target
    FROM closure JOIN edge ON edge.source = closure.target
  )
  SELECT source, target FROM closure
`;

function wavefront(db) {
  const stepSql = (frontier, next) => `
    INSERT OR IGNORE INTO ${next} (source, target)
    SELECT frontier.source, edge.target
    FROM ${frontier} frontier
    JOIN edge ON edge.source = frontier.target
    WHERE NOT EXISTS (SELECT 1 FROM reachable known
      WHERE known.source = frontier.source AND known.target = edge.target)
  `;
  const pingToPong = db.prepare(stepSql("frontier_ping", "frontier_pong"));
  const pongToPing = db.prepare(stepSql("frontier_pong", "frontier_ping"));
  const promotePing = db.prepare(
    `INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM frontier_ping`,
  );
  const promotePong = db.prepare(
    `INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM frontier_pong`,
  );
  const clearPing = db.prepare(`DELETE FROM frontier_ping`);
  const clearPong = db.prepare(`DELETE FROM frontier_pong`);
  let rounds = 0;
  let statements = 0;
  db.transaction(() => {
    db.prepare(
      `INSERT OR IGNORE INTO frontier_ping (source, target) SELECT source, target FROM edge`,
    ).run();
    promotePing.run();
    statements += 2;
    let usePing = true;
    for (;;) {
      const clear = usePing ? clearPong : clearPing;
      const step = usePing ? pingToPong : pongToPing;
      const promote = usePing ? promotePong : promotePing;
      clear.run();
      const derived = step.run().changes;
      statements += 2;
      if (derived === 0) break;
      promote.run();
      statements += 1;
      rounds += 1;
      usePing = !usePing;
    }
  })();
  return { rounds, statements };
}

export const variants = {
  cte_wor: {
    label: "WITH RECURSIVE UNION into WITHOUT ROWID result",
    schema: RESULT_WITHOUT_ROWID,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "pk",
    writesPerRow: 3,
    writesNote: "cte queue append + cte distinct index + result pk btree",
    derive(db) {
      db.exec(`INSERT INTO reachable (source, target) ${RECURSIVE_CLOSURE};`);
      return { rounds: 1, statements: 1 };
    },
  },

  cte_rowid: {
    label: "WITH RECURSIVE UNION into bare rowid result (no index)",
    schema: RESULT_ROWID_BARE,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "rowid",
    writesPerRow: 3,
    writesNote: "cte queue append + cte distinct index + result rowid append",
    derive(db) {
      db.exec(`INSERT INTO reachable (source, target) ${RECURSIVE_CLOSURE};`);
      return { rounds: 1, statements: 1 };
    },
  },

  cte_stream: {
    label: "WITH RECURSIVE folded straight to checksum, nothing materialized",
    schema: ``,
    scanSql: RECURSIVE_CLOSURE,
    pagedKind: null,
    streamOnly: true,
    writesPerRow: 2,
    writesNote: "cte queue append + cte distinct index, no result table",
    derive() {
      return { rounds: 0, statements: 0 };
    },
  },

  loop_notexists_wor: {
    label: "wavefront, deduped TEMP frontiers, NOT EXISTS gate, WITHOUT ROWID result",
    schema: RESULT_WITHOUT_ROWID + FRONTIER_PAIR_TEMPS,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "pk",
    writesPerRow: 2,
    writesNote: "frontier pk btree + result pk btree",
    derive(db) {
      return wavefront(db);
    },
  },

  loop_notexists_rowid: {
    label: "wavefront, deduped TEMP frontiers, NOT EXISTS gate, rowid result + unique index",
    schema: RESULT_ROWID_UNIQUE + FRONTIER_PAIR_TEMPS,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "rowid",
    writesPerRow: 3,
    writesNote: "frontier pk btree + result rowid append + result unique index",
    derive(db) {
      return wavefront(db);
    },
  },

  loop_appendfrontier_wor: {
    label: "wavefront, undeduped append TEMP frontiers, NOT EXISTS gate, WITHOUT ROWID result",
    schema: RESULT_WITHOUT_ROWID + FRONTIER_APPEND_TEMPS,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "pk",
    writesPerRow: 2,
    writesNote: "frontier rowid append + result pk btree, duplicates re-expanded",
    derive(db) {
      return wavefront(db);
    },
  },

  loop_range_rowid: {
    label: "wavefront with no frontier table, rowid range as the delta, OR IGNORE rejection",
    schema: RESULT_ROWID_UNIQUE,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "rowid",
    writesPerRow: 2,
    writesNote: "result rowid append + result unique index",
    derive(db) {
      return rowidRangeWavefront(db, false);
    },
  },

  loop_range_notexists_rowid: {
    label: "rowid range delta plus a NOT EXISTS prefilter against the result",
    schema: RESULT_ROWID_UNIQUE,
    scanSql: `SELECT source, target FROM reachable`,
    pagedKind: "rowid",
    writesPerRow: 2,
    writesNote: "result rowid append + result unique index, duplicates rejected before insert",
    derive(db) {
      return rowidRangeWavefront(db, true);
    },
  },
};

// The SELECT reads the table being written, so SQLite snapshots it into a
// transient table first; the rowid range and the NOT EXISTS both see round-start state.
function rowidRangeWavefront(db, prefilter) {
  const seed = db.prepare(
    `INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM edge`,
  );
  const step = db.prepare(`
    INSERT OR IGNORE INTO reachable (source, target)
    SELECT known.source, edge.target
    FROM reachable known JOIN edge ON edge.source = known.target
    WHERE known.rowid BETWEEN ? AND ?
    ${
      prefilter
        ? `AND NOT EXISTS (SELECT 1 FROM reachable seen
             WHERE seen.source = known.source AND seen.target = edge.target)`
        : ``
    }
  `);
  let rounds = 0;
  let statements = 0;
  db.transaction(() => {
    let low = 1;
    let high = seed.run().changes;
    statements += 1;
    for (;;) {
      const derived = step.run(low, high).changes;
      statements += 1;
      if (derived === 0) break;
      low = high + 1;
      high += derived;
      rounds += 1;
    }
  })();
  const tally = db.prepare(`SELECT max(rowid) AS top, count(*) AS rows FROM reachable`).get();
  if (tally.top !== tally.rows) {
    throw new Error(`rowid range broke: max(rowid)=${tally.top} count=${tally.rows}`);
  }
  return { rounds, statements };
}

export const variantNames = Object.keys(variants);
