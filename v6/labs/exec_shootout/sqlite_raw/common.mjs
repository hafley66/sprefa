import { readFileSync } from "node:fs";
import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";

export { Database };

const FNV_OFFSET_HI = 0xcbf29ce4;
const FNV_OFFSET_LO = 0x84222325;
const FNV_PRIME_LOW = 0x1b3;
const TWO_POW_32 = 4294967296;

let stepHi = 0;
let stepLo = 0;

// hash*0x100000001b3 mod 2^64 split as (hash<<40) + hash*0x1b3, kept in two
// 32-bit halves so every intermediate stays under 2^53.
function multiplyByPrime(highHalf, lowHalf) {
  const shiftedHigh = (lowHalf & 0xffffff) * 256;
  const lowProduct = lowHalf * FNV_PRIME_LOW;
  const carry = Math.floor(lowProduct / TWO_POW_32);
  stepLo = lowProduct % TWO_POW_32;
  stepHi = (highHalf * FNV_PRIME_LOW + carry + shiftedHigh) % TWO_POW_32;
}

function absorbByte(highHalf, lowHalf, byte) {
  multiplyByPrime(highHalf, (lowHalf ^ byte) >>> 0);
}

export function hashSourcePrefix(source) {
  absorbByte(FNV_OFFSET_HI, FNV_OFFSET_LO, source & 0xff);
  absorbByte(stepHi, stepLo, (source >>> 8) & 0xff);
  absorbByte(stepHi, stepLo, (source >>> 16) & 0xff);
  absorbByte(stepHi, stepLo, (source >>> 24) & 0xff);
  return [stepHi, stepLo];
}

export function hashPair(prefixHi, prefixLo, target) {
  absorbByte(prefixHi, prefixLo, target & 0xff);
  absorbByte(stepHi, stepLo, (target >>> 8) & 0xff);
  absorbByte(stepHi, stepLo, (target >>> 16) & 0xff);
  absorbByte(stepHi, stepLo, (target >>> 24) & 0xff);
  return [stepHi, stepLo];
}

export function formatChecksum(highHalf, lowHalf) {
  return (
    (highHalf >>> 0).toString(16).padStart(8, "0") +
    (lowHalf >>> 0).toString(16).padStart(8, "0")
  );
}

export function readEdges(path) {
  const text = readFileSync(path, "utf8");
  const lines = text.split("\n");
  const edges = [];
  let nodeCount = 0;
  for (const line of lines) {
    if (line.length === 0) continue;
    if (line[0] === "p") {
      const header = line.split(/\s+/);
      nodeCount = Number(header[1]);
      continue;
    }
    const space = line.indexOf(" ");
    edges.push([Number(line.slice(0, space)), Number(line.slice(space + 1))]);
  }
  return { nodeCount, edges };
}

// An in-memory database has no journal and no fsync, so only page_size and
// temp_store moved a measured number; the rest are kept out of the entrant.
export const PRAGMA_SETS = {
  chosen: `
    PRAGMA page_size=16384;
    PRAGMA temp_store=MEMORY;
  `,
  tuned: `
    PRAGMA journal_mode=OFF;
    PRAGMA synchronous=OFF;
    PRAGMA temp_store=MEMORY;
    PRAGMA cache_size=-1048576;
    PRAGMA locking_mode=EXCLUSIVE;
  `,
  wal_normal: `
    PRAGMA journal_mode=WAL;
    PRAGMA synchronous=NORMAL;
    PRAGMA temp_store=MEMORY;
  `,
  defaults: ``,
  no_cache_bump: `
    PRAGMA journal_mode=OFF;
    PRAGMA synchronous=OFF;
    PRAGMA temp_store=MEMORY;
  `,
  page_64k: `
    PRAGMA page_size=65536;
    PRAGMA journal_mode=OFF;
    PRAGMA synchronous=OFF;
    PRAGMA temp_store=MEMORY;
    PRAGMA cache_size=-1048576;
  `,
  page_16k: `
    PRAGMA page_size=16384;
    PRAGMA journal_mode=OFF;
    PRAGMA synchronous=OFF;
    PRAGMA temp_store=MEMORY;
    PRAGMA cache_size=-1048576;
  `,
};

export function openDatabase(pragmaSet = "tuned") {
  const db = new Database(":memory:");
  const pragmaText = PRAGMA_SETS[pragmaSet];
  if (pragmaText === undefined) throw new Error(`unknown pragma set ${pragmaSet}`);
  if (pragmaText.trim().length > 0) db.exec(pragmaText);
  db.exec(`
    CREATE TABLE edge (
      source INTEGER NOT NULL,
      target INTEGER NOT NULL,
      PRIMARY KEY (source, target)
    ) WITHOUT ROWID;
  `);
  return db;
}

export function loadEdges(db, edges) {
  const insert = db.prepare(`INSERT OR IGNORE INTO edge VALUES (?, ?)`);
  db.transaction(() => {
    for (const edge of edges) insert.run(edge);
  })();
}

