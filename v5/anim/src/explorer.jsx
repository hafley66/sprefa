// Interactive type-graph explorer. Progressive hop-reveal instead of a static
// hairball: only roots show at first; clicking a node reveals its direct
// dependencies and re-runs layout. Visibility is recomputed from the roots
// through the set of expanded nodes each time, so collapse, cycles, and
// multiple parents all fall out without bookkeeping.
//
// Data is public/types.json, produced by `node bin/types-to-d2.mjs --json`.

import cytoscape from 'cytoscape'
import fcose from 'cytoscape-fcose'
cytoscape.use(fcose)

const EDGE_COLOR = { field: '#88c0d0', variant: '#b48ead', impl: '#ebcb8b', generic: '#5e81ac' }

main()
async function main() {
const data = await fetch('./types.json').then(r => r.json())
const byId = new Map(data.nodes.map(n => [n.id, n]))
const out = new Map()                                  // node -> [{to, kind}]
for (const e of data.edges) {
  if (!out.has(e.source)) out.set(e.source, [])
  out.get(e.source).push({ to: e.target, kind: e.kind })
}

// roots = nothing depends on them, and not a foreign/std type. These are the
// natural entry points of the graph (Engine, Db, Program, Cli, ...).
let roots = data.nodes.filter(n => n.indeg === 0 && !n.external).map(n => n.id)
if (roots.length === 0) roots = data.nodes.slice().sort((a, b) => b.outdeg - a.outdeg).slice(0, 5).map(n => n.id)

const expanded = new Set()

function visibleSet() {
  const vis = new Set(roots)
  const q = [...roots]
  while (q.length) {
    const n = q.shift()
    if (!expanded.has(n)) continue
    for (const { to } of out.get(n) || []) if (!vis.has(to)) { vis.add(to); q.push(to) }
  }
  return vis
}

const nodeShape = (n) => n.external ? 'ellipse' : n.isOwner ? 'round-rectangle' : 'hexagon'
const nodeFill = (n) => n.external ? '#4c566a' : n.isOwner ? '#b48ead' : '#a3be8c'

const cy = cytoscape({
  container: document.getElementById('cy'),
  minZoom: 0.15,
  maxZoom: 3,
  style: [
    {
      selector: 'node',
      style: {
        shape: 'data(shape)', 'background-color': 'data(fill)',
        label: 'data(label)', color: '#2e3440', 'font-size': 12,
        'font-weight': 600, 'text-valign': 'center', 'text-halign': 'center',
        width: 'label', height: 28, 'padding': '6px',
        'border-width': 1, 'border-color': '#2e3440',
      },
    },
    // a visible node that still has hidden children: dashed ring = "click to expand"
    { selector: 'node.expandable', style: { 'border-width': 3, 'border-style': 'dashed', 'border-color': '#eceff4' } },
    { selector: 'node:selected', style: { 'border-width': 3, 'border-color': '#88c0d0' } },
    {
      selector: 'edge',
      style: {
        width: 1.5, 'line-color': 'data(color)', 'target-arrow-color': 'data(color)',
        'target-arrow-shape': 'triangle', 'curve-style': 'bezier', 'arrow-scale': 0.9,
        opacity: 0.7,
      },
    },
  ],
})

const LAYOUT = {
  name: 'fcose', animate: true, animationDuration: 450, randomize: false,
  fit: true, padding: 50, nodeRepulsion: 9000, idealEdgeLength: 95, nestingFactor: 0.1,
}

function render() {
  const vis = visibleSet()
  // drop nodes that are no longer visible
  cy.nodes().forEach(el => { if (!vis.has(el.id())) el.remove() })
  // add newly visible nodes (keeps existing positions → smooth relayout)
  for (const id of vis) {
    if (cy.getElementById(id).length) continue
    const n = byId.get(id)
    cy.add({ group: 'nodes', data: { id, label: n.label, shape: nodeShape(n), fill: nodeFill(n) } })
  }
  // sync edges among visible nodes
  const wantEdge = new Set()
  for (const e of data.edges) {
    if (!vis.has(e.source) || !vis.has(e.target)) continue
    const eid = `${e.source}__${e.target}`
    wantEdge.add(eid)
    if (!cy.getElementById(eid).length)
      cy.add({ group: 'edges', data: { id: eid, source: e.source, target: e.target, color: EDGE_COLOR[e.kind] || '#777' } })
  }
  cy.edges().forEach(el => { if (!wantEdge.has(el.id())) el.remove() })
  // mark which visible nodes still have something to reveal
  cy.nodes().forEach(el => {
    const kids = out.get(el.id()) || []
    const hidden = kids.some(k => !cy.getElementById(k.to).length)
    el.toggleClass('expandable', hidden && !expanded.has(el.id()))
  })
  cy.layout(LAYOUT).run()
}

cy.on('tap', 'node', (ev) => {
  const id = ev.target.id()
  if (expanded.has(id)) expanded.delete(id); else expanded.add(id)
  render()
})

document.getElementById('reset').onclick = () => { expanded.clear(); render() }
document.getElementById('collapse').onclick = () => { expanded.clear(); render() }
document.getElementById('search').addEventListener('keydown', (e) => {
  if (e.key !== 'Enter') return
  const term = e.target.value.trim()
  if (!byId.has(term)) { e.target.style.borderColor = '#bf616a'; return }
  e.target.style.borderColor = '#4c566a'
  // seed: make the searched type a root and expand it
  roots = [term]; expanded.clear(); expanded.add(term); render()
  cy.getElementById(term).select()
})

render()
}
