import { beforeEach, describe, expect, it } from 'vitest'
import { DEFAULT_SETTINGS, type CullApi } from '@/lib/contract'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { pickCompareWinner } from '@/hooks/use-workspace-keyboard'
import { setNotifier, type ToastInput } from '@/lib/notify'
import { burstFrames, groupOrder } from '@/lib/review/view'
import { usePrefsStore } from './prefs-store'
import { useReviewStore } from './review-store'
import { useWorkspaceStore } from './workspace-store'

const PROJECT = 'prj-lea-thomas'

async function setup(override: Partial<CullApi> = {}, params = 'mockPlus=active') {
  const mock = createMockApi({ thumbnails: false, storage: null, latency: 0, params: new URLSearchParams(params) })
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
    // Sorted by date (the default), the burst unfolds in capture order.
    const order = groupOrder(group, 'date', s)
    const start = after.indexOf(order[0])
    expect(after.slice(start, start + group.photoIds.length)).toEqual(order)
  })

  it('goes through every frame of a closed burst, whatever their decision, without unstacking it', async () => {
    await setup()
    useReviewStore.getState().setTab('selected')
    const s = useReviewStore.getState()
    const group = s.groupList.find((g) => g.photoIds.length >= 3 && s.photos[g.photoIds[0]].decision === 'selected' && g.photoIds.some((id) => s.photos[id].decision !== 'selected'))
    expect(group).toBeDefined()
    const frames = burstFrames(group!, s, s.filters, null)
    const cover = s.visibleIds.find((id) => group!.photoIds.includes(id)) as string
    const visible = s.visibleIds
    useReviewStore.getState().focus(cover)
    // To the first frame, then through all of them.
    for (let i = 0; i < frames.length; i++) useReviewStore.getState().moveInGroup(-1)
    expect(useReviewStore.getState().focusId).toBe(frames[0])
    const seen = [useReviewStore.getState().focusId]
    for (let i = 1; i < frames.length; i++) {
      useReviewStore.getState().moveInGroup(1)
      const now = useReviewStore.getState()
      seen.push(now.focusId)
      // The stack stays closed, in its slot, showing the frame under the cursor.
      expect(now.visibleIds).toBe(visible)
      expect(now.filters.expanded[group!.id]).toBeUndefined()
      expect(now.focusSlot).toBe(cover)
      expect(now.walkGroupId).toBe(group!.id)
    }
    expect(seen).toEqual(frames)
    // Hidden frames of other decisions are walked too.
    expect(frames.some((id) => !visible.includes(id))).toBe(true)
    useReviewStore.getState().moveInGroup(1) // last frame: stays
    expect(useReviewStore.getState().focusId).toBe(frames[frames.length - 1])
  })

  it('goes on from the stack’s slot after walking it or clicking a frame of the strip', async () => {
    await setup()
    const s = useReviewStore.getState()
    const group = s.groupList.find((g) => g.photoIds.length >= 3) as NonNullable<(typeof s.groupList)[number]>
    const slotIndex = s.visibleIds.findIndex((id) => group.photoIds.includes(id))
    const hidden = group.photoIds.find((id) => !s.visibleIds.includes(id)) as string
    // The inspector's strip focuses a hidden frame: ← / → leave from the stack's slot.
    useReviewStore.getState().focus(hidden)
    expect(useReviewStore.getState()).toMatchObject({ focusSlot: s.visibleIds[slotIndex], walkGroupId: group.id })
    useReviewStore.getState().move('right')
    expect(useReviewStore.getState()).toMatchObject({ focusId: s.visibleIds[slotIndex + 1], walkGroupId: null })
    useReviewStore.getState().focus(hidden)
    useReviewStore.getState().move('left')
    expect(useReviewStore.getState().focusId).toBe(s.visibleIds[slotIndex - 1])
    // G still unfolds the burst: the frame becomes a photo of the grid, the walk ends.
    useReviewStore.getState().focus(hidden)
    useReviewStore.getState().toggleGroup(group.id)
    expect(useReviewStore.getState()).toMatchObject({ focusId: hidden, focusSlot: hidden, walkGroupId: null })
  })

  it('marks the frame under the cursor, advances through the burst, then leaves it; undo comes back to it', async () => {
    await setup()
    const s = useReviewStore.getState()
    // A burst whose first frame in capture order is hidden behind another cover.
    const group = s.groupList.find((g) => {
      const frames = burstFrames(g, s, s.filters, null)
      return frames.length >= 3 && !s.visibleIds.includes(frames[0])
    }) as NonNullable<(typeof s.groupList)[number]>
    const frames = burstFrames(group, s, s.filters, null)
    const slotIndex = s.visibleIds.findIndex((id) => group.photoIds.includes(id))
    const next = s.visibleIds[slotIndex + 1]
    useReviewStore.getState().focus(frames[0])
    for (let i = 0; i < frames.length; i++) {
      expect(useReviewStore.getState().focusId).toBe(frames[i])
      await useReviewStore.getState().mark({ flag: 'reject' })
      expect(useReviewStore.getState().photos[frames[i]].mark.flag).toBe('reject')
    }
    // Every frame marked: the cursor left the stack for the next slot.
    expect(useReviewStore.getState()).toMatchObject({ focusId: next, walkGroupId: null })
    // Undo puts the last frame back under the cursor, in its stack's slot.
    await useReviewStore.getState().undo()
    const now = useReviewStore.getState()
    expect(now.focusId).toBe(frames[frames.length - 1])
    expect(now.visibleIds[now.visibleIds.indexOf(now.focusSlot as string) + 1]).toBe(next)
    useReviewStore.getState().move('right')
    expect(useReviewStore.getState().focusId).toBe(next)
  })

  it('chooses a frame of a burst: keeps it, rejects the other selected ones, leaves the to-review ones, one undo', async () => {
    await setup()
    const toasts: ToastInput[] = []
    setNotifier((toast) => toasts.push(toast))
    const s = useReviewStore.getState()
    const group = s.groupList.find((g) => g.photoIds.length >= 3 && s.photos[g.photoIds[0]].decision === 'selected' && g.photoIds.some((id) => s.photos[id].decision !== 'selected'))
    expect(group).toBeDefined()
    const ids = group!.photoIds
    const winner = ids.find((id) => s.photos[id].decision !== 'selected') as string
    const rivals = ids.filter((id) => id !== winner && s.photos[id].decision === 'selected')
    const untouched = ids.filter((id) => id !== winner && s.photos[id].decision !== 'selected')
    const before = Object.fromEntries(ids.map((id) => [id, s.photos[id].mark]))
    const undoDepth = s.undoStack.length
    await useReviewStore.getState().choose({ id: winner, advance: false })
    let now = useReviewStore.getState()
    expect(now.photos[winner].mark.flag).toBe('pick')
    for (const id of rivals) expect(now.photos[id].mark.flag).toBe('reject')
    for (const id of untouched) expect(now.photos[id].mark).toEqual(before[id])
    // The photographer's choice now covers the stack.
    expect(now.visibleIds).toContain(winner)
    expect(now.undoStack).toHaveLength(undoDepth + 1)
    expect(toasts.at(-1)?.title).toContain(s.photos[winner].fileName)
    await useReviewStore.getState().undo()
    now = useReviewStore.getState()
    expect(Object.fromEntries(ids.map((id) => [id, now.photos[id].mark]))).toEqual(before)
    setNotifier(() => {})
  })

  it('chooses in the compare view with the same rule: the page shown, the selected photos of the others', async () => {
    await setup()
    const s = useReviewStore.getState()
    const pick = (decision: string, not: string[]) => s.visibleIds.find((id) => s.photos[id].decision === decision && !not.includes(id)) as string
    const page = s.visibleIds.slice(0, 4)
    const selected = pick('selected', page)
    const toReview = pick('toReview', page)
    useWorkspaceStore.getState().reset()
    useWorkspaceStore.getState().openCompare([...page, selected, toReview])
    pickCompareWinner(1)
    await flush()
    const now = useReviewStore.getState()
    expect(now.photos[page[1]].mark.flag).toBe('pick')
    for (const id of [page[0], page[2], page[3], selected]) expect(now.photos[id].decision).toBe('rejected')
    // Never shown: stays to review.
    expect(now.photos[toReview].decision).toBe('toReview')
    expect(useWorkspaceStore.getState().mode).toBe('grid')
    await useReviewStore.getState().undo()
    expect(useReviewStore.getState().photos[selected].decision).toBe('selected')
  })

  it('flashes the mark of a single photo, not of a multi-selection', async () => {
    await setup()
    const store = useReviewStore.getState()
    await store.mark({ flag: 'pick' }, { advance: false })
    expect(useReviewStore.getState().flash?.patch).toEqual({ flag: 'pick' })
    const seq = useReviewStore.getState().flash?.seq ?? 0
    await useReviewStore.getState().mark({ label: 'red' }, { advance: false })
    await useReviewStore.getState().mark({ label: 'red' }, { advance: false })
    // Pressed again, the label comes off: the flash says so.
    expect(useReviewStore.getState().flash).toEqual({ seq: seq + 2, patch: { label: 'none' } })
    const [a, b] = useReviewStore.getState().visibleIds
    useReviewStore.getState().click(a, { range: false, toggle: false })
    useReviewStore.getState().click(b, { range: false, toggle: true })
    await useReviewStore.getState().mark({ rating: 3 })
    expect(useReviewStore.getState().flash?.seq).toBe(seq + 2)
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
