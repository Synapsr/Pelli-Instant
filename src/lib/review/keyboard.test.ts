import { describe, expect, it } from 'vitest'
import { resolveShortcut, type KeyInput } from './keyboard'

function key(k: string, extra: Partial<KeyInput> = {}): KeyInput {
  const code = /^\d$/.test(k) ? `Digit${k}` : k.length === 1 ? `Key${k.toUpperCase()}` : k
  return { key: k, code, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...extra }
}

describe('keyboard shortcuts', () => {
  it('maps flags, stars and labels', () => {
    expect(resolveShortcut(key('p'), 'grid')).toEqual({ type: 'flag', flag: 'pick' })
    expect(resolveShortcut(key('X'), 'grid')).toEqual({ type: 'flag', flag: 'reject' })
    expect(resolveShortcut(key('u'), 'loupe')).toEqual({ type: 'flag', flag: 'none' })
    expect(resolveShortcut(key('0'), 'grid')).toEqual({ type: 'rating', rating: 0 })
    expect(resolveShortcut(key('5'), 'grid')).toEqual({ type: 'rating', rating: 5 })
    expect(resolveShortcut(key('6'), 'grid')).toEqual({ type: 'label', label: 'red' })
    expect(resolveShortcut(key('9'), 'grid')).toEqual({ type: 'label', label: 'blue' })
  })

  it('reads digits from the physical row on AZERTY keyboards', () => {
    // AZERTY: the "1" key without Shift produces "&".
    expect(resolveShortcut({ ...key('&'), code: 'Digit1' }, 'grid')).toEqual({ type: 'rating', rating: 1 })
    expect(resolveShortcut({ ...key('è'), code: 'Digit7' }, 'grid')).toEqual({ type: 'label', label: 'yellow' })
  })

  it('keeps and rejects with the up and down arrows in the loupe', () => {
    expect(resolveShortcut(key('ArrowUp'), 'loupe')).toEqual({ type: 'flag', flag: 'pick' })
    expect(resolveShortcut(key('ArrowDown'), 'loupe')).toEqual({ type: 'flag', flag: 'reject' })
    // The grid keeps them to move between rows, compare ignores them.
    expect(resolveShortcut(key('ArrowUp'), 'grid')).toEqual({ type: 'move', dir: 'up' })
    expect(resolveShortcut(key('ArrowDown'), 'compare')).toBeNull()
    // The Lightroom keys stay as aliases.
    expect(resolveShortcut(key('p'), 'loupe')).toEqual({ type: 'flag', flag: 'pick' })
    expect(resolveShortcut(key('x'), 'loupe')).toEqual({ type: 'flag', flag: 'reject' })
  })

  it('maps navigation and views', () => {
    expect(resolveShortcut(key('ArrowRight'), 'grid')).toEqual({ type: 'move', dir: 'right' })
    expect(resolveShortcut(key('ArrowDown'), 'grid')).toEqual({ type: 'move', dir: 'down' })
    expect(resolveShortcut(key(' ', { code: 'Space' }), 'grid')).toEqual({ type: 'loupe' })
    expect(resolveShortcut(key('Enter'), 'grid')).toEqual({ type: 'loupe' })
    expect(resolveShortcut(key('c'), 'grid')).toEqual({ type: 'compare' })
    expect(resolveShortcut(key('g'), 'grid')).toEqual({ type: 'group' })
    expect(resolveShortcut(key('z'), 'loupe')).toEqual({ type: 'zoom' })
    expect(resolveShortcut(key('?', { shiftKey: true, code: 'Slash' }), 'grid')).toEqual({ type: 'help' })
    expect(resolveShortcut(key('Escape'), 'loupe')).toEqual({ type: 'escape' })
  })

  it('maps command shortcuts with Cmd or Ctrl', () => {
    expect(resolveShortcut(key('a', { metaKey: true }), 'grid')).toEqual({ type: 'selectAll' })
    expect(resolveShortcut(key('f', { ctrlKey: true }), 'grid')).toEqual({ type: 'search' })
    expect(resolveShortcut(key('e', { metaKey: true }), 'grid')).toEqual({ type: 'export' })
    expect(resolveShortcut(key('z', { ctrlKey: true }), 'grid')).toEqual({ type: 'undo' })
    expect(resolveShortcut(key('p', { metaKey: true }), 'grid')).toBeNull()
    expect(resolveShortcut(key('p', { altKey: true }), 'grid')).toBeNull()
  })

  it('uses 1–4 to pick the winner and arrows to page in compare', () => {
    expect(resolveShortcut(key('1'), 'compare')).toEqual({ type: 'winner', index: 0 })
    expect(resolveShortcut(key('4'), 'compare')).toEqual({ type: 'winner', index: 3 })
    expect(resolveShortcut(key('5'), 'compare')).toBeNull()
    expect(resolveShortcut(key('ArrowRight'), 'compare')).toEqual({ type: 'page', delta: 1 })
    expect(resolveShortcut(key('p'), 'compare')).toBeNull()
  })
})
