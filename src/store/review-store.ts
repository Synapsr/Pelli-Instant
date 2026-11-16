import { create } from 'zustand'
import type { CullSettings, DecisionCounts, PhotoView, ProjectData, ProjectSummary, Reason, Scene, SimilarGroup, UserMark } from '@/lib/contract'
import { api, toCullError } from '@/lib/api'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { playSound } from '@/lib/sound'
import { usePlusStore } from './plus-store'
import { applyMarkPatch, chooseChanges, countDecisions, type MarkChange } from '@/lib/review/decision'
import {
  clickSelection,
  focusAfterMark,
  markTargets,
  moveFocus,
  moveInRows,
  selectAll,
  slotOf,
  type BurstOf,
  type ClickModifiers,
  type Direction,
} from '@/lib/review/selection'
import {
  burstFrames,
  captureOrder,
  computeCameraCounts,
  computeGroupRows,
  computeReasonCounts,
  computeTabCounts,
  computeVisible,
  DEFAULT_FILTERS,
  groupIdOf,
  isStackClosed,
  sameIds,
  type FilterTab,
  type GroupRow,
  type SortKey,
  type TabCounts,
  type ViewFilters,
} from '@/lib/review/view'
import { usePrefsStore } from './prefs-store'

export interface SearchState {
  query: string
  status: 'idle' | 'loading' | 'done' | 'error'
  hits: Record<string, number> | null
  /** sort to restore when the search is cleared */
  previousSort: SortKey
}

interface UndoEntry {
  projectId: string
  before: Record<string, UserMark>
}

export interface MarkOptions {
  /** explicit targets (default: selection, else focus) */
  ids?: string[]
  /** auto-advance override (default: preference, single target only) */
  advance?: boolean
}

/** "Choose this one": who wins, and against what (default: the rest of its burst). */
export interface ChooseOptions {
  /** the photo kept (default: the focus) */
  id?: string
  /** photos shown next to it (a compare page): all rejected */
  seen?: string[]
  /** the competing photos: only the selected ones are rejected (default: its burst) */
  pool?: string[]
  /** leave the burst afterwards (default: the auto-advance preference) */
  advance?: boolean
}

/** A single photo was just marked: what the confirmation overlay shows. */
export interface MarkFlash {
  /** changes on every mark, so the same mark twice shows twice */
  seq: number
  patch: Partial<UserMark>
}

interface Derived {
  visibleIds: string[]
  groupRows: GroupRow[]
  tabCounts: TabCounts
}

export interface ReviewState extends Derived {
  projectId: string | null
  project: ProjectSummary | null
  photos: Record<string, PhotoView>
  dateOrder: string[]
  groups: Record<number, SimilarGroup>
  groupList: SimilarGroup[]
  scenes: Scene[]
  filters: ViewFilters
  search: SearchState
  reasonCounts: Partial<Record<Reason, number>>
  cameraCounts: { camera: string; count: number }[]
  /** the photo under the cursor: a visible photo, or a frame of the closed stack being walked */
  focusId: string | null
  /** where the focus is shown in `visibleIds` (`slotOf`): itself, or its stack's slot */
  focusSlot: string | null
  /** the closed stack whose frames the cursor walks (⌥← ⌥→), null otherwise */
  walkGroupId: number | null
  selection: Record<string, true>
  selectionCount: number
  anchorId: string | null
  columns: number
  undoStack: UndoEntry[]
  recomputing: boolean
  /** last mark of a single photo (multi-selection marks get a toast instead) */
  flash: MarkFlash | null

