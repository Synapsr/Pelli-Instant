import { describe, expect, it } from 'vitest'
import { photo } from '@/test/fixtures'
import { applyMarkPatch, chooseChanges } from './decision'
import { clickSelection, focusAfterMark, markTargets, moveFocus, moveInRows, selectAll, slotOf, type BurstOf } from './selection'

const visible = ['a', 'b', 'c', 'd', 'e', 'f', 'g']
const empty = { selection: {}, anchorId: null, focusId: null }

describe('selection', () => {
  it('selects a single photo on plain click', () => {
    expect(clickSelection(empty, 'c', { range: false, toggle: false }, visible)).toEqual({ selection: { c: true }, anchorId: 'c', focusId: 'c' })
  })

  it('selects a range from the anchor with shift', () => {
    const start = clickSelection(empty, 'b', { range: false, toggle: false }, visible)
    const range = clickSelection(start, 'e', { range: true, toggle: false }, visible)
    expect(Object.keys(range.selection)).toEqual(['b', 'c', 'd', 'e'])
    expect(range.anchorId).toBe('b')
    expect(range.focusId).toBe('e')
    const back = clickSelection(range, 'a', { range: true, toggle: false }, visible)
    expect(Object.keys(back.selection)).toEqual(['a', 'b'])
  })

  it('toggles with cmd/ctrl, keeping the implicit focused photo', () => {
    const focused = { selection: {}, anchorId: 'b', focusId: 'b' }
    const two = clickSelection(focused, 'd', { range: false, toggle: true }, visible)
    expect(Object.keys(two.selection).sort()).toEqual(['b', 'd'])
    const one = clickSelection(two, 'b', { range: false, toggle: true }, visible)
    expect(Object.keys(one.selection)).toEqual(['d'])
  })

  it('selects everything visible', () => {
    expect(Object.keys(selectAll(visible, 'c').selection)).toHaveLength(7)
  })

  it('marks the multi-selection in visible order, else the focus', () => {
    expect(markTargets({ selection: { e: true, b: true }, anchorId: 'b', focusId: 'e' }, visible)).toEqual(['b', 'e'])
    expect(markTargets({ selection: {}, anchorId: null, focusId: 'c' }, visible)).toEqual(['c'])
    expect(markTargets(empty, visible)).toEqual([])
  })
})

describe('keyboard navigation', () => {
  it('moves left/right and by rows, clamped', () => {
    expect(moveFocus(visible, 'c', 'right', 3)).toBe('d')
    expect(moveFocus(visible, 'c', 'left', 3)).toBe('b')
    expect(moveFocus(visible, 'b', 'down', 3)).toBe('e')
    expect(moveFocus(visible, 'f', 'down', 3)).toBe('g')
    expect(moveFocus(visible, 'b', 'up', 3)).toBe('b')
    expect(moveFocus(visible, 'a', 'left', 3)).toBe('a')
    expect(moveFocus(visible, null, 'right', 3)).toBe('a')
    expect(moveFocus([], 'a', 'right', 3)).toBeNull()
  })

  it('moves across rows of unequal length (duplicates view)', () => {
    const rows = [['a', 'b', 'c'], ['d', 'e'], ['f', 'g', 'h', 'i']]
    expect(moveInRows(rows, 'c', 'right')).toBe('d')
    expect(moveInRows(rows, 'd', 'left')).toBe('c')
    expect(moveInRows(rows, 'c', 'down')).toBe('e')
    expect(moveInRows(rows, 'e', 'down')).toBe('g')
    expect(moveInRows(rows, 'a', 'up')).toBe('a')
  })

  it('advances to the next photo after a mark, even when the photo leaves the list', () => {
    // "À vérifier": the marked photo disappears, the next one takes its place.
    expect(focusAfterMark(['a', 'b', 'c'], ['a', 'c'], 'b', true)).toBe('c')
    // "Toutes": the photo stays, focus moves on.
    expect(focusAfterMark(['a', 'b', 'c'], ['a', 'b', 'c'], 'b', true)).toBe('c')
    // Last photo: stay on it.
    expect(focusAfterMark(['a', 'b', 'c'], ['a', 'b', 'c'], 'c', true)).toBe('c')
    // No auto-advance: keep the photo, or its former slot when it left.
    expect(focusAfterMark(['a', 'b', 'c'], ['a', 'b', 'c'], 'b', false)).toBe('b')
    expect(focusAfterMark(['a', 'b', 'c'], ['a', 'c'], 'b', false)).toBe('c')
    expect(focusAfterMark(['a'], [], 'a', true)).toBeNull()
  })
})

/** a b [c] d e: c is the cover of a closed stack hiding x and y (the burst is x, c, y). */
const shown = ['a', 'b', 'c', 'd', 'e']
const burst: BurstOf = (id) => (['x', 'c', 'y'].includes(id) ? ['x', 'c', 'y'] : undefined)

