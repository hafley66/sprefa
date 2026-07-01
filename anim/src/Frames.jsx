import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { ShikiMagicMove } from 'shiki-magic-move/react'
import { marked } from 'marked'
import panzoom from 'panzoom'

// One frame === one idea. Code and graph are both OPTIONAL: a frame can be pure
// prose (a durable discussion note), prose + code, prose + graph, or all three.
// Frames carry a `chapter` (from the src/deck/ tree) shown as a breadcrumb, and
// `o` opens an outline of the whole tree to jump around.
const reEscape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
// Wrap the first occurrence of each glossary term in the narration DOM with an
// <abbr> hover card. Walks text nodes so it never breaks code spans / links.
function wrapGlossary(root, gloss) {
  if (!root || !gloss) return
  const terms = Object.keys(gloss)
  if (!terms.length) return
  const re = new RegExp('\\b(' + terms.map(reEscape).sort((a, b) => b.length - a.length).join('|') + ')\\b', 'i')
  const used = new Set()
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.nodeValue.trim() && !n.parentElement.closest('abbr, a, code, .xref')) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
  })
  const targets = []
  for (let n = walker.nextNode(); n; n = walker.nextNode()) targets.push(n)
  for (const tn of targets) {
    const text = tn.nodeValue
    const rg = new RegExp(re.source, 'ig')
    let m, last = 0, found = false
    const frag = document.createDocumentFragment()
    while ((m = rg.exec(text))) {
      const key = terms.find((k) => k.toLowerCase() === m[0].toLowerCase())
      if (used.has(key)) continue
      used.add(key)
      found = true
      frag.appendChild(document.createTextNode(text.slice(last, m.index)))
      const ab = document.createElement('abbr')
      ab.className = 'gloss'; ab.title = gloss[key]; ab.textContent = m[0]
      frag.appendChild(ab)
      last = m.index + m[0].length
    }
    if (found) { frag.appendChild(document.createTextNode(text.slice(last))); tn.replaceWith(frag) }
  }
}

