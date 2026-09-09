// All reactive state for the report viewer lives on one Model object, created once in main.tsx.
// Every component reads the signals it needs through SignalReact and re-renders on change.
//
// `selected` (file/test, param `s`) and `pivotStack` (param `p`) are historyAdapter signals:
// selecting a test or pushing a pivot is a navigation the user expects Back to undo, one entry
// per change. `continuous` (search, kind toggles, min level, failed-only) is urlAdapter
// (replaceState): it changes on every keystroke and must not spam history. `hoveredId` is a plain
// Signal: hover is not shareable state, it never touches the URL or localStorage.
import { createMarbler, DEFAULT_PHASE_STYLES, type Marbler } from '@hafley66/marbler'
import { map } from 'rxjs'
import { Signal, storageSignal, urlAdapter, historyAdapter, type Signal as SignalType } from '@hafley66/signals'
import { pivotStackSignal, popPivotsTo, type PivotEntry } from '@hafley66/report-shell'
import { syncMarbler } from '@hafley66/report-shell/marbler'
import type { Event } from '../report/timeline.js'
import { timelineToMarble } from './adapter/timelineToMarble.js'
import { buildProcessNav, type NavNode } from './adapter/navTree.js'
import { buildVerdicts, verdictOf, type Verdict } from './lib/verdicts.js'
import { findFirstFailure, findFirstLeaf, type Selection } from './lib/navSelection.js'
import { filterFailedOnly } from './lib/failedOnly.js'

export const LEVEL: Record<string, number> = { trace: 0, debug: 1, info: 2, warning: 3, error: 4, fatal: 5 }

export type { Verdict, Selection }
export type Filters = { search: string; kinds: Set<string>; minLevel: string }
export type { PivotEntry }

export type ContinuousState = {
  search: string
  kinds: string[]
  minLevel: string
  failedOnly: boolean
}

export const DEFAULT_CONTINUOUS_STATE: ContinuousState = {
  search: '',
  kinds: ['log', 'span', 'playwright'],
  minLevel: 'debug',
  failedOnly: false,
}

export const DEFAULT_SELECTION: Selection = { file: null, test: null }

const REPORT_PHASE_STYLES: Record<string, { label: string; color: string }> = {
  beforeEach: { label: 'beforeEach', color: '#8e57bc' },
  beforeAll: { label: 'beforeAll', color: '#6b4796' },
  afterEach: { label: 'afterEach', color: '#3f8dbd' },
  afterAll: { label: 'afterAll', color: '#2f6a8e' },
  callback: { label: 'callback', color: '#49a56b' },
  cleanup: { label: 'cleanup', color: '#777f8b' },
}

function encodeSelection(selection: Selection): string {
  return selection.file ? `${selection.file}::${selection.test ?? ''}` : ''
}
function decodeSelection(raw: string): Selection {
  if (!raw) return { ...DEFAULT_SELECTION }
  const at = raw.lastIndexOf('::')
  if (at === -1) return { file: raw, test: null }
  return { file: raw.slice(0, at), test: raw.slice(at + 2) || null }
}

export type Model = {
  rows: SignalType<Event[]>
  continuous: SignalType<ContinuousState>
  selected: SignalType<Selection>
  pivotStack: SignalType<PivotEntry[]>
  hoveredId: SignalType<string | null>
  verdicts: SignalType<Map<string, Verdict>>
  eventsForSelected: SignalType<Event[]>
  nav: SignalType<NavNode[]>
  firstFailure: SignalType<Selection | null>
  // True only while the report is still showing the load-time "first failing test" default view
  // (see main.tsx); Title shows a one-line hint while this is true, dismissDefaultViewHint clears
  // it on the first click anywhere (App.tsx).
  defaultViewHint: SignalType<boolean>
  marbler: Marbler
  unsubscribe: () => void
}

export const endOf = (e: Event): number => e.t + (e.durationMs || 0)