describe('slots (a frame walked inside a closed stack)', () => {
  it('maps a hidden frame to its stack’s slot', () => {
    expect(slotOf('b', shown, burst)).toBe('b')
    expect(slotOf('x', shown, burst)).toBe('c')
    expect(slotOf('y', shown, burst)).toBe('c')
    expect(slotOf('z', shown, burst)).toBeNull()
    expect(slotOf(null, shown, burst)).toBeNull()
    // Unstacked, several frames shown: the first one in the list.
    expect(slotOf('y', ['x', 'a', 'c'], burst)).toBe('x')
  })

  it('moves from the stack’s slot, not from the first photo of the shoot', () => {
    expect(moveFocus(shown, 'x', 'right', 5, burst)).toBe('d')
    expect(moveFocus(shown, 'y', 'left', 5, burst)).toBe('b')
    expect(moveFocus(shown, 'x', 'down', 2, burst)).toBe('e')
    // Without bursts, an unknown focus still starts at the beginning.
    expect(moveFocus(shown, 'x', 'right', 5)).toBe('a')
    expect(moveInRows([['a', 'b'], ['c', 'd']], 'x', 'right', burst)).toBe('d')
  })

  it('advances through the walked burst, then leaves the stack', () => {
    const walk = ['x', 'c', 'y']
    expect(focusAfterMark(shown, shown, 'x', true, burst, walk)).toBe('c')
    expect(focusAfterMark(shown, shown, 'c', true, burst, walk)).toBe('y')
    // Last frame: the slot after the stack.
    expect(focusAfterMark(shown, shown, 'y', true, burst, walk)).toBe('d')
    // The stack left the tab (its cover was its only selected frame): the next slot.
    expect(focusAfterMark(shown, ['a', 'b', 'd', 'e'], 'c', true, burst, walk)).toBe('d')
    // Without auto-advance the cursor stays on the frame, even hidden.
    expect(focusAfterMark(shown, shown, 'x', false, burst, walk)).toBe('x')
    // Not walking: the next slot, as before.
    expect(focusAfterMark(shown, shown, 'c', true, burst)).toBe('d')
  })

  it('keeps the marked photo in its stack’s slot when its stack changed cover', () => {
    // c rejected in "all": x now covers the stack; without auto-advance the cursor stays on c.
    const after = ['a', 'b', 'x', 'd', 'e']
    expect(focusAfterMark(shown, after, 'c', false, burst)).toBe('c')
    expect(focusAfterMark(shown, after, 'c', true, burst)).toBe('d')
  })

  it('selects a range from a walked frame through its slot', () => {
    const walking = { selection: {}, anchorId: 'x', focusId: 'x' }
    const range = clickSelection(walking, 'e', { range: true, toggle: false }, shown, burst)
    expect(Object.keys(range.selection)).toEqual(['c', 'd', 'e'])
    // ⌘-click toggles what the grid shows: the stack's slot.
    const toggled = clickSelection(walking, 'a', { range: false, toggle: true }, shown, burst)
    expect(Object.keys(toggled.selection).sort()).toEqual(['a', 'c'])
  })
})

describe('choose this one', () => {
  const photos = {
    best: photo('best', { verdict: 'keep', groupId: 1 }),
    mine: applyMarkPatch(photo('mine', { verdict: 'reject', groupId: 1 }), { flag: 'pick' }),
    other: photo('other', { verdict: 'maybe', groupId: 1 }),
    gone: photo('gone', { verdict: 'reject', groupId: 1 }),
    alt: photo('alt', { verdict: 'keep', groupId: 1 }),
  }
  const pool = ['best', 'mine', 'other', 'gone', 'alt']

  it('keeps the photo and rejects the other selected ones, by the AI or by the photographer', () => {
    expect(chooseChanges(photos, 'other', { pool })).toEqual([
      { ids: ['other'], patch: { flag: 'pick' } },
      { ids: ['best', 'mine', 'alt'], patch: { flag: 'reject' } },
    ])
  })

  it('leaves the to-review photos nobody looked at, and rejects what was shown next to it', () => {
    expect(chooseChanges(photos, 'alt', { pool })[1].ids).not.toContain('other')
    // A compare page: everything shown is rejected; other pages, only the selected photos.
    expect(chooseChanges(photos, 'best', { seen: ['best', 'other'], pool })).toEqual([
      { ids: ['best'], patch: { flag: 'pick' } },
      { ids: ['other', 'mine', 'alt'], patch: { flag: 'reject' } },
    ])
  })

  it('is a plain keep when nothing competes', () => {
    expect(chooseChanges(photos, 'best', { pool: ['best', 'gone'] })).toEqual([{ ids: ['best'], patch: { flag: 'pick' } }])
  })
})
