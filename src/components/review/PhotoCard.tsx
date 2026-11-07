import { Award, Check, Flag, Layers } from 'lucide-react'
import { memo, type MouseEvent } from 'react'
import { DecisionDot, DECISION_TEXT } from '@/components/ui/Decision'
import { LabelSwatch, StarsBadge } from '@/components/ui/Marks'
import { ReasonIcon } from '@/components/ui/ReasonIcon'
import type { PhotoView } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT, type Translate } from '@/lib/i18n'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'

export const CARD_INFO_HEIGHT = 30

function decisionLabel(t: Translate, photo: PhotoView): string {
  if (photo.status !== 'analyzed') return t('review.card.pending')
  const decision = t(`verdict.${photo.decision}`)
  return photo.mark.flag !== 'none' ? `${decision} · ${t('verdict.yourChoice')}` : `${decision} · ${t('verdict.aiChoice')}`
}

/**
 * One photo of the grid. Subscribes to its own photo / focus / selection so
 * a mark re-renders this card only.
 */
export const PhotoCard = memo(function PhotoCard({
  id,
  width,
  height,
  x,
  y,
  flow = false,
}: {
  id: string
  width: number
  height: number
  /** absolute position in the grid (ignored with `flow`) */
  x?: number
  y?: number
  /** laid out by the parent (flex row) instead of absolutely positioned */
  flow?: boolean
}) {
  const t = useT()
  const photo = useReviewStore((s) => s.photos[id])
  const focused = useReviewStore((s) => s.focusId === id)
  const selected = useReviewStore((s) => s.selection[id] === true)
  const group = useReviewStore((s) => {
    const gid = s.photos[id]?.assessment?.groupId
    return gid === null || gid === undefined ? undefined : s.groups[gid]
  })
  const expanded = useReviewStore((s) => (group ? s.filters.expanded[group.id] === true && s.filters.stackGroups : false))
  const inDuplicates = useReviewStore((s) => s.filters.tab === 'duplicates')
  const stacked = useReviewStore((s) => s.filters.stackGroups) && !inDuplicates

  if (!photo) return null
  const pending = photo.status !== 'analyzed'
  const rejected = !pending && photo.decision === 'rejected'
  const reasons = photo.assessment?.reasons ?? []
  const groupSize = group?.photoIds.length ?? 0
  const rank = group ? group.photoIds.indexOf(id) : -1
  const imageHeight = height - CARD_INFO_HEIGHT

  const onClick = (e: MouseEvent) => {
    useReviewStore.getState().click(id, { range: e.shiftKey, toggle: e.metaKey || e.ctrlKey })
  }
  const onDoubleClick = () => {
    useReviewStore.getState().focus(id)
    useWorkspaceStore.getState().openLoupe()
  }
  const onGroupBadge = (e: MouseEvent) => {
    e.stopPropagation()
    if (!group) return
    if (stacked && e.altKey) useReviewStore.getState().toggleGroup(group.id)
    else useWorkspaceStore.getState().openCompare(group.photoIds)
  }

  return (
    <div
      role="gridcell"
      aria-selected={focused || selected}
      aria-label={`${photo.fileName}, ${decisionLabel(t, photo)}`}
      data-photo-id={id}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      className={cn('group/card select-none', flow ? 'relative flex-none' : 'absolute left-0 top-0')}
      style={flow ? { width, height } : { width, height, transform: `translate3d(${x ?? 0}px, ${y ?? 0}px, 0)`, transition: 'transform 260ms cubic-bezier(0.22, 1, 0.36, 1)' }}
    >
      {stacked && groupSize > 1 && !expanded && (
        <>
          <span aria-hidden className="absolute inset-x-3 -top-[5px] h-3 rounded-t-lg bg-default-200/70 dark:bg-white/[0.06]" />
          <span aria-hidden className="absolute inset-x-1.5 -top-[2.5px] h-3 rounded-t-[10px] bg-default-200 dark:bg-white/[0.1]" />
        </>
      )}
      <div
        className={cn(
          'photo-well relative overflow-hidden rounded-xl transition-[box-shadow,background-color] duration-200 ease-out',
          pending && !photo.thumbUrl && 'photo-shimmer',
          focused
            ? 'ring-2 ring-[#5840f0] ring-offset-2 ring-offset-background dark:ring-[#a599f5]'
            : selected
              ? 'ring-2 ring-[#5840f0]/60 dark:ring-[#a599f5]/60'
              : 'group-hover/card:ring-1 group-hover/card:ring-default-300 dark:group-hover/card:ring-white/15',
        )}
        style={{ height: imageHeight }}
      >
        {photo.thumbUrl && (
          <div className={cn('h-full w-full transition-opacity duration-300', rejected && 'opacity-40', pending && 'opacity-70')}>
            <img src={photo.thumbUrl} alt="" loading="lazy" decoding="async" draggable={false} className="h-full w-full object-contain p-1.5 animate-fade-in" />
          </div>
        )}
        {selected && (
          <span className="absolute left-2 top-2 flex size-5 items-center justify-center rounded-full bg-[#5840f0] text-white shadow-md shadow-black/30 dark:bg-[#a599f5] dark:text-[#141414]">
            <Check className="size-3" strokeWidth={3} aria-hidden />
          </span>
        )}
        {!selected && reasons.length > 0 && (
          <div className="pointer-events-none absolute left-2 top-2 flex gap-1 opacity-0 transition-opacity duration-200 group-hover/card:opacity-100">
            {reasons.slice(0, 3).map((r) => (
              <span key={r} title={t(`reason.${r}`)} className="flex size-6 items-center justify-center rounded-full bg-black/65 text-white/90">
                <ReasonIcon reason={r} className="size-3.5" />
              </span>
            ))}
          </div>
        )}
        {inDuplicates && rank === 0 && (
          <span className="absolute right-2 top-2 inline-flex h-6 items-center gap-1 rounded-full bg-[#5840f0] px-2 text-[11px] font-semibold text-white shadow-md shadow-[#5840f0]/30">
            <Award className="size-3" aria-hidden />
            {t('compare.best')}
          </span>
        )}
        {!inDuplicates && groupSize > 1 && (rank === 0 || !stacked || !expanded) && (
          <button
            type="button"
            tabIndex={-1}
            onClick={onGroupBadge}
            title={`${t('review.card.group', { count: groupSize })} · ${t('review.groups.compare')} (C)`}
            className="absolute right-2 top-2 flex h-6 items-center gap-1 rounded-full bg-black/65 pl-1.5 pr-2 text-[11px] font-semibold text-white/95 transition-colors hover:bg-black/80"
          >
            <Layers className="size-3" aria-hidden />
            {groupSize}
          </button>
        )}
        {stacked && expanded && rank > 0 && (
          <span className="absolute right-2 top-2 flex h-6 items-center rounded-full bg-black/60 px-2 text-[11px] font-medium text-white/80 tabular">
            {rank + 1}/{groupSize}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1.5 px-1" style={{ height: CARD_INFO_HEIGHT }}>
        <DecisionDot decision={photo.decision} user={photo.mark.flag !== 'none'} pending={pending} size={7} />
        <span className={cn('min-w-0 flex-1 truncate text-xs', rejected ? 'text-default-400' : 'text-default-600')}>{photo.fileName}</span>
        <LabelSwatch label={photo.mark.label} size={8} />
        <StarsBadge rating={photo.mark.rating} />
        {photo.mark.flag !== 'none' && <Flag className={cn('size-3', DECISION_TEXT[photo.decision], photo.mark.flag === 'pick' && 'fill-current')} aria-hidden />}
        {photo.assessment && <span className="tabular text-[11px] text-default-400">{formatNumber(Math.round(photo.assessment.score))}</span>}
      </div>
    </div>
  )
})
