import { describe, expect, it } from 'vitest'
import { leftToControl, resolveShortcut, resultEnterReviews, type KeyInput, type KeyTarget } from './keyboard'

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

  it('goes through the burst with ⌥← ⌥→ (Alt on Windows) in the grid and the loupe', () => {
    expect(resolveShortcut(key('ArrowRight', { altKey: true }), 'loupe')).toEqual({ type: 'groupMove', delta: 1 })
    expect(resolveShortcut(key('ArrowLeft', { altKey: true }), 'grid')).toEqual({ type: 'groupMove', delta: -1 })
    // Not in compare, not with ⌘ / Ctrl, not up and down (they keep and reject in the loupe).
    expect(resolveShortcut(key('ArrowRight', { altKey: true }), 'compare')).toBeNull()
    expect(resolveShortcut(key('ArrowRight', { altKey: true, metaKey: true }), 'loupe')).toBeNull()
    expect(resolveShortcut(key('ArrowUp', { altKey: true }), 'loupe')).toBeNull()
  })

  it('chooses a frame of a burst with S, in the grid and the loupe', () => {
    expect(resolveShortcut(key('s'), 'loupe')).toEqual({ type: 'choose' })
    expect(resolveShortcut(key('S', { shiftKey: true }), 'grid')).toEqual({ type: 'choose' })
    // Compare chooses with 1–4; ⌘S / Alt+S are not ours.
    expect(resolveShortcut(key('s'), 'compare')).toBeNull()
    expect(resolveShortcut(key('s', { metaKey: true }), 'loupe')).toBeNull()
    expect(resolveShortcut(key('s', { altKey: true }), 'loupe')).toBeNull()
  })

  it('shows the photo in full screen with F, ⌘F staying the search', () => {
    expect(resolveShortcut(key('f'), 'loupe')).toEqual({ type: 'fullscreen' })
    expect(resolveShortcut(key('F', { shiftKey: true }), 'grid')).toEqual({ type: 'fullscreen' })
    expect(resolveShortcut(key('f', { metaKey: true }), 'loupe')).toEqual({ type: 'search' })
    expect(resolveShortcut(key('f'), 'compare')).toBeNull()
  })

  it('uses 1–4 to pick the winner and arrows to page in compare', () => {
    expect(resolveShortcut(key('1'), 'compare')).toEqual({ type: 'winner', index: 0 })
    expect(resolveShortcut(key('4'), 'compare')).toEqual({ type: 'winner', index: 3 })
    expect(resolveShortcut(key('5'), 'compare')).toBeNull()
    expect(resolveShortcut(key('ArrowRight'), 'compare')).toEqual({ type: 'page', delta: 1 })
    expect(resolveShortcut(key('p'), 'compare')).toBeNull()
  })
})

/** A fake element sitting inside the given selectors (what `closest` answers). */
function inside(...selectors: string[]): KeyTarget {
  return { closest: (list: string) => (list.split(',').some((sel) => selectors.includes(sel.trim())) ? {} : null) }
}

describe('keys left to a focused control', () => {
  it('keeps the arrows on the photos after a click on a button (filmstrip, Keep, chevrons)', () => {
    const thumbnail = inside('button')
    expect(leftToControl('ArrowRight', thumbnail)).toBe(false)
    expect(leftToControl('ArrowUp', thumbnail)).toBe(false)
    // The button still answers Enter and Space.
    expect(leftToControl('Enter', thumbnail)).toBe(true)
    expect(leftToControl(' ', thumbnail)).toBe(true)
  })

  it('leaves the arrows to the widgets that use them', () => {
    expect(leftToControl('ArrowLeft', inside('button', '[role="radio"]'))).toBe(true)
    expect(leftToControl('ArrowUp', inside('[role="slider"]'))).toBe(true)
    expect(leftToControl('ArrowDown', inside('button', '[role="menuitem"]'))).toBe(true)
    expect(leftToControl('ArrowRight', inside('[role="dialog"]'))).toBe(true)
    expect(leftToControl('End', inside('[role="option"]'))).toBe(true)
  })

  it('never takes the keys from the grid, nor without a control', () => {
    expect(leftToControl('Enter', inside('[data-grid-root]', 'button'))).toBe(false)
    expect(leftToControl('ArrowRight', inside())).toBe(false)
    expect(leftToControl(' ', null)).toBe(false)
    expect(leftToControl('p', inside('button'))).toBe(false)
  })
})

describe('Enter on the result screen of a sort', () => {
  const surface: KeyTarget = { closest: () => null }

  it('opens the review from the screen itself', () => {
    expect(resultEnterReviews('Enter', surface)).toBe(true)
    expect(resultEnterReviews('Enter', null)).toBe(true)
    expect(resultEnterReviews(' ', surface)).toBe(false)
  })

  it('leaves Enter to a focused control: the support dialog’s buttons, links', () => {
    expect(resultEnterReviews('Enter', inside('button'))).toBe(false)
    expect(resultEnterReviews('Enter', inside('a[href]'))).toBe(false)
    expect(resultEnterReviews('Enter', inside('[role="dialog"]'))).toBe(false)
  })

  it('leaves Enter to a text field', () => {
    const field = { tagName: 'INPUT', isContentEditable: false, closest: () => null } as unknown as KeyTarget
    expect(resultEnterReviews('Enter', field)).toBe(false)
  })
})
