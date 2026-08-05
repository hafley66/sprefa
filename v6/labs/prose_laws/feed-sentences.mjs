#!/usr/bin/env node
// prose-nothing feed: walk ~/.claude/projects/*/*.jsonl, split assistant and
// user text into sentences, emit JSONL {"side","seq","sentence"} rows. In
// fixture mode it reads a JSON array [{side,sentence}] from a file instead.

import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const HOME = os.homedir();
const MODE = process.argv[2];

function injectedScaffold(text) {
  if (!text.trimStart().startsWith("<")) return false;
  return /system-reminder|command-name|local-command|task-notification/.test(text);
}

function stripCode(text) {
  return text
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`[^`]*`/g, " ")
    .replace(/\s+/g, " ");
}

function splitSentences(text) {
  const out = [];
  const parts = text.split(/(?<=[.!?])\s+/);
  for (const part of parts) {
    const clean = part.trim();
    if (clean && clean !== ".") out.push(clean);
  }
  return out;
}

let rows = [];
let seq = 0;

const CHUNK_SIZE = Number(process.env.PROSE_LAWS_CHUNK_SIZE || "5000");
const chunkIndex = MODE === "feed" ? process.argv[3] : undefined;

if (MODE === "fixture") {
  const chunkIndex = process.argv[3];
  if (!chunkIndex || !fs.existsSync(chunkIndex)) {
    process.stderr.write(`feed-sentences.mjs: fixture file missing: ${chunkIndex}\n`);
    process.exit(2);
  }
  const fixture = JSON.parse(fs.readFileSync(chunkIndex, "utf8"));
  for (const item of fixture) {
    const side = item.side;
    for (const sentence of splitSentences(stripCode(item.sentence))) {
      rows.push(JSON.stringify({ side, seq: seq++, sentence }));
    }
  }
} else {
  const projectsRoot = path.join(HOME, ".claude", "projects");
  let files = [];
  try {
    files = fs.readdirSync(projectsRoot).flatMap((dir) => {
      const dirPath = path.join(projectsRoot, dir);
      try {
        return fs.readdirSync(dirPath).filter((f) => f.endsWith(".jsonl")).map((f) => path.join(dirPath, f));
      } catch {
        return [];
      }
    });
  } catch {
    process.stderr.write(`feed-sentences.mjs: cannot read ${projectsRoot}\n`);
    process.exit(2);
  }

  for (const file of files) {
    let handle;
    try {
      handle = fs.openSync(file, "r");
    } catch {
      continue;
    }
    const stream = fs.createReadStream(file, { fd: handle });
    let buffer = "";
    await new Promise((resolve, reject) => {
      stream.on("data", (chunk) => {
        buffer += chunk.toString("utf8");
        let nl;
        while ((nl = buffer.indexOf("\n")) !== -1) {
          const line = buffer.slice(0, nl).trim();
          buffer = buffer.slice(nl + 1);
          if (!line) continue;
          let record;
          try {
            record = JSON.parse(line);
          } catch {
            continue;
          }
          const type = record.type;
          const content = (record.message ?? {}).content;
          let blocks = [];
          if (type === "assistant") {
            if (Array.isArray(content)) {
              blocks = content.filter((item) => item?.type === "text").map((item) => item.text).filter(Boolean);
            }
          } else if (type === "user") {
            if (typeof content === "string") {
              blocks = [content];
            } else if (Array.isArray(content)) {
              blocks = content.filter((item) => item?.type === "text").map((item) => item.text).filter(Boolean);
            }
          }
          for (const block of blocks) {
            if (injectedScaffold(block)) continue;
            const side = type === "assistant" ? "assistant" : "user";
            for (const sentence of splitSentences(stripCode(block))) {
              rows.push(JSON.stringify({ side, seq: seq++, sentence }));
            }
          }
        }
      });
      stream.on("end", resolve);
      stream.on("error", reject);
    });
  }
}

if (MODE === "feed" && chunkIndex !== undefined && chunkIndex !== "all") {
  const index = Number(chunkIndex);
  const start = index * CHUNK_SIZE;
  rows = rows.slice(start, start + CHUNK_SIZE);
}

process.stdout.write(rows.join("\n") + (rows.length ? "\n" : ""));
