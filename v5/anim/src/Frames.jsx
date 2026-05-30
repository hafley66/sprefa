import React, { useEffect, useMemo, useRef, useState } from 'react'
import { ShikiMagicMove } from 'shiki-magic-move/react'
import { marked } from 'marked'
import panzoom from 'panzoom'

// One frame === one idea. Code and graph are both OPTIONAL: a frame can be pure
// prose (a durable discussion note), prose + code, prose + graph, or all three.
// Frames carry a `chapter` (from the src/deck/ tree) shown as a breadcrumb, and
// `o` opens an outline of the whole tree to jump around.
export default function Frames({ frames, highlighter, theme }) {
  const start = Math.min(Number(sessionStorage.getItem('frame') || 0), frames.length - 1)
  const [i, setI] = useState(start)
  const [outline, setOutline] = useState(false)
  const [map, setMap] = useState(false)
  useEffect(() => { sessionStorage.setItem('frame', String(i)) }, [i])
  const f = frames[i]
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
      <div className={`deck${hasGraph ? '' : ' nograph'}${hasCode ? '' : ' nocode'}`}>
        <div className="left">
          <div className="head">
            <div className="counter">
              {f.chapter && <span className="crumb">{f.chapter} › </span>}
              {i + 1} / {frames.length}
            </div>
            <h2 className="title">{f.title}</h2>
          </div>
          <div key={i} className={`narration fade md${hasCode ? '' : ' grow'}`} dangerouslySetInnerHTML={{ __html: html }} />
          {hasCode && (
            <div className="code">
              <ShikiMagicMove
                lang={f.lang}
                theme={theme}
                highlighter={highlighter}
                code={f.code}
                options={{ duration: 700, stagger: 0.2, lineNumbers: false }}
              />
            </div>
          )}
          <div className="help">← prev · → next · o outline · m map{hasGraph ? ' · scroll/drag graph' : ''}</div>
        </div>
        {hasGraph && (
          <div className="right">
            <Graph src={f.graph} />
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
function Graph({ src }) {
  const panRef = useRef(null)
  const svgRef = useRef(null)

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
      })
    return () => { alive = false }
  }, [src])

  return (
    <div className="graph-viewport">
      <div className="graph-pan" ref={panRef}>
        <div className="graph-svg" ref={svgRef} />
      </div>
      <div className="graph-hint">scroll = zoom · drag = pan</div>
    </div>
  )
}
