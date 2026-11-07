import { describe, expect, it } from 'vitest'
import { clickSelection, focusAfterMark, markTargets, moveFocus, moveInRows, selectAll } from './selection'

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
