import { useVirtualizer } from '@tanstack/react-virtual'
import { Layers } from 'lucide-react'
import { memo, useEffect, useRef } from 'react'
import { DecisionDot } from '@/components/ui/Decision'
import { StarsBadge } from '@/components/ui/Marks'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { framePosition } from '@/lib/review/view'
import { useReviewStore } from '@/store/review-store'

const ITEM_W = 104
const GAP = 6

/** One slot of the strip; a walked stack shows the frame under the cursor. */
const FilmItem = memo(function FilmItem({ id, start }: { id: string; start: number }) {
  const focused = useReviewStore((s) => s.focusSlot === id)
  const shownId = useReviewStore((s) => (s.focusSlot === id && s.focusId ? s.focusId : id))
  const photo = useReviewStore((s) => s.photos[shownId])
  const group = useReviewStore((s) => {
    const gid = s.photos[shownId]?.assessment?.groupId
    return gid === null || gid === undefined ? undefined : s.groups[gid]
  })
  // Walking this stack: the frame's place in the burst ("3/7") instead of its size.
  const walkIndex = useReviewStore((s) => (focused && group && s.walkGroupId === group.id ? framePosition(group, shownId, s.filters.sort, s.search.hits, s) : -1))
  const groupSize = group?.photoIds.length ?? 0
  if (!photo) return null
  const pending = photo.status !== 'analyzed'
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label={photo.fileName}
      aria-current={focused}
      // A click must not keep the focus: the keyboard stays on the photos.
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => useReviewStore.getState().focus(shownId)}
      className={cn(
        'photo-well absolute top-2 bottom-2 overflow-hidden rounded-lg transition-[box-shadow,opacity] duration-200',
        focused ? 'ring-2 ring-[#5840f0] ring-offset-2 ring-offset-[var(--loupe-bg)] dark:ring-[#a599f5]' : 'opacity-75 hover:opacity-100',
        !photo.thumbUrl && 'photo-shimmer',
      )}
      style={{ left: start, width: ITEM_W }}
    >
      {photo.thumbUrl && <img src={photo.thumbUrl} alt="" draggable={false} className={cn('h-full w-full object-contain p-0.5', photo.decision === 'rejected' && !pending && 'opacity-45')} />}
      <span className="absolute bottom-1 left-1 flex rounded-full bg-black/55 p-[3px]">
        <DecisionDot decision={photo.decision} user={photo.mark.flag !== 'none'} pending={pending} size={6} />
      </span>
      {groupSize > 1 && (
        <span
          aria-hidden
          className={cn(
            'absolute right-1 top-1 flex h-[18px] items-center gap-0.5 rounded-full pl-1 pr-1.5 text-[10px] font-semibold text-white/95',
            walkIndex >= 0 ? 'bg-[#5840f0]' : 'bg-black/65',
          )}
        >
          <Layers className="size-2.5" />
          <span className="tabular">{walkIndex >= 0 ? `${formatNumber(walkIndex + 1)}/${formatNumber(groupSize)}` : formatNumber(groupSize)}</span>
        </span>
      )}
      {photo.mark.rating > 0 && (
        <span className="absolute bottom-1 right-1 flex h-[18px] items-center rounded-full bg-black/55 px-1.5">
          <StarsBadge rating={photo.mark.rating} />
        </span>
      )}
    </button>
  )
})

/** Horizontal, virtualised strip of the current filter. */
export function Filmstrip() {
  const t = useT()
  const ids = useReviewStore((s) => s.visibleIds)
  const focusSlot = useReviewStore((s) => s.focusSlot)
  const ref = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    horizontal: true,
    count: ids.length,
    getScrollElement: () => ref.current,
    estimateSize: () => ITEM_W + GAP,
    overscan: 8,
    paddingStart: 12,
    paddingEnd: 12,
  })
  const index = focusSlot ? ids.indexOf(focusSlot) : -1
  useEffect(() => {
    if (index >= 0) virtualizer.scrollToIndex(index, { align: 'center' })
  }, [index, virtualizer])

  return (
    <div ref={ref} aria-label={t('loupe.filmstrip')} className="relative h-[88px] shrink-0 overflow-x-auto overflow-y-hidden border-t border-default-200/60 bg-[var(--loupe-bg)] dark:border-white/[0.06]">
      <div className="relative h-full" style={{ width: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((item) => (
          <FilmItem key={ids[item.index]} id={ids[item.index]} start={item.start} />
        ))}
      </div>
    </div>
  )
}
