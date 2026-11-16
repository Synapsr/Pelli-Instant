import { describe, expect, it } from 'vitest'
import { data, photo } from '@/test/fixtures'
import { applyMarkPatch, finalDecision } from './decision'
import { burstFrames, computeGroupRows, computeReasonCounts, computeTabCounts, computeVisible, DEFAULT_FILTERS, framePosition, groupOrder, isBest, sameIds, stackCover, type ViewFilters } from './view'

const burst = { id: 1, kind: 'burst' as const, photoIds: ['p3', 'p2', 'p4'] }

function fixture() {
  return data(
    [
      photo('p1', { verdict: 'keep', score: 80, sceneId: 1 }),
      photo('p2', { verdict: 'reject', reasons: ['duplicate'], score: 70, groupId: 1, sceneId: 1 }),
      photo('p3', { verdict: 'keep', score: 90, groupId: 1, sceneId: 1 }),
      photo('p4', { verdict: 'reject', reasons: ['duplicate', 'eyesClosed'], score: 40, groupId: 1, sceneId: 2 }),
      photo('p5', { verdict: 'maybe', reasons: ['lowAesthetic'], score: 55, sceneId: 2, camera: 'Canon EOS R6 Mark II' }),
      photo('p6', { verdict: 'reject', reasons: ['blurry'], score: 20, sceneId: 2 }),
      photo('p7', { pending: true }),
    ],
    [burst],
  )
}

const filters = (patch: Partial<ViewFilters> = {}): ViewFilters => ({ ...DEFAULT_FILTERS, ...patch })

describe('final decision', () => {
  it('lets the photographer flag win over the AI verdict', () => {
    expect(finalDecision('reject', { flag: 'pick', rating: 0, label: 'none' })).toBe('selected')
    expect(finalDecision('keep', { flag: 'reject', rating: 0, label: 'none' })).toBe('rejected')
    expect(finalDecision('maybe', { flag: 'none', rating: 3, label: 'red' })).toBe('toReview')
    expect(finalDecision(null, { flag: 'none', rating: 0, label: 'none' })).toBe('toReview')
  })

  it('recomputes the decision when a mark is applied', () => {
    const p = applyMarkPatch(photo('p1', { verdict: 'reject' }), { flag: 'pick' })
    expect(p.decision).toBe('selected')
    expect(applyMarkPatch(p, { flag: 'none' }).decision).toBe('rejected')
  })
})