  load: (data: ProjectData, options?: { keepView?: boolean }) => void
  reset: () => void
  upsertPhotos: (photos: PhotoView[]) => void
  setTab: (tab: FilterTab) => void
  setSort: (sort: SortKey) => void
  setReason: (reason: Reason | null) => void
  setScene: (sceneId: number | null) => void
  setCamera: (camera: string | null) => void
  setStackGroups: (stack: boolean) => void
  toggleGroup: (groupId: number) => void
  clearFilters: () => void
  setColumns: (columns: number) => void
  focus: (id: string | null) => void
  click: (id: string, mods: ClickModifiers) => void
  selectAllVisible: () => void
  clearSelection: () => void
  move: (dir: Direction) => void
  /** previous (-1) / next (1) frame of the focused photo's burst; a closed stack stays closed */
  moveInGroup: (delta: -1 | 1) => void
  mark: (patch: Partial<UserMark>, options?: MarkOptions) => Promise<void>
  markMany: (changes: MarkChange[], focusId?: string | null) => Promise<void>
  /** keeps a photo and rejects the selected photos competing with it (S, compare); one undo */
  choose: (options?: ChooseOptions) => Promise<void>
  undo: () => Promise<void>
  runSearch: (query: string) => Promise<void>
  clearSearch: () => void
  recompute: (settings: CullSettings) => Promise<{ before: DecisionCounts; after: DecisionCounts } | null>
  counts: () => DecisionCounts
}

const EMPTY_SEARCH: SearchState = { query: '', status: 'idle', hits: null, previousSort: 'date' }

type Base = Pick<ReviewState, 'photos' | 'dateOrder' | 'groups' | 'filters' | 'search' | 'visibleIds' | 'groupRows'>

/** Recomputes the derived lists, keeping array identity when nothing moved. */
function derive(s: Base): Derived {
  const data = { photos: s.photos, dateOrder: s.dateOrder, groups: s.groups }
  const hits = s.search.hits
  const visible = computeVisible(data, s.filters, hits)
  const groupRows = s.filters.tab === 'duplicates' ? computeGroupRows(data, s.filters, hits) : []
  return {
    visibleIds: sameIds(visible, s.visibleIds) ? s.visibleIds : visible,
    groupRows,
    tabCounts: computeTabCounts(data, s.filters, hits),
  }
}

function burstOfState(s: Pick<ReviewState, 'photos' | 'groups'>): BurstOf {
  return (id) => {
    const p = s.photos[id]
    const gid = p ? groupIdOf(p) : null
    return gid === null ? undefined : s.groups[gid]?.photoIds
  }
}

/** Keeps the focus valid once the visible list changed: still shown (or in its stack's slot), else what took its slot. */
function settleFocus(prevVisible: readonly string[], visible: readonly string[], focusId: string | null, burstOf: BurstOf): string | null {
  if (!visible.length) return null
  if (focusId && slotOf(focusId, visible, burstOf)) return focusId
  const slot = slotOf(focusId, prevVisible, burstOf)
  const index = slot ? prevVisible.indexOf(slot) : -1
  return visible[Math.min(Math.max(index, 0), visible.length - 1)]
}

/**
 * Focus, its slot and the walk, consistent: a frame hidden in a closed stack
 * means the cursor walks that stack; on the stack's cover, the walk goes on
 * only if it was already under way (`walk`).
 */
function placeFocus(
  s: Pick<ReviewState, 'photos' | 'groups' | 'filters'>,
  visible: readonly string[],
  focusId: string | null,
  walk: number | null,
): Pick<ReviewState, 'focusId' | 'focusSlot' | 'walkGroupId'> {
  const focusSlot = slotOf(focusId, visible, burstOfState(s))
  const photo = focusId ? s.photos[focusId] : undefined
  const gid = photo ? groupIdOf(photo) : null
  let walkGroupId: number | null = null
  if (focusSlot && gid !== null) {
    if (focusSlot !== focusId) walkGroupId = gid
    else if (walk === gid && isStackClosed(gid, s.filters)) walkGroupId = gid
  }
  return { focusId, focusSlot, walkGroupId }
}

function pruneSelection(selection: Record<string, true>, visible: readonly string[]): Record<string, true> {
  const keys = Object.keys(selection)
  if (!keys.length) return selection
  const present = new Set(visible)
  if (keys.every((k) => present.has(k))) return selection
  const next: Record<string, true> = {}
  keys.forEach((k) => present.has(k) && (next[k] = true))
  return next
}

