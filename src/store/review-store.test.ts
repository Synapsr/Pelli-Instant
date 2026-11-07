import { beforeEach, describe, expect, it } from 'vitest'
import { DEFAULT_SETTINGS, type CullApi } from '@/lib/contract'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { usePrefsStore } from './prefs-store'
import { useReviewStore } from './review-store'

const PROJECT = 'prj-lea-thomas'

async function setup(override: Partial<CullApi> = {}) {
  const mock = createMockApi({ thumbnails: false, storage: null, latency: 0, params: new URLSearchParams() })
  setApi({ ...mock, ...override })
  const data = await mock.getProject(PROJECT)
  useReviewStore.getState().reset()
  useReviewStore.getState().load(data)
  return { mock, data }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 5))

describe('review store', () => {
  beforeEach(() => {
    usePrefsStore.setState({ autoAdvance: true })
  })

  it('loads a project with the default view and focuses the first photo', async () => {
    const { data } = await setup()
    const s = useReviewStore.getState()
    expect(s.projectId).toBe(PROJECT)
    expect(s.dateOrder).toHaveLength(data.photos.length)
    expect(s.filters.tab).toBe('all')
    expect(s.focusId).toBe(s.visibleIds[0])
    expect(s.tabCounts.all).toBe(data.photos.length)
    expect(s.visibleIds.length).toBeLessThan(data.photos.length) // bursts are stacked
  })

  it('marks optimistically, persists, and auto-advances in a filtered tab', async () => {
    const { mock } = await setup()
    const store = useReviewStore.getState()
    store.setTab('toReview')
    const before = useReviewStore.getState().visibleIds
    const target = before[0]
    useReviewStore.getState().focus(target)
    const pending = useReviewStore.getState().mark({ flag: 'pick' })
    // Optimistic: applied before the engine answers.
    const s = useReviewStore.getState()
    expect(s.photos[target].decision).toBe('selected')
    expect(s.visibleIds).not.toContain(target)
    expect(s.focusId).toBe(before[1])
    await pending
    const fresh = await mock.getProject(PROJECT)
    expect(fresh.photos.find((p) => p.id === target)?.mark.flag).toBe('pick')
  })

  it('keeps the visible array identity when a mark does not change membership', async () => {
    await setup()
    const before = useReviewStore.getState().visibleIds
    await useReviewStore.getState().mark({ rating: 4 }, { advance: false })
    expect(useReviewStore.getState().visibleIds).toBe(before)
  })

  it('rolls back when the engine refuses the mark', async () => {
    await setup({ setMarks: () => Promise.reject(new Error('disk full')) })
    const id = useReviewStore.getState().focusId as string
    const initial = useReviewStore.getState().photos[id].mark
    await useReviewStore.getState().mark({ flag: 'reject' })
    await flush()
    expect(useReviewStore.getState().photos[id].mark).toEqual(initial)
    expect(useReviewStore.getState().undoStack).toHaveLength(0)
  })

  it('toggles a colour label off when pressed again', async () => {
    await setup()
    const id = useReviewStore.getState().focusId as string
    await useReviewStore.getState().mark({ label: 'red' }, { advance: false })
    expect(useReviewStore.getState().photos[id].mark.label).toBe('red')
    await useReviewStore.getState().mark({ label: 'red' }, { advance: false })
    expect(useReviewStore.getState().photos[id].mark.label).toBe('none')
  })

  it('applies marks to the multi-selection and undoes them in one step', async () => {
    await setup()
    const store = useReviewStore.getState()
    const [a, , c] = store.visibleIds
    store.click(a, { range: false, toggle: false })
    useReviewStore.getState().click(c, { range: true, toggle: false })
    expect(useReviewStore.getState().selectionCount).toBe(3)
    const marksBefore = [a, c].map((id) => useReviewStore.getState().photos[id].mark)
    await useReviewStore.getState().mark({ flag: 'reject' })
    expect(useReviewStore.getState().photos[a].mark.flag).toBe('reject')
    expect(useReviewStore.getState().photos[c].mark.flag).toBe('reject')
    await useReviewStore.getState().undo()
    expect([a, c].map((id) => useReviewStore.getState().photos[id].mark)).toEqual(marksBefore)
  })

  it('navigates by rows using the grid column count', async () => {
    await setup()
    const store = useReviewStore.getState()
    store.setColumns(4)
    const ids = useReviewStore.getState().visibleIds
    useReviewStore.getState().focus(ids[1])
    useReviewStore.getState().move('down')
    expect(useReviewStore.getState().focusId).toBe(ids[5])
    useReviewStore.getState().move('left')
    expect(useReviewStore.getState().focusId).toBe(ids[4])
  })

  it('expands a stacked group inline', async () => {
    await setup()
    const s = useReviewStore.getState()
    const group = s.groupList.find((g) => g.photoIds.length >= 3) as NonNullable<(typeof s.groupList)[number]>
    const before = s.visibleIds.length
    s.toggleGroup(group.id)
    const after = useReviewStore.getState().visibleIds
    expect(after.length).toBe(before + group.photoIds.length - 1)
    const start = after.indexOf(group.photoIds[0])
    expect(after.slice(start, start + group.photoIds.length)).toEqual(group.photoIds)
  })

  it('searches with relevance order and restores the sort when cleared', async () => {
    await setup()
    await useReviewStore.getState().runSearch('mariée bouquet')
    const s = useReviewStore.getState()
    expect(s.search.status).toBe('done')
    expect(s.filters.sort).toBe('relevance')
    expect(s.visibleIds.length).toBeGreaterThan(0)
    expect(s.visibleIds.every((id) => s.search.hits?.[id] !== undefined)).toBe(true)
    s.clearSearch()
    expect(useReviewStore.getState().filters.sort).toBe('date')
    expect(useReviewStore.getState().search.hits).toBeNull()
  })

  it('recomputes with a new sensitivity, keeping the photographer marks', async () => {
    await setup()
    const id = useReviewStore.getState().focusId as string
    await useReviewStore.getState().mark({ flag: 'pick' }, { advance: false })
    const strict = await useReviewStore.getState().recompute({ ...DEFAULT_SETTINGS, sensitivity: 'strict' })
    const lenient = await useReviewStore.getState().recompute({ ...DEFAULT_SETTINGS, sensitivity: 'lenient' })
    expect(strict && lenient).toBeTruthy()
    expect(lenient!.after.rejected).toBeLessThan(strict!.after.rejected)
    expect(useReviewStore.getState().photos[id].decision).toBe('selected')
  })

  it('upserts streamed photos without losing the view', async () => {
    await setup()
    useReviewStore.getState().setTab('rejected')
    const id = useReviewStore.getState().visibleIds[0]
    const p = useReviewStore.getState().photos[id]
    useReviewStore.getState().upsertPhotos([{ ...p, thumbUrl: 'blob:thumb' }])
    expect(useReviewStore.getState().photos[id].thumbUrl).toBe('blob:thumb')
    expect(useReviewStore.getState().filters.tab).toBe('rejected')
  })
})
