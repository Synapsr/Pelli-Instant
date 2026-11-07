import { describe, expect, it } from 'vitest'
import { data, photo } from '@/test/fixtures'
import { applyMarkPatch, finalDecision } from './decision'
import { computeGroupRows, computeReasonCounts, computeTabCounts, computeVisible, DEFAULT_FILTERS, sameIds, type ViewFilters } from './view'

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

  it('expands a group in rank order', () => {
    expect(computeVisible(fixture(), filters({ expanded: { 1: true } }), null)).toEqual(['p1', 'p3', 'p2', 'p4', 'p5', 'p6', 'p7'])
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
    // Every frame within the scope, best first, at the place of the group.
    expect(computeVisible(fixture(), filters({ tab: 'selected', expanded: { 1: true } }), null)).toEqual(['p1', 'p3', 'p2', 'p4'])
    expect(computeVisible(fixture(), filters({ tab: 'rejected', expanded: { 1: true } }), null)).toEqual(['p3', 'p2', 'p4', 'p6'])
    // The scope still applies: p4 is in another moment.
    expect(computeVisible(fixture(), filters({ tab: 'selected', sceneId: 1, expanded: { 1: true } }), null)).toEqual(['p1', 'p3', 'p2'])
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
    expect(rows[0].ids).toEqual(['p3', 'p2', 'p4'])
    expect(computeVisible(fixture(), filters({ tab: 'duplicates' }), null)).toEqual(['p3', 'p2', 'p4'])
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