export default function Frames({ frames, highlighter, theme, glossary }) {
  const start = Math.min(Number(sessionStorage.getItem('frame') || 0), frames.length - 1)
  const [i, setI] = useState(start)
  const [outline, setOutline] = useState(false)
  const [map, setMap] = useState(false)
  const [lit, setLit] = useState(null) // graph node labels to highlight on anchor hover
  const codeWrapRef = useRef(null)
  const narrationRef = useRef(null)
  useEffect(() => { sessionStorage.setItem('frame', String(i)) }, [i])
  useEffect(() => { setLit(null) }, [i])
  const f = frames[i]

  // anchor hover: light the graph node(s) and the matching code token together
  const markCode = (token, on) => {
    const r = codeWrapRef.current
    if (!r) return
    r.querySelectorAll('span').forEach((s) => { if (s.textContent.trim() === token) s.classList.toggle('code-lit', on) })
  }
  const hoverAnchor = (a, on) => { setLit(on ? a.nodes : null); markCode(a.token, on) }

  // wrap glossary terms in the narration after each frame renders
  useEffect(() => { wrapGlossary(narrationRef.current, glossary) }, [i, glossary])
  const go = (d) => setI((p) => Math.max(0, Math.min(frames.length - 1, p + d)))

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'o') { setOutline((v) => !v); setMap(false); return }
      if (e.key === 'm') { setMap((v) => !v); setOutline(false); return }
      if (e.key === 'Escape') { setOutline(false); setMap(false); return }
      if (e.key === 'ArrowRight' || e.key === ' ') { e.preventDefault(); go(1) }
      if (e.key === 'ArrowLeft') go(-1)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const hasCode = !!(f.code && f.code.trim())
  const hasGraph = !!f.graph
  const hasRight = !!(f.graph || f.fs || f.git)
  const html = useMemo(() => {
    // render [[other-slide]] cross-links as styled references
    const src = (f.narration || '').replace(/\[\[([^\]]+)\]\]/g, '<span class="xref">$1</span>')
    return marked.parse(src, { breaks: true, gfm: true })
  }, [f.narration])

  // group frames by chapter for the outline (tree = table of contents)
  const chapters = useMemo(() => {
    const out = []
    frames.forEach((fr, idx) => {
      const ch = fr.chapter || '·'
      let last = out[out.length - 1]
      if (!last || last.chapter !== ch) { last = { chapter: ch, items: [] }; out.push(last) }
      last.items.push({ idx, title: fr.title })
    })
    return out
  }, [frames])

  return (
    <div className="stage">
      <div className={`deck${hasRight ? '' : ' nograph'}${hasCode ? '' : ' nocode'}`}>
        <div className="left">
          <div className="head">
            <div className="counter">
              {f.chapter && <span className="crumb">{f.chapter} › </span>}
              {i + 1} / {frames.length}
            </div>
            <h2 className="title">{f.title}</h2>
          </div>
          <div ref={narrationRef} key={i} className={`narration fade md${hasCode ? '' : ' grow'}`} dangerouslySetInnerHTML={{ __html: html }} />
          {hasCode && (
            <div className="code" ref={codeWrapRef}>
              <ShikiMagicMove
                lang={f.lang}
                theme={theme}
                highlighter={highlighter}
                code={f.code}
                options={{ duration: 700, stagger: 0.2, lineNumbers: false }}
              />
            </div>
          )}
          {f.anchors && f.anchors.length > 0 && (
            <div className="anchors">
              {f.anchors.map((a) => (
                <button
                  key={a.token}
                  className="anchor-chip"
                  onMouseEnter={() => hoverAnchor(a, true)}
                  onMouseLeave={() => hoverAnchor(a, false)}
                >
                  <code>{a.token}</code> → {a.nodes.join(' · ')}
                </button>
              ))}
            </div>
          )}
          <div className="help">← prev · → next · o outline · m map{hasGraph ? ' · scroll/drag graph' : ''}</div>
        </div>
        {hasRight && (
          <div className="right">
            {f.git ? <div className="fs-card"><GitLens commits={f.git} /></div>
              : f.fs ? <div className="fs-card"><FsTree tree={f.fs} /></div>
              : <Graph src={f.graph} lit={lit} />}
          </div>
        )}
      </div>

      {map && <MapView current={i} onJump={(idx) => { setI(idx); setMap(false) }} onClose={() => setMap(false)} />}

      {outline && (
        <div className="outline" onClick={() => setOutline(false)}>
          <div className="outline-panel" onClick={(e) => e.stopPropagation()}>
            <div className="outline-head">outline · the deck tree</div>
            {chapters.map((c) => (
              <div key={c.chapter} className="outline-chapter">
                <div className="outline-chapter-name">{c.chapter}</div>
                {c.items.map((it) => (
                  <button
                    key={it.idx}
                    className={`outline-item${it.idx === i ? ' current' : ''}`}
                    onClick={() => { setI(it.idx); setOutline(false) }}
                  >
                    <span className="outline-num">{it.idx + 1}</span> {it.title}
                  </button>
                ))}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

// Shared keyed-FLIP: rows present before & after slide to their new position; new
// rows fade in. Keyed by [data-key]. Used by the fs and git lenses.
function flipRows(wrap, rectsRef) {
  if (!wrap) return
  const now = new Map()
  wrap.querySelectorAll('[data-key]').forEach((el) => {
    const key = el.dataset.key
    const rect = el.getBoundingClientRect()
    now.set(key, rect)
    const prev = rectsRef.current.get(key)
    if (prev) {
      const dy = prev.top - rect.top
      if (dy) {
        el.style.transition = 'none'; el.style.transform = `translateY(${dy}px)`
        requestAnimationFrame(() => { el.style.transition = 'transform .42s cubic-bezier(.2,.7,.2,1)'; el.style.transform = '' })
      }
    } else {
      el.style.transition = 'none'; el.style.opacity = '0'; el.style.transform = 'translateX(-10px)'
      requestAnimationFrame(() => { el.style.transition = 'opacity .35s ease, transform .35s ease'; el.style.opacity = ''; el.style.transform = '' })
    }
  })
  rectsRef.current = now
}

// Git lens: a commit timeline (newest on top) that FLIP-animates between frames —
// advance the rev and new commits slide in at the top. Keyed by commit sha.
function GitLens({ commits }) {
  const wrapRef = useRef(null)
  const rects = useRef(new Map())
  useLayoutEffect(() => { flipRows(wrapRef.current, rects) }, [commits])
  return (
    <div className="gitlens" ref={wrapRef}>
      {commits.length === 0 && <div className="git-empty">no commits (is the repo path right?)</div>}
      {commits.map((c, idx) => (
        <div className="git-row" data-key={c.sha} key={c.sha}>
          <span className="git-rail"><span className={`git-dot${idx === 0 ? ' head' : ''}`} /></span>
          <span className="git-sha">{c.sha}</span>
          <span className="git-subject">{c.subject}</span>
          {idx === 0 && <span className="git-head">HEAD</span>}
        </div>
      ))}
    </div>
  )
}

// Build ordered explorer rows (dirs + files) from a flat path list.
function buildRows(items) {
  const all = new Map()
  for (const { path, mark } of items) {
    const clean = path.replace(/\/$/, '')
    const parts = clean.split('/')
    for (let k = 1; k < parts.length; k++) { const d = parts.slice(0, k).join('/'); if (!all.has(d)) all.set(d, { path: d, isDir: true, mark: '' }) }
    all.set(clean, { path: clean, isDir: path.endsWith('/') || (all.get(clean)?.isDir ?? false), mark })
  }
  return [...all.values()]
    .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
    .map((n) => ({ key: n.path, depth: n.path.split('/').length - 1, name: n.path.split('/').pop(), isDir: n.isDir, mark: n.mark }))
}

// FS lens: a file-explorer view that FLIP-animates between frames. Rows present in
// both frames slide to their new position; new rows fade in; removed rows fade out.
// Same keyed-FLIP idea as magic-move (token keys) and d2 (node ids) — here, path keys.
function FsTree({ tree }) {
  const rows = useMemo(() => buildRows(tree), [tree])
  const wrapRef = useRef(null)
  const rects = useRef(new Map())
  const prevRows = useRef([])
  const [exiting, setExiting] = useState([])

  useLayoutEffect(() => { flipRows(wrapRef.current, rects) }, [rows])

  useEffect(() => {
    const wrap = wrapRef.current
    const nextKeys = new Set(rows.map((r) => r.key))
    const gone = prevRows.current.filter((r) => !nextKeys.has(r.key))
    if (gone.length && wrap) {
      const top = wrap.getBoundingClientRect().top - wrap.scrollTop
      setExiting(gone.map((r) => ({ ...r, y: (rects.current.get(r.key)?.top ?? 0) - top })))
      const t = setTimeout(() => setExiting([]), 360)
      prevRows.current = rows
      return () => clearTimeout(t)
    }
    prevRows.current = rows
  }, [rows])

  const Row = (r, ghost) => (
    <div
      key={(ghost ? 'g-' : '') + r.key}
      data-key={ghost ? undefined : r.key}
      className={`fs-row${r.isDir ? ' dir' : ''}${r.mark === '*' ? ' focus' : ''}${ghost ? ' fs-ghost' : ''}`}
      style={ghost ? { top: r.y } : { paddingLeft: 8 + r.depth * 16 }}
    >
      <span className="fs-ic">{r.isDir ? '▾' : '·'}</span>
      <span className="fs-name">{r.name}</span>
      {r.mark === '+' && <span className="fs-badge add">added</span>}
      {r.mark === '~' && <span className="fs-badge chg">changed</span>}
    </div>
  )

  return (
    <div className="fstree" ref={wrapRef}>
      {rows.map((r) => Row(r, false))}
      {exiting.map((r) => Row(r, true))}
    </div>
  )
}

// The map: the deck's own structure graph (chapters -> slides, [[links]] as
// edges), rendered by the same d2 pipeline. Nodes link to #index; clicking jumps,
// and the current slide is marked. See the import/export graph from any slide.
function MapView({ current, onJump, onClose }) {
  const panRef = useRef(null)
  const svgRef = useRef(null)

  useEffect(() => {
    if (!panRef.current) return
    const pz = panzoom(panRef.current, { maxZoom: 8, minZoom: 0.2, bounds: true, boundsPadding: 0.1 })
    return () => pz.dispose()
  }, [])

  useEffect(() => {
    const el = svgRef.current
    if (!el) return
    let alive = true
    fetch('/_map.svg')
      .then((r) => r.text())
      .then((txt) => {
        if (!alive) return
        el.innerHTML = txt
        el.querySelectorAll('a').forEach((a) => {
          const href = a.getAttribute('href') || a.getAttributeNS('http://www.w3.org/1999/xlink', 'href') || ''
          if (href === `#${current}`) a.classList.add('map-here')
        })
      })
    return () => { alive = false }
  }, [current])

  const onClick = (e) => {
    const a = e.target.closest('a')
    if (!a) return
    e.preventDefault()
    const href = a.getAttribute('href') || a.getAttributeNS('http://www.w3.org/1999/xlink', 'href') || ''
    const m = href.match(/#(\d+)/)
    if (m) onJump(Number(m[1]))
  }

  return (
    <div className="mapview" onClick={onClose}>
      <div className="map-card" onClick={(e) => e.stopPropagation()}>
        <div className="map-head">map · the deck's own graph · click a node to jump · m / esc to close</div>
        <div className="map-pan" ref={panRef} onClickCapture={onClick}>
          <div className="map-svg" ref={svgRef} />
        </div>
      </div>
    </div>
  )
}

// Inline the SVG so we can animate it (edges draw on, whole graph fades up) and
// wrap it in panzoom for scroll-zoom + drag-pan. The pan target is stable across
// frames, so your zoom/pan persists; the fade is opacity-only so it never fights
// the panzoom transform.
function Graph({ src, lit }) {
  const panRef = useRef(null)
  const svgRef = useRef(null)
  const [ready, setReady] = useState(0)

  useEffect(() => {
    if (!panRef.current) return
    const pz = panzoom(panRef.current, {
      maxZoom: 8, minZoom: 0.3, bounds: true, boundsPadding: 0.05, zoomDoubleClickSpeed: 1,
    })
    return () => pz.dispose()
  }, [])

  useEffect(() => {
    const el = svgRef.current
    if (!el || !src) return
    let alive = true
    fetch(src)
      .then((r) => r.text())
      .then((txt) => {
        if (!alive) return
        el.innerHTML = txt
        el.querySelectorAll('path').forEach((p) => p.setAttribute('pathLength', '1'))
        el.classList.remove('graph-anim')
        void el.offsetWidth
        el.classList.add('graph-anim')
        setReady((n) => n + 1)
      })
    return () => { alive = false }
  }, [src])

  // light the graph node(s) whose label matches an anchor's targets
  useEffect(() => {
    const el = svgRef.current
    if (!el) return
    el.querySelectorAll('g.lit').forEach((g) => g.classList.remove('lit'))
    if (!lit || !lit.length) return
    const want = new Set(lit.map((s) => s.toLowerCase()))
    el.querySelectorAll('text').forEach((t) => {
      if (want.has((t.textContent || '').trim().toLowerCase())) t.closest('g')?.classList.add('lit')
    })
  }, [lit, ready])

  return (
    <div className="graph-viewport">
      <div className="graph-pan" ref={panRef}>
        <div className="graph-svg" ref={svgRef} />
      </div>
      <div className="graph-hint">scroll = zoom · drag = pan</div>
    </div>
  )
}
