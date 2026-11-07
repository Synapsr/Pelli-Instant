import type { ColorLabel, Flag } from '@/lib/contract'
import type { Direction } from './selection'

export type KeyContext = 'grid' | 'loupe' | 'compare'

export type ShortcutAction =
  | { type: 'move'; dir: Direction }
  | { type: 'flag'; flag: Flag }
  | { type: 'rating'; rating: number }
  | { type: 'label'; label: ColorLabel }
  | { type: 'loupe' }
  | { type: 'escape' }
  | { type: 'compare' }
  | { type: 'group' }
  | { type: 'selectAll' }
  | { type: 'search' }
  | { type: 'export' }
  | { type: 'zoom' }
  | { type: 'undo' }
  | { type: 'help' }
  | { type: 'winner'; index: number }
  | { type: 'page'; delta: number }

export interface KeyInput {
  key: string
  code: string
  metaKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
}

const LABEL_KEYS: Record<number, ColorLabel> = { 6: 'red', 7: 'yellow', 8: 'green', 9: 'blue' }
const ARROWS: Record<string, Direction> = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down' }

/** Digit of the physical number row / keypad (AZERTY users need no Shift). */
function digitOf(e: KeyInput): number | null {
  const m = /^(?:Digit|Numpad)(\d)$/.exec(e.code)
  if (m) return Number(m[1])
  return /^\d$/.test(e.key) ? Number(e.key) : null
}

/**
 * Maps a key press to a workspace action. Letters use `key` (layout-aware,
 * so P is P on AZERTY), digits use `code`. Returns null for keys the
 * workspace does not handle.
 *
 * Marking: in the loupe, ↑ keeps and ↓ rejects, so a whole shoot is sorted
 * with one hand on the arrows (← → move). P / X / U, the Lightroom keys,
 * work everywhere for those who have them in their fingers.
 */
export function resolveShortcut(e: KeyInput, context: KeyContext): ShortcutAction | null {
  const mod = e.metaKey || e.ctrlKey
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key

  if (mod && !e.altKey) {
    if (key === 'a') return { type: 'selectAll' }
    if (key === 'f') return { type: 'search' }
    if (key === 'e') return { type: 'export' }
    if (key === 'z' && !e.shiftKey) return { type: 'undo' }
    return null
  }
  if (e.altKey) return null

  if (key === 'Escape') return { type: 'escape' }
  if (key === '?') return { type: 'help' }

  if (context === 'compare') {
    const digit = digitOf(e)
    if (digit !== null && digit >= 1 && digit <= 4) return { type: 'winner', index: digit - 1 }
    if (key === 'ArrowLeft' || key === 'ArrowRight') return { type: 'page', delta: key === 'ArrowLeft' ? -1 : 1 }
    if (key === 'z') return { type: 'zoom' }
    if (key === 'c') return { type: 'escape' }
    return null
  }

  const dir = ARROWS[key]
  if (dir) {
    if (context === 'loupe' && dir === 'up') return { type: 'flag', flag: 'pick' }
    if (context === 'loupe' && dir === 'down') return { type: 'flag', flag: 'reject' }
    return { type: 'move', dir }
  }
  if (key === ' ' || key === 'Enter') return { type: 'loupe' }
  if (key === 'p') return { type: 'flag', flag: 'pick' }
  if (key === 'x') return { type: 'flag', flag: 'reject' }
  if (key === 'u') return { type: 'flag', flag: 'none' }
  if (key === 'c') return { type: 'compare' }
  if (key === 'g') return { type: 'group' }
  if (key === 'z') return { type: 'zoom' }

  const digit = digitOf(e)
  if (digit !== null) {
    if (digit <= 5) return { type: 'rating', rating: digit }
    return { type: 'label', label: LABEL_KEYS[digit] }
  }
  return null
}

/** True when the event comes from a text field (shortcuts must not fire). */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!target || typeof (target as HTMLElement).tagName !== 'string') return false
  const el = target as HTMLElement
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable
}
