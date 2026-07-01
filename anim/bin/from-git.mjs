#!/usr/bin/env node
// Turn a git commit range into animation frames: each commit becomes one frame,
// the file snapshot at that commit is the `code`, the commit message is the
// `title` + `narration`. This is the "spam commit like a save button" pipeline.
//
//   node bin/from-git.mjs <range> <path> [lang]
//   node bin/from-git.mjs feat/x..HEAD v5/src/engine.rs rust
//
// Commit small (one idea per commit) and the log animates cleanly.
import { execSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'

const [range, file, lang = 'rust'] = process.argv.slice(2)
if (!range || !file) {
  console.error('usage: from-git <range> <path> [lang]')
  process.exit(1)
}

const sh = (c) => execSync(c, { encoding: 'utf8' })
const shas = sh(`git log --reverse --format=%H ${range} -- ${file}`).trim().split('\n').filter(Boolean)

const frames = shas.map((sha) => {
  const title = sh(`git show -s --format=%s ${sha}`).trim()
  const narration = sh(`git show -s --format=%b ${sha}`).trim() || title
  let code = ''
  try { code = sh(`git show ${sha}:${file}`) } catch { code = '% (file absent at this commit)\n' }
  return { title, narration, lang, code }
})

const out = new URL('../src/frames.json', import.meta.url)
writeFileSync(out, JSON.stringify(frames, null, 2) + '\n')
console.log(`wrote ${frames.length} frames from ${file} (${range}) -> src/frames.json`)
