import { useVirtualizer } from '@tanstack/react-virtual'
import { useEffect, useMemo, useRef } from 'react'
import { useElementSize } from '@/hooks/use-element-size'
import { useT } from '@/lib/i18n'
import { usePrefsStore } from '@/store/prefs-store'
import { useReviewStore } from '@/store/review-store'
import { CARD_INFO_HEIGHT, PhotoCard } from './PhotoCard'

const GAP = 12
const PAD = 20
/** Image area aspect (width / height) of a cell: 3:2 photos fit, 2:3 sit centred. */
const CELL_ASPECT = 1.32

export interface GridLayout {
  columns: number
  cellWidth: number
  cellHeight: number
}

export function gridLayout(width: number, target: number): GridLayout {
  const inner = Math.max(0, width - PAD * 2)
  const columns = Math.max(1, Math.floor((inner + GAP) / (target + GAP)))
  const cellWidth = Math.floor((inner - GAP * (columns - 1)) / columns)
  return { columns, cellWidth, cellHeight: Math.round(cellWidth / CELL_ASPECT) + CARD_INFO_HEIGHT }
}

/**
 * Virtualised grid: only the rows on screen (+ overscan) are mounted, cards
 * are absolutely positioned and keyed by photo id, so when a photo leaves the
 * current filter the others glide to their new place instead of remounting.
 */
export function PhotoGrid() {
  const t = useT()
  const ids = useReviewStore((s) => s.visibleIds)
  const focusId = useReviewStore((s) => s.focusId)
  const setColumns = useReviewStore((s) => s.setColumns)
  const thumbSize = usePrefsStore((s) => s.thumbSize)
  const ref = useRef<HTMLDivElement>(null)
  const { width } = useElementSize(ref)
  const layout = useMemo(() => gridLayout(width, thumbSize), [width, thumbSize])
  const rows = Math.ceil(ids.length / layout.columns)

  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => ref.current,
    estimateSize: () => layout.cellHeight + GAP,
    overscan: 3,
    paddingStart: PAD,
    paddingEnd: PAD,
  })

  useEffect(() => {
    if (width > 0) setColumns(layout.columns)
  }, [layout.columns, setColumns, width])

  useEffect(() => {
    virtualizer.measure()
  }, [layout.cellHeight, virtualizer])

  // Keep the focused photo on screen while navigating with the keyboard.
  useEffect(() => {
    if (!focusId) return
    const index = ids.indexOf(focusId)
    if (index >= 0) virtualizer.scrollToIndex(Math.floor(index / layout.columns), { align: 'auto' })
  }, [focusId, ids, layout.columns, virtualizer])

  const items = virtualizer.getVirtualItems()
  const first = items.length ? items[0].index * layout.columns : 0
  const last = items.length ? Math.min(ids.length, (items[items.length - 1].index + 1) * layout.columns) : 0

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('review.tabs.all')}
      aria-rowcount={rows}
      aria-colcount={layout.columns}
      data-grid-root
      tabIndex={0}
      className="focus-ring-inset relative h-full overflow-y-auto overscroll-contain outline-none"
    >
      <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
        {width > 0 &&
          ids.slice(first, last).map((id, i) => {
            const index = first + i
            const row = Math.floor(index / layout.columns)
            const col = index % layout.columns
            const x = PAD + col * (layout.cellWidth + GAP)
            const y = PAD + row * (layout.cellHeight + GAP)
            return (
              <PhotoCard key={id} id={id} width={layout.cellWidth} height={layout.cellHeight} x={x} y={y} />
            )
          })}
      </div>
    </div>
  )
}
