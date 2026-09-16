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
function fileTestChildren(rows: Event[], verdicts: Map<string, Verdict>, selectedTest: { file: string | null; test: string | null }, scope: string): NavNode[] {
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
    rowsByFile.set(e.file, [...(rowsByFile.get(e.file) ?? []), e])
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
        selected: selectedTest.file === file && selectedTest.test === test,
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
      selected: selectedTest.file === file && !selectedTest.test,
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

export function buildProcessNav(
  rows: Event[],
  verdicts: Map<string, Verdict>,
  selectedTest: { file: string | null; test: string | null },
): NavNode[] {
  const processes = collectProcessRows(rows)
  const nodeByPid = new Map<number, NavNode>()
  for (const [pid, row] of processes) {
    const ownRows = rows.filter((e) => e.pid === pid && e.kind !== 'process')
    const fileChildren = fileTestChildren(ownRows, verdicts, selectedTest, String(pid))
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
  // a real pid is excluded so it renders once.
  const attributedTests = new Set(
    rows.filter((e) => e.pid != null && processes.has(e.pid) && e.test).map((e) => `${e.file}::${e.test}`),
  )
  const orphanRows = rows.filter((e) => e.kind !== 'process' && e.test && !attributedTests.has(`${e.file}::${e.test}`))
  const hostIndex = buildHostIndex(rows)
  const byHost = groupBy(orphanRows, (e) => `${hostPidOf(e, hostIndex) ?? 'none'}\u0000${e.realm}`)
  for (const [key, group] of Object.entries(byHost)) {
    const [hostPid, realm] = key.split('\u0000')
    const children = fileTestChildren(group, verdicts, selectedTest, `${hostPid}:${realm}`)
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
