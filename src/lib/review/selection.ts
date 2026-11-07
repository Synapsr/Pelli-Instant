export interface SelectionState {
  selection: Readonly<Record<string, true>>
  anchorId: string | null
  focusId: string | null
}

export interface ClickModifiers {
  /** shift: range from the anchor */
  range: boolean
  /** cmd (macOS) / ctrl: toggle */
  toggle: boolean
}

/** Finder-like selection on click. */
export function clickSelection(state: SelectionState, id: string, mods: ClickModifiers, visible: readonly string[]): SelectionState {
  if (mods.range) {
    const anchor = state.anchorId ?? state.focusId ?? id
    const a = visible.indexOf(anchor)
    const b = visible.indexOf(id)
    if (a === -1 || b === -1) return { selection: { [id]: true }, anchorId: id, focusId: id }
    const [from, to] = a < b ? [a, b] : [b, a]
    const selection: Record<string, true> = mods.toggle ? { ...state.selection } : {}
    for (let i = from; i <= to; i++) selection[visible[i]] = true
    return { selection, anchorId: anchor, focusId: id }
  }
  if (mods.toggle) {
    const selection: Record<string, true> = { ...state.selection }
    // Toggling from a single implicit selection keeps the focused photo selected.
    if (state.focusId && Object.keys(selection).length === 0) selection[state.focusId] = true
    if (selection[id]) delete selection[id]
    else selection[id] = true
    return { selection, anchorId: id, focusId: id }
  }
  return { selection: { [id]: true }, anchorId: id, focusId: id }
}

export function selectAll(visible: readonly string[], focusId: string | null): SelectionState {
  const selection: Record<string, true> = {}
  visible.forEach((id) => (selection[id] = true))
  return { selection, anchorId: visible[0] ?? null, focusId: focusId ?? visible[0] ?? null }
}

/** Photos a mark applies to: the multi-selection, else the focused photo. */
export function markTargets(state: SelectionState, visible: readonly string[]): string[] {
  const keys = Object.keys(state.selection)
  if (keys.length > 1) {
    const order = new Map(visible.map((id, i) => [id, i]))
    return keys.sort((a, b) => (order.get(a) ?? Infinity) - (order.get(b) ?? Infinity))
  }
  return state.focusId ? [state.focusId] : []
}

export type Direction = 'left' | 'right' | 'up' | 'down'

/** Next focus in a grid of `columns` columns (clamped, no wrap). */
export function moveFocus(visible: readonly string[], focusId: string | null, dir: Direction, columns: number): string | null {
  if (!visible.length) return null
  const index = focusId ? visible.indexOf(focusId) : -1
  if (index === -1) return visible[0]
  const step = dir === 'left' ? -1 : dir === 'right' ? 1 : dir === 'up' ? -Math.max(1, columns) : Math.max(1, columns)
  const next = index + step
  if (next < 0) return dir === 'up' ? visible[index] : visible[0]
  if (next >= visible.length) return dir === 'down' ? visible[visible.length - 1] : visible[visible.length - 1]
  return visible[next]
}

/** Moves between rows of unequal length (the "Doublons" view). */
export function moveInRows(rows: readonly (readonly string[])[], focusId: string | null, dir: Direction): string | null {
  if (!rows.length) return null
  let r = rows.findIndex((row) => focusId !== null && row.includes(focusId))
  if (r === -1) return rows[0][0] ?? null
  let c = rows[r].indexOf(focusId as string)
  if (dir === 'left' || dir === 'right') {
    c += dir === 'left' ? -1 : 1
    if (c < 0 && r > 0) {
      r -= 1
      c = rows[r].length - 1
    } else if (c >= rows[r].length && r < rows.length - 1) {
      r += 1
      c = 0
    }
    c = Math.max(0, Math.min(rows[r].length - 1, c))
    return rows[r][c]
  }
  r = Math.max(0, Math.min(rows.length - 1, r + (dir === 'up' ? -1 : 1)))
  return rows[r][Math.min(c, rows[r].length - 1)]
}

/**
 * Focus after marking `targetId` with auto-advance: the next photo of the
 * list as it was, or — when the marked photo left the list (filtered tab) —
 * whatever now sits at its former index.
 */
export function focusAfterMark(
  before: readonly string[],
  after: readonly string[],
  targetId: string,
  advance: boolean,
): string | null {
  if (!after.length) return null
  const index = before.indexOf(targetId)
  const present = new Set(after)
  const stillThere = present.has(targetId)
  if (!advance) {
    if (stillThere) return targetId
    return after[Math.min(Math.max(index, 0), after.length - 1)]
  }
  for (let i = index + 1; i < before.length; i++) {
    if (present.has(before[i])) return before[i]
  }
  return stillThere ? targetId : after[after.length - 1]
}