export function foldChecksumStreaming(db, sql) {
  const statement = db.prepare(sql).raw(true);
  let accumulatorHi = 0;
  let accumulatorLo = 0;
  let rowCount = 0;
  let cachedSource = -1;
  let prefixHi = 0;
  let prefixLo = 0;
  for (const row of statement.iterate()) {
    const source = row[0];
    const target = row[1];
    if (source !== cachedSource) {
      const prefix = hashSourcePrefix(source);
      prefixHi = prefix[0];
      prefixLo = prefix[1];
      cachedSource = source;
    }
    const pair = hashPair(prefixHi, prefixLo, target);
    accumulatorHi ^= pair[0];
    accumulatorLo ^= pair[1];
    rowCount += 1;
  }
  return { rowCount, checksum: formatChecksum(accumulatorHi, accumulatorLo) };
}

// One text blob per page instead of one N-API row crossing per row; the
// scanner walks char codes, so no split/Number allocation per pair.
export function foldChecksumConcat(db, table, pageRows = 131072) {
  const statement = db
    .prepare(
      `SELECT group_concat(source || ',' || target, ',') AS packed FROM (
         SELECT source, target FROM ${table}
         WHERE (source, target) > (?, ?)
         ORDER BY source, target LIMIT ?
       )`,
    )
    .raw(true);
  let accumulatorHi = 0;
  let accumulatorLo = 0;
  let rowCount = 0;
  let cursorSource = -1;
  let cursorTarget = -1;
  let cachedSource = -1;
  let prefixHi = 0;
  let prefixLo = 0;
  for (;;) {
    const packed = statement.get(cursorSource, cursorTarget, pageRows)[0];
    if (packed === null) break;
    let pending = 0;
    let holdingSource = true;
    let source = 0;
    const length = packed.length;
    for (let at = 0; at <= length; at++) {
      const code = at === length ? 44 : packed.charCodeAt(at);
      if (code !== 44) {
        pending = pending * 10 + (code - 48);
        continue;
      }
      if (holdingSource) {
        source = pending;
        holdingSource = false;
      } else {
        if (source !== cachedSource) {
          const prefix = hashSourcePrefix(source);
          prefixHi = prefix[0];
          prefixLo = prefix[1];
          cachedSource = source;
        }
        const pair = hashPair(prefixHi, prefixLo, pending);
        accumulatorHi ^= pair[0];
        accumulatorLo ^= pair[1];
        cursorSource = source;
        cursorTarget = pending;
        rowCount += 1;
        holdingSource = true;
      }
      pending = 0;
    }
    if (rowCount % pageRows !== 0) break;
  }
  return { rowCount, checksum: formatChecksum(accumulatorHi, accumulatorLo) };
}

export function foldChecksumPagedRowid(db, table, pageRows = 262144) {
  const statement = db
    .prepare(`SELECT rowid, source, target FROM ${table} WHERE rowid > ? ORDER BY rowid LIMIT ?`)
    .raw(true);
  let accumulatorHi = 0;
  let accumulatorLo = 0;
  let rowCount = 0;
  let cursorRowid = 0;
  for (;;) {
    const page = statement.all(cursorRowid, pageRows);
    for (const row of page) {
      const prefix = hashSourcePrefix(row[1]);
      const pair = hashPair(prefix[0], prefix[1], row[2]);
      accumulatorHi ^= pair[0];
      accumulatorLo ^= pair[1];
      cursorRowid = row[0];
    }
    rowCount += page.length;
    if (page.length < pageRows) break;
  }
  return { rowCount, checksum: formatChecksum(accumulatorHi, accumulatorLo) };
}

export function foldChecksumPaged(db, table, pageRows = 262144) {
  const statement = db
    .prepare(
      `SELECT source, target FROM ${table}
       WHERE (source, target) > (?, ?)
       ORDER BY source, target LIMIT ?`,
    )
    .raw(true);
  let accumulatorHi = 0;
  let accumulatorLo = 0;
  let rowCount = 0;
  let cursorSource = -1;
  let cursorTarget = -1;
  let cachedSource = -1;
  let prefixHi = 0;
  let prefixLo = 0;
  for (;;) {
    const page = statement.all(cursorSource, cursorTarget, pageRows);
    for (const row of page) {
      const source = row[0];
      const target = row[1];
      if (source !== cachedSource) {
        const prefix = hashSourcePrefix(source);
        prefixHi = prefix[0];
        prefixLo = prefix[1];
        cachedSource = source;
      }
      const pair = hashPair(prefixHi, prefixLo, target);
      accumulatorHi ^= pair[0];
      accumulatorLo ^= pair[1];
      cursorSource = source;
      cursorTarget = target;
    }
    rowCount += page.length;
    if (page.length < pageRows) break;
  }
  return { rowCount, checksum: formatChecksum(accumulatorHi, accumulatorLo) };
}
