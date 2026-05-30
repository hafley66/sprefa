// Parse the markdown authoring format (src/frames.md) into src/frames.json and
// render any inline d2 graph blocks to public/*.svg. This is the format an AI (or
// you) writes: fenced code + fenced d2, no JSON escaping.
//
// Frame grammar (one frame per `## ` heading):
//   ## the title
//   prose lines  -> narration (joined)
//   graph: name  -> reuse an already-defined graph /name.svg   (optional)
//   ```prolog        ```rust ...   -> the code block (info string = lang)
//   ```d2 name   ...              -> inline graph; rendered to /name.svg
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, statSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const MD = path.join(root, 'src/frames.md')
const DECK = path.join(root, 'src/deck')
const OUT = path.join(root, 'src/frames.json')
const KIT = path.join(root, 'src/kit.d2')
const GRAPHS = path.join(root, 'graphs')
const PUBLIC = path.join(root, 'public')

// Source = a src/deck/ tree of chapter files (the FS is the table of contents),
// or a single src/frames.md. The tree is walked in sorted order; each file's
// derived name is the chapter shown in the breadcrumb/outline.
function walkMd(dir) {
  const out = []
  for (const name of readdirSync(dir).sort()) {
    const p = path.join(dir, name)
    if (statSync(p).isDirectory()) out.push(...walkMd(p))
    else if (name.endsWith('.md')) out.push(p)
  }
  return out
}
function chapterName(p) {
  return path.relative(DECK, p).replace(/\.md$/, '')
    .split(path.sep)
    .map((s) => s.replace(/^\d+[-_]?/, '').replace(/[-_]/g, ' '))
    .join(' · ')
}
// `code: ../src/foo.rs#L10-24 [as lang]` pulls a snippet from a real file at build
// time instead of pasting it — kills copy-drift and token cost. Paths resolve
// relative to the app root. Line range optional (whole file if omitted).
const EXT_LANG = {
  rs: 'rust', ts: 'typescript', tsx: 'tsx', js: 'javascript', jsx: 'jsx', mjs: 'javascript',
  py: 'python', go: 'go', c: 'c', h: 'cpp', cc: 'cpp', cpp: 'cpp', hpp: 'cpp', java: 'java',
  rb: 'ruby', sql: 'sql', sh: 'bash', bash: 'bash', json: 'json', toml: 'toml',
  yaml: 'yaml', yml: 'yaml', md: 'markdown', prolog: 'prolog', pl: 'prolog',
}
function resolveCodeRef(spec) {
  let lang = null
  const asM = spec.match(/\s+as\s+(\S+)\s*$/)
  if (asM) { lang = asM[1]; spec = spec.slice(0, asM.index) }
  const m = spec.trim().match(/^(.+?)(?:#L(\d+)(?:-(\d+))?)?$/)
  if (!m) return null
  const fp = path.resolve(root, m[1])
  if (!existsSync(fp)) { console.error(`code: file not found: ${m[1]}`); return null }
  const all = readFileSync(fp, 'utf8').split('\n')
  const a = m[2] ? +m[2] : 1
  const b = m[3] ? +m[3] : (m[2] ? a : all.length)
  let slice = all.slice(a - 1, b)
  const indent = Math.min(...slice.filter((l) => l.trim()).map((l) => l.match(/^\s*/)[0].length))
  if (isFinite(indent) && indent > 0) slice = slice.map((l) => l.slice(indent))
  return { code: slice.join('\n').replace(/\s+$/, '') + '\n', lang: lang || EXT_LANG[path.extname(fp).slice(1).toLowerCase()] || 'text' }
}

function collectSources() {
  if (existsSync(DECK)) return walkMd(DECK).map((file) => ({ file, chapter: chapterName(file), slug: path.basename(file, '.md') }))
  return [{ file: MD, chapter: '', slug: 'frames' }]
}

// The deck's own graph: chapters become containers, slides become nodes that link
// to their frame (#index), and [[links]] become edges. Same d2 pipeline renders it,
// so circular references between slides auto-colour as cycles.
const d2str = (s) => String(s).replace(/"/g, "'").replace(/\s+/g, ' ').trim()
function buildMapD2(frames) {
  const norm = (x) => String(x).toLowerCase().replace(/^\d+[-_]?/, '').replace(/[^a-z0-9]/g, '')
  const order = [], cidOf = new Map()
  frames.forEach((f) => {
    if (!cidOf.has(f.chapterSlug)) { const cid = 'c' + order.length; cidOf.set(f.chapterSlug, cid); order.push({ cid, slug: f.chapterSlug, name: f.chapter || f.chapterSlug }) }
  })
  let s = 'direction: down\n'
  for (const ch of order) {
    s += `${ch.cid}: "${d2str(ch.name)}" {\n`
    frames.forEach((f, idx) => { if (f.chapterSlug === ch.slug) s += `  n${idx}: "${idx + 1} · ${d2str(f.title)}" { link: "#${idx}" }\n` })
    s += `}\n`
  }
  const resolve = (L) => {
    const t = norm(L)
    let idx = frames.findIndex((f) => norm(f.chapterSlug) === t)
    if (idx >= 0) return idx
    return frames.findIndex((f) => norm(f.title) === t)
  }
  frames.forEach((f, src) => {
    for (const L of f.links || []) {
      const tgt = resolve(L)
      if (tgt >= 0 && tgt !== src) s += `${cidOf.get(f.chapterSlug)}.n${src} -> ${cidOf.get(frames[tgt].chapterSlug)}.n${tgt}\n`
    }
  })
  return s
}

export function parseFrames(md) {
  const frames = []
  const graphs = []
  let cur = null
  let inFence = false, info = '', body = []

  const finishFrame = () => {
    if (!cur) return
    // narration is raw markdown (lists, bold, links, inline code survive)
    cur.narration = cur._narr.join('\n').replace(/^\n+/, '').replace(/\n+$/, '')
    // [[other-slide]] cross-links feed the deck's own import/export graph
    cur.links = [...cur.narration.matchAll(/\[\[([^\]]+)\]\]/g)].map((m) => m[1].trim())
    delete cur._narr
    frames.push(cur)
  }

  for (const line of md.split('\n')) {
    if (!inFence && line.startsWith('## ')) {
      finishFrame()
      cur = { title: line.slice(3).trim(), narration: '', lang: 'text', code: '', graph: null, _narr: [] }
      continue
    }
    if (!cur) continue

    const fence = line.match(/^```(.*)$/)
    if (fence && !inFence) { inFence = true; info = fence[1].trim(); body = []; continue }
    if (fence && inFence) {
      inFence = false
      const [kind, name] = info.split(/\s+/)
      const text = body.join('\n').replace(/\s*$/, '') + '\n'
      if (kind === 'd2') {
        const gname = name || cur.title.replace(/\W+/g, '-').toLowerCase()
        graphs.push({ name: gname, src: text })
        cur.graph = `/${gname}.svg`
      } else {
        cur.lang = kind || 'text'
        cur.code = text
      }
      continue
    }
    if (inFence) { body.push(line); continue }

    const g = line.match(/^graph:\s*(\S+)\s*$/)
    if (g) { cur.graph = g[1].startsWith('/') ? g[1] : `/${g[1]}.svg`; continue }
    const c = line.match(/^code:\s+(.+?)\s*$/)
    if (c) { cur.codeRef = c[1]; continue }
    // anchor: <code-token> -> <graph node>[, node]  binds code to graph nodes
    const an = line.match(/^anchor:\s*(.+?)\s*->\s*(.+)$/)
    if (an) { (cur.anchors ||= []).push({ token: an[1].trim(), nodes: an[2].split(',').map((s) => s.trim()).filter(Boolean) }); continue }
    cur._narr.push(line)
  }
  finishFrame()
  return { frames, graphs }
}

// Generic loop coloring: find cycles (SCCs) in a d2 graph and tint their nodes,
// one colour per cycle. So any graph gets its loops highlighted without hand
// styling. Opt out by putting `# noautocolor` in the d2 block. Nodes that already
// set their own style.fill are left alone.
const CYCLE_PALETTE = ['#bf616a', '#a3be8c', '#b48ead', '#ebcb8b', '#d08770', '#88c0d0']
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

function tarjan(nodes, adj) {
  let idx = 0
  const stack = [], onStack = new Set(), index = new Map(), low = new Map(), out = []
  const strong = (v) => {
    index.set(v, idx); low.set(v, idx); idx++; stack.push(v); onStack.add(v)
    for (const w of adj.get(v) || []) {
      if (!index.has(w)) { strong(w); low.set(v, Math.min(low.get(v), low.get(w))) }
      else if (onStack.has(w)) { low.set(v, Math.min(low.get(v), index.get(w))) }
    }
    if (low.get(v) === index.get(v)) {
      const comp = []; let w
      do { w = stack.pop(); onStack.delete(w); comp.push(w) } while (w !== v)
      out.push(comp)
    }
  }
  for (const v of nodes) if (!index.has(v)) strong(v)
  return out
}

export function autoColorCycles(src) {
  const edges = [], nodes = new Set()
  for (const raw of src.split('\n')) {
    const l = raw.trim()
    if (l.startsWith('#')) continue
    const m = l.match(/^([A-Za-z0-9_.-]+(?:\s*->\s*[A-Za-z0-9_.-]+)+)/)
    if (!m) continue
    const parts = m[1].split('->').map((s) => s.trim())
    for (let i = 0; i + 1 < parts.length; i++) { edges.push([parts[i], parts[i + 1]]); nodes.add(parts[i]); nodes.add(parts[i + 1]) }
  }
  if (!edges.length) return src
  const adj = new Map([...nodes].map((n) => [n, []]))
  for (const [a, b] of edges) adj.get(a).push(b)
  const selfLoop = new Set(edges.filter(([a, b]) => a === b).map(([a]) => a))
  const cycles = tarjan([...nodes], adj).filter((s) => s.length > 1 || s.some((n) => selfLoop.has(n)))
  if (!cycles.length) return src

  let add = '\n# auto: cycle highlight\n'
  cycles.forEach((scc, idx) => {
    const color = CYCLE_PALETTE[idx % CYCLE_PALETTE.length]
    for (const n of scc) {
      if (new RegExp(`${escapeRe(n)}[^\\n]*style\\.fill`).test(src)) continue // respect manual fill
      add += `${n}.style.fill: "${color}"\n${n}.style.stroke: "#2e3440"\n`
    }
  })
  return src + add
}

export function buildFrames() {
  const frames = [], graphs = []
  for (const { file, chapter, slug } of collectSources()) {
    const r = parseFrames(readFileSync(file, 'utf8'))
    for (const f of r.frames) { f.chapter = chapter; f.chapterSlug = slug }
    frames.push(...r.frames)
    graphs.push(...r.graphs)
  }
  // resolve `code:` source-spans against real files
  for (const f of frames) {
    if (f.codeRef && !(f.code && f.code.trim())) {
      const r = resolveCodeRef(f.codeRef)
      if (r) { f.code = r.code; f.lang = r.lang }
    }
  }
  graphs.push({ name: '_map', src: buildMapD2(frames) })
  mkdirSync(GRAPHS, { recursive: true })
  mkdirSync(PUBLIC, { recursive: true })
  const kit = existsSync(KIT) ? readFileSync(KIT, 'utf8') + '\n' : ''
  for (const g of graphs) {
    const d2path = path.join(GRAPHS, `${g.name}.d2`)
    const noauto = /(^|\n)\s*#\s*noautocolor/.test(g.src)
    const body = noauto ? g.src : autoColorCycles(g.src)
    writeFileSync(d2path, kit + body)
    try {
      execFileSync('d2', [d2path, path.join(PUBLIC, `${g.name}.svg`)], { stdio: 'pipe' })
    } catch (e) {
      console.error(`d2 failed for ${g.name}: ${e.message}`)
    }
  }
  writeFileSync(OUT, JSON.stringify(frames, null, 2) + '\n')
  return { frames: frames.length, graphs: graphs.length }
}

const KIT_CLASSES = new Set(['fn', 'relation', 'type', 'module', 'sink', 'dead', 'hub', 'ghost'])
// Lint the deck without launching the app: an AI gets compiler-style errors it can
// fix in one turn. Checks broken [[links]], undefined graphs, missing code: files,
// unknown kit classes, empty frames, anchors without a graph.
export function checkDeck() {
  const norm = (s) => String(s).toLowerCase().replace(/^\d+[-_]?/, '').replace(/[^a-z0-9]/g, '')
  const sources = collectSources()
  const perFile = sources.map((s) => ({ ...s, ...parseFrames(readFileSync(s.file, 'utf8')) }))
  const graphNames = new Set(perFile.flatMap((pf) => pf.graphs.map((g) => g.name)))
  const slugs = new Set(perFile.flatMap((pf) => pf.frames.map(() => norm(pf.slug))))
  const titles = new Set(perFile.flatMap((pf) => pf.frames.map((f) => norm(f.title))))
  const diags = []
  for (const pf of perFile) {
    const rel = path.relative(root, pf.file)
    for (const f of pf.frames) {
      const at = `${rel} › "${f.title}"`
      if (!(f.narration || '').trim() && !(f.code || '').trim() && !f.codeRef && !f.graph) diags.push(`ERROR ${at}: empty frame`)
      if (f.graph) { const n = f.graph.replace(/^\//, '').replace(/\.svg$/, ''); if (!graphNames.has(n)) diags.push(`ERROR ${at}: graph "${n}" is never defined`) }
      if (f.codeRef && !resolveCodeRef(f.codeRef)) diags.push(`ERROR ${at}: code: ${f.codeRef} did not resolve`)
      for (const L of f.links || []) if (!slugs.has(norm(L)) && !titles.has(norm(L))) diags.push(`WARN  ${at}: [[${L}]] resolves to nothing`)
      if ((f.anchors || []).length && !f.graph) diags.push(`WARN  ${at}: anchor(s) but no graph in this frame`)
    }
    for (const g of pf.graphs) for (const m of g.src.matchAll(/\.class:\s*(\w+)/g)) if (!KIT_CLASSES.has(m[1])) diags.push(`WARN  ${rel} (graph ${g.name}): unknown kit class "${m[1]}"`)
  }
  return diags
}

if (import.meta.url === `file://${process.argv[1]}`) {
  if (process.argv.includes('--check')) {
    const d = checkDeck()
    d.forEach((x) => console.log(x))
    const errs = d.filter((x) => x.startsWith('ERROR')).length
    console.log(`${d.length} issue(s), ${errs} error(s)`)
    process.exit(errs ? 1 : 0)
  }
  const r = buildFrames()
  console.log(`built ${r.frames} frames, ${r.graphs} graphs`)
}
