// Nav tree: cli wrapper process > its ancestry/child processes by ppid > project > file > test.
// Real `kind: 'process'` rows plus synthetic nodes parsed from their `ancestry` strings (the
// sh/pnpm/node chain above vitest, known only through the `ps` walk, not its own otel resource).
import { groupBy, sortBy } from 'lodash-es'
import type { NavRow } from '@hafley66/report-shell'
import type { Event } from '../../report/timeline.js'
import { verdictOf, type Verdict } from '../lib/verdicts.js'
import { collectProcessRows, projectOfRows, shortCommand, type ProcessRow } from '../lib/processRows.js'
import { trimSyntheticRoots } from '../lib/navRoots.js'
import { buildHostIndex, hostPidOf } from './hostPid.js'

export type { Verdict }

export type NavNode = NavRow<{
  kind: 'process' | 'project' | 'file' | 'test'
  pid?: number
  file?: string
  test?: string | null
  faded?: boolean
  // Vitest project name for a file row (from Event.project); undefined for process/test rows.
  project?: string
}>

// Just the short command: pid and span count move to the nav tree's secondary line
// (Nav.tsx / NavColumns.tsx), read straight off this node's own `pid` and `events` fields.
function processLabel(row: ProcessRow): string {
  return shortCommand(row.command)
}

// file > test children scoped to one process's rows. `scope` (the parent's pid or host key) lands in every
// id: the same file runs under several processes (shards, reruns in one out dir), and TanStack keys rows by id.
// No selection state here: `model.selected` is read by Nav.tsx when it paints the row, so a click
// never rebuilds this tree (model.ts `nav` depends on rows/verdicts/prefs only).
function fileTestChildren(rows: Event[], verdicts: Map<string, Verdict>, scope: string): NavNode[] {
  const byFile = new Map<string, Map<string, { start: number; end: number }>>()
  const rowsByFile = new Map<string, Event[]>()
  for (const e of rows) {
    if (!e.test || e.kind === 'verdict') continue
    const files = byFile.get(e.file) ?? new Map()
    const span = files.get(e.test) ?? { start: e.t, end: e.t }
    span.start = Math.min(span.start, e.t)
    span.end = Math.max(span.end, e.t + (e.durationMs ?? 0))
    files.set(e.test, span)
    byFile.set(e.file, files)
    const fileRows = rowsByFile.get(e.file)
    if (fileRows) fileRows.push(e)
    else rowsByFile.set(e.file, [e])
  }
  return sortBy([...byFile.entries()], ([file]) => file).map(([file, tests]) => {
    const testNodes: NavNode[] = sortBy([...tests.entries()], ([test]) => test).map(([test, span]) => {
      const verdict = verdictOf(verdicts, file, test)
      return {
        id: `test:${scope}/${file}::${test}`,
        kind: 'test' as const,
        label: test,
        status: verdict.status ?? 'none',
        durationMs: verdict.durationMs || Math.round(span.end - span.start),
        events: 0,
        file,
        test,
      }
    })
    const status = testNodes.some((t) => t.status === 'fail') ? 'fail' : testNodes.some((t) => t.status === 'pass') ? 'pass' : 'none'
    return {
      id: `file:${scope}/${file}`,
      kind: 'file' as const,
      label: file,
      status,
      durationMs: testNodes.reduce((sum, t) => sum + t.durationMs, 0),
      events: 0,
      file,
      project: projectOfRows(rowsByFile.get(file) ?? []),
      children: testNodes,
    }
  })
}

function toNavNode(row: ProcessRow, children: NavNode[]): NavNode {
  return {
    id: `process:${row.pid}`,
    kind: 'process',
    label: processLabel(row),
    status: 'none',
    durationMs: row.durationMs,
    events: row.spanCount,
    pid: row.pid,
    children,
  }
}

// Two passes over `rows` total: one to bucket every event under its own process and record which
// tests a real pid already covers, one to collect the leftovers. The per-process `rows.filter`
// this replaced ran processes x events (279k events x 296 processes on the gothic report).
function partitionRows(rows: Event[], processes: Map<number, ProcessRow>): { byPid: Map<number, Event[]>; orphans: Event[] } {
  const byPid = new Map<number, Event[]>()
  const attributedTests = new Set<string>()
  for (const e of rows) {
    if (e.pid == null || !processes.has(e.pid)) continue
    if (e.test) attributedTests.add(`${e.file}::${e.test}`)
    if (e.kind === 'process') continue
    const own = byPid.get(e.pid)
    if (own) own.push(e)
    else byPid.set(e.pid, [e])
  }
  const orphans: Event[] = []
  for (const e of rows) {
    if (e.kind === 'process' || !e.test) continue
    if (!attributedTests.has(`${e.file}::${e.test}`)) orphans.push(e)
  }
  return { byPid, orphans }
}

export function buildProcessNav(rows: Event[], verdicts: Map<string, Verdict>): NavNode[] {
  const processes = collectProcessRows(rows)
  const { byPid, orphans } = partitionRows(rows, processes)
  const nodeByPid = new Map<number, NavNode>()
  for (const [pid, row] of processes) {
    const fileChildren = fileTestChildren(byPid.get(pid) ?? [], verdicts, String(pid))
    nodeByPid.set(pid, toNavNode(row, fileChildren))
  }

  const roots: NavNode[] = []
  for (const [pid, row] of processes) {
    const node = nodeByPid.get(pid)!
    const parent = row.ppid != null ? nodeByPid.get(row.ppid) : undefined
    if (parent) parent.children = [...(parent.children ?? []), node]
    else roots.push(node)
  }

  // Events with no process resource (browser page spans, junit verdicts) hang off the process
  // that opened their trace context (hostPid.ts), grouped per realm as a synthetic child such as
  // "browser page" under the vitest main that launched the page. A test already reachable through
  // a real pid is excluded so it renders once (partitionRows above).
  const hostIndex = buildHostIndex(rows)
  const byHost = groupBy(orphans, (e) => `${hostPidOf(e, hostIndex) ?? 'none'}\u0000${e.realm}`)
  for (const [key, group] of Object.entries(byHost)) {
    const [hostPid, realm] = key.split('\u0000')
    const children = fileTestChildren(group, verdicts, `${hostPid}:${realm}`)
    if (!children.length) continue
    const node: NavNode = { id: `process:${hostPid}:${realm}`, kind: 'process', label: `${realm} page · via trace`, status: 'none', durationMs: 0, events: group.length, children }
    const host = nodeByPid.get(Number(hostPid))
    if (host) host.children = [...(host.children ?? []), node]
    else roots.push(node)
  }

  const trimmed = trimSyntheticRoots(roots, (node) => node.pid != null && (processes.get(node.pid)?.synthetic ?? false))
  return sortBy(trimmed, (root) => (root.pid != null ? (processes.get(root.pid)?.t ?? 0) : Number.POSITIVE_INFINITY))
}

// Flat leaf list for pivot: pivot filters by a plain column value, which only makes sense over a
// flat row set, not a tree where children are hidden inside `.children`.
export function flattenLeaves(nodes: NavNode[]): NavNode[] {
  const leaves: NavNode[] = []
  const walk = (level: NavNode[]) => {
    for (const node of level) {
      if (node.kind === 'test') leaves.push(node)
      if (node.children) walk(node.children)
    }
  }
  walk(nodes)
  return leaves
}