export const isErrorEvent = (e: Event): boolean =>
  e.status === 'error' || e.status === 'fail' || LEVEL[e.level ?? 'debug'] >= LEVEL.error

// Scope: file/test, kind toggles, minimum log level, free text query against the whole event.
export function eventsFor(rows: Event[], filters: Filters, file: string, test: string | null): Event[] {
  const q = filters.search.toLowerCase()
  return rows
    .filter((e) => e.file === file && (!test || e.test === test))
    .filter((e) => filters.kinds.has(e.kind))
    .filter((e) => e.kind !== 'log' || LEVEL[e.level ?? 'debug'] >= LEVEL[filters.minLevel])
    .filter((e) => !q || JSON.stringify(e).toLowerCase().includes(q))
    .sort((a, b) => a.t - b.t || (b.durationMs || 0) - (a.durationMs || 0))
}

export function patchContinuous(model: Pick<Model, 'continuous'>, patch: Partial<ContinuousState>): void {
  model.continuous.$({ ...model.continuous.$(), ...patch })
}

export function createModel(initialRows: Event[]): Model {
  const rows = Signal<Event[]>(initialRows)
  const continuous = storageSignal(urlAdapter('q'), DEFAULT_CONTINUOUS_STATE)
  const selected = storageSignal(historyAdapter('s'), DEFAULT_SELECTION, { serialize: encodeSelection, parse: decodeSelection })
  const pivotStack = pivotStackSignal('p')
  const hoveredId = Signal<string | null>(null)

  const verdicts = Signal<Map<string, Verdict>>(() => buildVerdicts(rows.$()))

  const eventsForSelected = Signal<Event[]>(() => {
    const sel = selected.$()
    if (!sel.file) return []
    const cont = continuous.$()
    const filters: Filters = { search: cont.search, kinds: new Set(cont.kinds), minLevel: cont.minLevel }
    return eventsFor(rows.$(), filters, sel.file, sel.test)
  })

  const nav = Signal<NavNode[]>(() => {
    const built = buildProcessNav(rows.$(), verdicts.$(), selected.$())
    return continuous.$().failedOnly ? filterFailedOnly(built) : built
  })
  const firstFailure = Signal<Selection | null>(() => findFirstFailure(nav.$()))
  const defaultViewHint = Signal<boolean>(false)

  const marbler = createMarbler(timelineToMarble(eventsForSelected.$(), verdicts.$()), { phaseStyles: REPORT_PHASE_STYLES })
  const marblerSync = syncMarbler(
    marbler,
    eventsForSelected.$.pipe(map((events) => timelineToMarble(events, verdicts.$()))),
    selected,
    (a, b) => a.file === b.file && a.test === b.test,
  )

  return { rows, continuous, selected, pivotStack, hoveredId, verdicts, eventsForSelected, nav, firstFailure, defaultViewHint, marbler, unsubscribe: () => marblerSync.unsubscribe() }
}

// A pivot is a drill-down over the nav for the current view; a new selection or the title × ends it.
// Both writes are history entries, so Back undoes them one at a time (pivots first, then selection).
export function clearSelection(model: Pick<Model, 'selected' | 'pivotStack'>): void {
  if (model.pivotStack.$().length) popPivotsTo(model.pivotStack, 0)
  model.selected.$({ ...DEFAULT_SELECTION })
}
export function selectNode(model: Pick<Model, 'selected' | 'pivotStack'>, node: NavNode): void {
  if (node.kind !== 'file' && node.kind !== 'test') return
  if (model.pivotStack.$().length) popPivotsTo(model.pivotStack, 0)
  model.selected.$({ file: node.file ?? null, test: node.test ?? null })
}

export function dismissDefaultViewHint(model: Pick<Model, 'defaultViewHint'>): void {
  if (model.defaultViewHint.$()) model.defaultViewHint.$(false)
}

export { findFirstLeaf, findFirstFailure, DEFAULT_PHASE_STYLES, verdictOf }
export type { NavNode }