const initial = {
  projectId: null,
  project: null,
  photos: {},
  dateOrder: [],
  groups: {},
  groupList: [],
  scenes: [],
  filters: DEFAULT_FILTERS,
  search: EMPTY_SEARCH,
  visibleIds: [],
  groupRows: [],
  tabCounts: { selected: 0, toReview: 0, rejected: 0, all: 0, duplicates: 0 },
  reasonCounts: {},
  cameraCounts: [],
  focusId: null,
  focusSlot: null,
  walkGroupId: null,
  selection: {},
  selectionCount: 0,
  anchorId: null,
  columns: 5,
  undoStack: [],
  recomputing: false,
  flash: null,
} satisfies Partial<ReviewState>

export const useReviewStore = create<ReviewState>((set, get) => {
  /** Applies a state change, re-deriving lists and settling focus/selection. */
  const update = (patch: Partial<ReviewState>, explicitFocus?: string | null) => {
    const s = get()
    const next = { ...s, ...patch }
    const derived = derive(next)
    const wanted = explicitFocus !== undefined ? explicitFocus : next.focusId
    const focusId = wanted === null ? null : settleFocus(s.visibleIds, derived.visibleIds, wanted, burstOfState(next))
    const placed = placeFocus(next, derived.visibleIds, focusId, next.walkGroupId)
    const selection = pruneSelection(next.selection, derived.visibleIds)
    set({ ...patch, ...derived, ...placed, selection, selectionCount: Object.keys(selection).length })
  }

  /** Moves the cursor (no re-derivation): a single focus, no multi-selection. */
  const moveTo = (focusId: string, walk: number | null) => {
    const s = get()
    set({ ...placeFocus(s, s.visibleIds, focusId, walk), anchorId: focusId, selection: {}, selectionCount: 0 })
  }

  const setFilters = (filters: Partial<ViewFilters>) => update({ filters: { ...get().filters, ...filters } })

  const applyLocal = (changes: MarkChange[]) => {
    const photos = { ...get().photos }
    const before: Record<string, UserMark> = {}
    for (const { ids, patch } of changes) {
      for (const id of ids) {
        const p = photos[id]
        if (!p) continue
        if (!(id in before)) before[id] = p.mark
        photos[id] = applyMarkPatch(p, patch)
      }
    }
    return { photos, before }
  }

  /** Sends marks to the engine; rolls back the photos still holding our value on failure. */
  const persist = async (projectId: string, changes: MarkChange[], before: Record<string, UserMark>) => {
    try {
      const results = await Promise.all(changes.filter((c) => c.ids.length).map((c) => api().setMarks(projectId, c.ids, c.patch)))
      if (get().projectId !== projectId) return
      const fresh = results.flat()
      const photos = { ...get().photos }
      let changed = false
      for (const p of fresh) {
        const local = photos[p.id]
        // Keep newer local edits; take the engine's view otherwise.
        if (local && JSON.stringify(local.mark) === JSON.stringify(p.mark)) {
          photos[p.id] = { ...local, ...p, thumbUrl: p.thumbUrl ?? local.thumbUrl }
          changed = true
        }
      }
      if (changed) update({ photos })
    } catch {
      if (get().projectId !== projectId) return
      const photos = { ...get().photos }
      for (const [id, mark] of Object.entries(before)) if (photos[id]) photos[id] = applyMarkPatch(photos[id], mark)
      update({ photos, undoStack: get().undoStack.slice(0, -1) })
      notify({ title: t('marks.failed'), tone: 'danger' })
    }
  }

  return {
    ...initial,

    load: (data, options = {}) => {
      const s = get()
      const keep = options.keepView && s.projectId === data.project.id
      const photos: Record<string, PhotoView> = {}
      data.photos.forEach((p) => (photos[p.id] = p))
      const groups: Record<number, SimilarGroup> = {}
      data.groups.forEach((g) => (groups[g.id] = g))
      const base = {
        projectId: data.project.id,
        project: data.project,
        photos,
        dateOrder: captureOrder(data.photos),
        groups,
        groupList: data.groups,
        scenes: data.scenes,
        reasonCounts: computeReasonCounts(data.photos),
        cameraCounts: computeCameraCounts(data.photos),
      }
      if (keep) {
        const sceneIds = new Set(data.scenes.map((sc) => sc.id))
        const filters = s.filters.sceneId !== null && !sceneIds.has(s.filters.sceneId) ? { ...s.filters, sceneId: null } : s.filters
        update({ ...base, filters })
        return
      }
      const fresh = { ...base, filters: DEFAULT_FILTERS, search: EMPTY_SEARCH, selection: {}, anchorId: null, undoStack: [], visibleIds: [], groupRows: [] }
      const derived = derive(fresh)
      const focusId = derived.visibleIds[0] ?? null
      set({ ...fresh, ...derived, focusId, focusSlot: focusId, walkGroupId: null, selectionCount: 0 })
    },

    reset: () => set({ ...initial }),

    upsertPhotos: (incoming) => {
      const s = get()
      if (!incoming.length) return
      const photos = { ...s.photos }
      let added = false
      for (const p of incoming) {
        if (!photos[p.id]) added = true
        photos[p.id] = p
      }
      const all = Object.values(photos)
      update({
        photos,
        dateOrder: added ? captureOrder(all) : s.dateOrder,
        reasonCounts: computeReasonCounts(all),
        cameraCounts: added ? computeCameraCounts(all) : s.cameraCounts,
      })
    },

    setTab: (tab) => setFilters({ tab }),
    setSort: (sort) => setFilters({ sort }),
    setReason: (reason) => setFilters({ reason }),
    setScene: (sceneId) => setFilters({ sceneId }),
    setCamera: (camera) => setFilters({ camera }),
    setStackGroups: (stackGroups) => setFilters({ stackGroups }),
    toggleGroup: (groupId) => {
      const expanded = { ...get().filters.expanded }
      if (expanded[groupId]) delete expanded[groupId]
      else expanded[groupId] = true
      setFilters({ expanded })
    },
    clearFilters: () => {
      setFilters({ reason: null, sceneId: null, camera: null })
      if (get().search.hits) get().clearSearch()
    },
    setColumns: (columns) => {
      if (columns !== get().columns) set({ columns })
    },

    // A frame of the walked stack (the inspector's strip) keeps the walk going.
    focus: (id) => {
      if (id) moveTo(id, get().walkGroupId)
      else set({ focusId: null, focusSlot: null, walkGroupId: null, anchorId: null, selection: {}, selectionCount: 0 })
    },
    click: (id, mods) => {
      const s = get()
      const next = clickSelection(s, id, mods, s.visibleIds, burstOfState(s))
      const count = Object.keys(next.selection).length
      // A single selected photo is just the focus.
      const selection = count > 1 ? next.selection : {}
      set({ ...next, ...placeFocus(s, s.visibleIds, next.focusId, s.walkGroupId), selection, selectionCount: count > 1 ? count : 0 })
    },
    selectAllVisible: () => {
      const s = get()
      const next = selectAll(s.visibleIds, s.focusId)
      set({ ...next, ...placeFocus(s, s.visibleIds, next.focusId, s.walkGroupId), selectionCount: Object.keys(next.selection).length })
    },
    clearSelection: () => set({ selection: {}, selectionCount: 0 }),

    // ← → leave a walked stack from its slot, to the neighbouring slots.
    move: (dir) => {
      const s = get()
      const burstOf = burstOfState(s)
      const next =
        s.filters.tab === 'duplicates'
          ? moveInRows(
              s.groupRows.map((r) => r.ids),
              s.focusId,
              dir,
              burstOf,
            )
          : moveFocus(s.visibleIds, s.focusId, dir, s.columns, burstOf)
      if (next && next !== (s.focusSlot ?? s.focusId)) moveTo(next, null)
    },

    // The cursor goes through every frame, whatever its decision; a closed
    // stack stays closed and shows the frame under the cursor (G unfolds it).
    moveInGroup: (delta) => {
      const s = get()
      const focusId = s.focusId
      const photo = focusId ? s.photos[focusId] : undefined
      const gid = photo ? groupIdOf(photo) : null
      const group = gid === null ? undefined : s.groups[gid]
      if (!focusId || !group || group.photoIds.length < 2) return
      const frames = burstFrames(group, s, s.filters, s.search.hits)
      const index = frames.indexOf(focusId)
      const next = index === -1 ? frames[0] : frames[index + delta]
      if (next && next !== focusId) moveTo(next, group.id)
    },

    mark: async (patch, options = {}) => {
      const s = get()
      const projectId = s.projectId
      if (!projectId) return
      const ids = options.ids ?? markTargets(s, s.visibleIds)
      if (!ids.length) return
      let finalPatch = patch
      // Pressing a colour label again removes it (Lightroom behaviour).
      if (patch.label && patch.label !== 'none' && ids.every((id) => s.photos[id]?.mark.label === patch.label)) {
        finalPatch = { ...patch, label: 'none' }
      }
      if (finalPatch.flag === 'pick' || finalPatch.flag === 'reject') playSound(finalPatch.flag === 'pick' ? 'keep' : 'reject')
      const changes = [{ ids, patch: finalPatch }]
      const { photos, before } = applyLocal(changes)
      const single = ids.length === 1
      const advance = single && (options.advance ?? usePrefsStore.getState().autoAdvance)
      const prevVisible = s.visibleIds
      const next = { ...s, photos }
      const derived = derive(next)
      const burstOf = burstOfState(next)
      // Walking a closed stack: auto-advance goes to its next frame, then leaves it.
      const walked = single && s.walkGroupId !== null && ids[0] === s.focusId ? s.groups[s.walkGroupId] : undefined
      const walk = walked ? burstFrames(walked, s, s.filters, s.search.hits) : null
      const focusId = single ? focusAfterMark(prevVisible, derived.visibleIds, ids[0], advance, burstOf, walk) : settleFocus(prevVisible, derived.visibleIds, s.focusId, burstOf)
      const flash = single ? { seq: (s.flash?.seq ?? 0) + 1, patch: finalPatch } : s.flash
      update({ photos, undoStack: [...s.undoStack.slice(-49), { projectId, before }], flash }, focusId)
      if (!single) {
        notify({
          title: t('marks.applied', { count: ids.length }),
          action: { label: t('marks.undo'), run: () => void get().undo() },
        })
      }
      await persist(projectId, changes, before)
    },

    choose: async (options = {}) => {
      const s = get()
      const winner = options.id ?? s.focusId
      const photo = winner ? s.photos[winner] : undefined
      if (!winner || !photo || !s.projectId) return
      const gid = groupIdOf(photo)
      const pool = options.pool ?? (gid === null ? [] : (s.groups[gid]?.photoIds ?? []))
      const changes = chooseChanges(s.photos, winner, { seen: options.seen, pool })
      const rejected = changes[1]?.ids ?? []
      // Nothing competes with it: a plain keep.
      if (!rejected.length) {
        await get().mark({ flag: 'pick' }, { ids: [winner], advance: options.advance })
        return
      }
      // The burst is decided: auto-advance leaves it.
      const advance = options.advance ?? usePrefsStore.getState().autoAdvance
      const after = derive({ ...s, photos: applyLocal(changes).photos }).visibleIds
      const focusId = advance ? focusAfterMark(s.visibleIds, after, winner, true, burstOfState(s)) : winner
      const pending = get().markMany(changes, focusId)
      notify({
        title: t('marks.chosen', { name: photo.fileName }),
        description: t('marks.othersRejected', { count: rejected.length, name: s.photos[rejected[0]]?.fileName ?? '' }),
        tone: 'success',
        action: { label: t('marks.undo'), run: () => void get().undo() },
      })
      await pending
    },

    markMany: async (changes, focusId) => {
      const s = get()
      const projectId = s.projectId
      if (!projectId) return
      // Keeping the best of a burst rejects the others: one "keep" says it.
      const flags = changes.map((c) => c.patch.flag)
      if (flags.includes('pick')) playSound('keep')
      else if (flags.includes('reject')) playSound('reject')
      const { photos, before } = applyLocal(changes)
      update({ photos, undoStack: [...s.undoStack.slice(-49), { projectId, before }] }, focusId === undefined ? undefined : focusId)
      await persist(projectId, changes, before)
    },

    undo: async () => {
      const s = get()
      const entry = s.undoStack[s.undoStack.length - 1]
      if (!entry || entry.projectId !== s.projectId) return
      // Group photos sharing the same previous mark: one engine call per value.
      const byMark = new Map<string, { ids: string[]; patch: UserMark }>()
      for (const [id, mark] of Object.entries(entry.before)) {
        const key = JSON.stringify(mark)
        const bucket = byMark.get(key) ?? { ids: [], patch: mark }
        bucket.ids.push(id)
        byMark.set(key, bucket)
      }
      const changes = [...byMark.values()]
      const { photos } = applyLocal(changes)
      const firstId = Object.keys(entry.before)[0]
      update({ photos, undoStack: s.undoStack.slice(0, -1) }, firstId ?? s.focusId)
      notify({ title: t('marks.undone'), timeout: 2000 })
      try {
        await Promise.all(changes.map((c) => api().setMarks(entry.projectId, c.ids, c.patch)))
      } catch {
        notify({ title: t('marks.failed'), tone: 'danger' })
      }
    },

    runSearch: async (query) => {
      const s = get()
      const projectId = s.projectId
      const q = query.trim()
      if (!projectId) return
      if (!q) {
        get().clearSearch()
        return
      }
      set({ search: { ...s.search, query: q, status: 'loading' } })
      try {
        const results = await api().search(projectId, q)
        if (get().projectId !== projectId || get().search.query !== q) return
        const hits: Record<string, number> = {}
        results.forEach((h) => (hits[h.photoId] = h.score))
        const cur = get()
        const previousSort = cur.search.hits ? cur.search.previousSort : cur.filters.sort
        update({
          search: { query: q, status: 'done', hits, previousSort },
          filters: { ...cur.filters, sort: 'relevance' },
        })
        // Results only in other tabs (the review opens on "to review"): show them all.
        const counts = get().tabCounts
        if (counts[get().filters.tab] === 0 && counts.all > 0) update({ filters: { ...get().filters, tab: 'all' } })
      } catch (error) {
        // Plus went away (released, refunded): say what AI search needs.
        if (toCullError(error).code === 'plusRequired') {
          set({ search: EMPTY_SEARCH })
          usePlusStore.getState().openDialog('search')
          return
        }
        set({ search: { ...get().search, status: 'error' } })
      }
    },

    clearSearch: () => {
      const s = get()
      const sort = s.filters.sort === 'relevance' ? s.search.previousSort : s.filters.sort
      update({ search: EMPTY_SEARCH, filters: { ...s.filters, sort } })
    },

    recompute: async (settings) => {
      const s = get()
      const projectId = s.projectId
      if (!projectId) return null
      const before = s.counts()
      set({ recomputing: true })
      try {
        const data = await api().recompute(projectId, settings)
        if (get().projectId !== projectId) return null
        get().load(data, { keepView: true })
        return { before, after: get().counts() }
      } finally {
        set({ recomputing: false })
      }
    },

    counts: () => countDecisions(Object.values(get().photos).filter((p) => p.status === 'analyzed')),
  }
})