describe('visible photos', () => {
  it('stacks a group behind its best visible member, at the place of its first frame', () => {
    expect(computeVisible(fixture(), filters(), null)).toEqual(['p1', 'p3', 'p5', 'p6', 'p7'])
  })

  it('expands a group in the order of the sort: capture order by date, best first by score', () => {
    expect(computeVisible(fixture(), filters({ expanded: { 1: true } }), null)).toEqual(['p1', 'p2', 'p3', 'p4', 'p5', 'p6', 'p7'])
    expect(computeVisible(fixture(), filters({ sort: 'score', expanded: { 1: true } }), null)).toEqual(['p3', 'p2', 'p4', 'p1', 'p5', 'p6', 'p7'])
  })

  it('shows every frame when stacking is off', () => {
    expect(computeVisible(fixture(), filters({ stackGroups: false }), null)).toEqual(['p1', 'p2', 'p3', 'p4', 'p5', 'p6', 'p7'])
  })

  it('filters by decision tab and keeps pending photos out of decided tabs', () => {
    expect(computeVisible(fixture(), filters({ tab: 'selected' }), null)).toEqual(['p1', 'p3'])
    // Rejected members of a stacked group collapse behind the best rejected one.
    expect(computeVisible(fixture(), filters({ tab: 'rejected' }), null)).toEqual(['p2', 'p6'])
    expect(computeVisible(fixture(), filters({ tab: 'toReview' }), null)).toEqual(['p5'])
  })

  it('shows the whole burst when a group is expanded, whatever the decision tab', () => {
    // Every frame within the scope, in capture order, at the place of the group.
    expect(computeVisible(fixture(), filters({ tab: 'selected', expanded: { 1: true } }), null)).toEqual(['p1', 'p2', 'p3', 'p4'])
    expect(computeVisible(fixture(), filters({ tab: 'rejected', expanded: { 1: true } }), null)).toEqual(['p2', 'p3', 'p4', 'p6'])
    // The scope still applies: p4 is in another moment.
    expect(computeVisible(fixture(), filters({ tab: 'selected', sceneId: 1, expanded: { 1: true } }), null)).toEqual(['p1', 'p2', 'p3'])
    // A group with no frame in the tab stays out of it.
    expect(computeVisible(fixture(), filters({ tab: 'toReview', expanded: { 1: true } }), null)).toEqual(['p5'])
  })

  it('filters by reason (duplicates include exact duplicates), scene and camera', () => {
    expect(computeVisible(fixture(), filters({ reason: 'eyesClosed', stackGroups: false }), null)).toEqual(['p4'])
    expect(computeVisible(fixture(), filters({ reason: 'duplicate', stackGroups: false }), null)).toEqual(['p2', 'p4'])
    expect(computeVisible(fixture(), filters({ sceneId: 2, stackGroups: false }), null)).toEqual(['p4', 'p5', 'p6'])
    expect(computeVisible(fixture(), filters({ camera: 'Canon EOS R6 Mark II' }), null)).toEqual(['p5'])
  })

  it('restricts to search hits ordered by relevance', () => {
    const hits = { p6: 0.31, p1: 0.22, p5: 0.27 }
    expect(computeVisible(fixture(), filters({ sort: 'relevance' }), hits)).toEqual(['p6', 'p5', 'p1'])
  })

  it('sorts by score, pending photos last', () => {
    expect(computeVisible(fixture(), filters({ sort: 'score', stackGroups: false }), null)).toEqual(['p3', 'p1', 'p2', 'p5', 'p4', 'p6', 'p7'])
  })

  it('builds the duplicates rows and counts groups in the tab', () => {
    const rows = computeGroupRows(fixture(), filters({ tab: 'duplicates' }), null)
    expect(rows).toHaveLength(1)
    expect(rows[0].ids).toEqual(['p2', 'p3', 'p4'])
    expect(computeVisible(fixture(), filters({ tab: 'duplicates' }), null)).toEqual(['p2', 'p3', 'p4'])
    // Sorted by score, the rows put the best first.
    expect(computeGroupRows(fixture(), filters({ tab: 'duplicates', sort: 'score' }), null)[0].ids).toEqual(['p3', 'p2', 'p4'])
  })

  it('covers a stack with the photographer’s kept photo, else the best selected one, else the best', () => {
    const burst3 = [{ id: 1, kind: 'burst' as const, photoIds: ['p1', 'p2', 'p3'] }]
    const frames = [
      photo('p1', { verdict: 'reject', reasons: ['eyesClosed'], score: 90, groupId: 1 }),
      photo('p2', { verdict: 'keep', score: 70, groupId: 1 }),
      photo('p3', { verdict: 'reject', reasons: ['duplicate'], score: 60, groupId: 1 }),
    ]
    // The AI's best has its eyes closed (rejected): the next best, selected, covers the stack in "all".
    expect(computeVisible(data(frames, burst3), filters(), null)).toEqual(['p2'])
    // The photographer kept p3: it covers the stack, wherever its rank.
    const kept = data([frames[0], frames[1], applyMarkPatch(frames[2], { flag: 'pick' })], burst3)
    expect(computeVisible(kept, filters(), null)).toEqual(['p3'])
    // Only photos of the tab can cover it: the best rejected one in "rejected".
    expect(computeVisible(kept, filters({ tab: 'rejected' }), null)).toEqual(['p1'])
    // Nothing selected: the best.
    const none = data([photo('p1', { verdict: 'reject', score: 90, groupId: 1 }), photo('p2', { verdict: 'maybe', score: 70, groupId: 1 })], [{ id: 1, kind: 'burst', photoIds: ['p1', 'p2'] }])
    expect(stackCover(none.groups[1], none.photos, () => true)).toBe('p1')
  })

  it('orders a burst like the grid, the best staying the best', () => {
    // Names that follow neither the capture order nor the rank.
    const named = (id: string, fileName: string, score: number) => ({ ...photo(id, { score, groupId: 1 }), fileName })
    const d = data([named('p1', 'IMG_0200.CR3', 40), named('p2', 'IMG_0300.CR3', 90), named('p3', 'IMG_0100.CR3', 60)], [{ id: 1, kind: 'burst', photoIds: ['p2', 'p3', 'p1'] }])
    const group = d.groups[1]
    expect(groupOrder(group, 'date', d)).toEqual(['p1', 'p2', 'p3'])
    expect(groupOrder(group, 'score', d)).toEqual(['p2', 'p3', 'p1'])
    expect(groupOrder(group, 'relevance', d)).toEqual(['p2', 'p3', 'p1'])
    expect(groupOrder(group, 'name', d)).toEqual(['p3', 'p1', 'p2'])
    // Position is not rank: p2 is second in capture order and still the best.
    expect(framePosition(group, 'p2', 'date', null, d)).toBe(1)
    expect(framePosition(group, 'p2', 'relevance', null, d)).toBe(1) // relevance without a search: the date
    expect(isBest(group, 'p2')).toBe(true)
    expect(isBest(group, 'p1')).toBe(false)
  })

  it('walks every frame of a burst within the scope, whatever its decision', () => {
    expect(burstFrames(fixture().groups[1], fixture(), filters({ tab: 'selected' }), null)).toEqual(['p2', 'p3', 'p4'])
    expect(burstFrames(fixture().groups[1], fixture(), filters({ sceneId: 1 }), null)).toEqual(['p2', 'p3'])
    expect(burstFrames(fixture().groups[1], fixture(), filters({ sort: 'score' }), null)).toEqual(['p3', 'p2', 'p4'])
  })

  it('counts tabs within the sidebar scope', () => {
    expect(computeTabCounts(fixture(), filters(), null)).toEqual({ selected: 2, toReview: 1, rejected: 3, all: 7, duplicates: 1 })
    expect(computeTabCounts(fixture(), filters({ sceneId: 2 }), null)).toEqual({ selected: 0, toReview: 1, rejected: 2, all: 3, duplicates: 1 })
  })

  it('counts reasons once per photo', () => {
    const counts = computeReasonCounts(Object.values(fixture().photos))
    expect(counts).toEqual({ duplicate: 2, eyesClosed: 1, lowAesthetic: 1, blurry: 1 })
  })

  it('compares id lists by content', () => {
    expect(sameIds(['a', 'b'], ['a', 'b'])).toBe(true)
    expect(sameIds(['a', 'b'], ['b', 'a'])).toBe(false)
  })
})
