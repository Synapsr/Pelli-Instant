import { Button } from '@heroui/react'
import { Award, ChevronLeft, ChevronRight, ImageIcon, LayoutGrid, Link2 } from 'lucide-react'
import { memo, useMemo } from 'react'
import { ZoomableImage } from '@/components/loupe/ZoomableImage'
import { DecisionDot, DECISION_TEXT } from '@/components/ui/Decision'
import { ReasonIcon } from '@/components/ui/ReasonIcon'
import { pickCompareWinner } from '@/hooks/use-workspace-keyboard'
import { useLargePreview } from '@/hooks/use-large-preview'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useReviewStore } from '@/store/review-store'
import { COMPARE_PAGE_SIZE, useWorkspaceStore } from '@/store/workspace-store'

const Pane = memo(function Pane({ id, index, best }: { id: string; index: number; best: boolean }) {
  const t = useT()
  const projectId = useReviewStore((s) => s.projectId)
  const photo = useReviewStore((s) => s.photos[id])
  const zoom = useWorkspaceStore((s) => s.zoom)
  const setZoom = useWorkspaceStore((s) => s.setZoom)
  const url = useLargePreview(projectId, id)
  if (!photo) return null
  const reasons = photo.assessment?.reasons ?? []
  return (
    <div
      className={cn(
        'relative flex min-h-0 min-w-0 flex-col overflow-hidden rounded-2xl bg-[var(--loupe-bg)] ring-1 transition-shadow duration-200',
        best ? 'ring-2 ring-[#5840f0]/70 dark:ring-[#a599f5]/70' : 'ring-default-200/70 dark:ring-white/[0.07]',
      )}
    >
      <div className="relative min-h-0 flex-1">
        <ZoomableImage src={url} fallbackSrc={photo.thumbUrl} photoWidth={photo.width} photoHeight={photo.height} zoom={zoom} onZoom={setZoom} padding={14} label={photo.fileName} />
        <span className="pointer-events-none absolute left-3 top-3 flex size-6 items-center justify-center rounded-md bg-black/55 text-xs font-semibold text-white backdrop-blur-md">{index + 1}</span>
        {best && (
          <span className="pointer-events-none absolute right-3 top-3 inline-flex h-6 items-center gap-1 rounded-full bg-[#5840f0] px-2.5 text-[11px] font-semibold text-white shadow-lg shadow-[#5840f0]/30">
            <Award className="size-3.5" aria-hidden />
            {t('compare.best')}
          </span>
        )}
      </div>
      <div className="flex h-12 shrink-0 items-center gap-2 border-t border-default-200/60 bg-background/80 px-3 backdrop-blur dark:border-white/[0.06]">
        <DecisionDot decision={photo.decision} user={photo.mark.flag !== 'none'} pending={photo.status !== 'analyzed'} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13px] font-medium text-foreground">{photo.fileName}</p>
          <p className={cn('truncate text-[11px]', DECISION_TEXT[photo.decision])}>{t(`verdict.${photo.decision}`)}</p>
        </div>
        <div className="flex items-center gap-1 text-default-400">
          {reasons.slice(0, 3).map((r) => (
            <span key={r} title={t(`reason.${r}`)}>
              <ReasonIcon reason={r} className="size-3.5" />
            </span>
          ))}
        </div>
        {photo.assessment && <span className="tabular w-8 text-right text-sm font-semibold text-foreground">{formatNumber(Math.round(photo.assessment.score))}</span>}
        <Button size="sm" variant="flat" radius="lg" className="h-7 min-w-0 px-2.5 text-xs font-semibold" onPress={() => pickCompareWinner(index)} title={t('compare.pickKey', { key: index + 1 })}>
          {t('compare.pick')}
          <kbd className="font-sans text-[10px] opacity-60">{index + 1}</kbd>
        </Button>
      </div>
    </div>
  )
})

/** 2–4 photos side by side with synced zoom/pan; one key picks the winner. */
export function CompareView() {
  const t = useT()
  const ids = useWorkspaceStore((s) => s.compareIds)
  const page = useWorkspaceStore((s) => s.comparePage)
  const setPage = useWorkspaceStore((s) => s.setComparePage)
  const close = useWorkspaceStore((s) => s.closeOverlay)
  const fromLoupe = useWorkspaceStore((s) => s.compareFrom === 'loupe')
  const photos = useReviewStore((s) => s.photos)
  const pages = Math.max(1, Math.ceil(ids.length / COMPARE_PAGE_SIZE))
  const pageIds = ids.slice(page * COMPARE_PAGE_SIZE, page * COMPARE_PAGE_SIZE + COMPARE_PAGE_SIZE)
  const bestId = useMemo(() => ids.reduce<string | null>((best, id) => ((photos[id]?.assessment?.score ?? -1) > (best ? (photos[best]?.assessment?.score ?? -1) : -1) ? id : best), null), [ids, photos])
  const cols = pageIds.length === 3 ? 'grid-cols-3' : 'grid-cols-2'
  const rows = pageIds.length === 4 ? 'grid-rows-2' : 'grid-rows-1'

  return (
    <div className="flex min-h-0 flex-1 flex-col animate-fade-in">
      <div className="flex h-12 shrink-0 items-center gap-3 border-b border-default-200/60 px-3 dark:border-white/[0.06]">
        <Button
          size="sm"
          variant="light"
          radius="lg"
          className="h-8 min-w-0 gap-1.5 px-2.5 text-[13px] text-default-500"
          startContent={fromLoupe ? <ImageIcon className="size-3.5" /> : <LayoutGrid className="size-3.5" />}
          onPress={close}
          title={`${fromLoupe ? t('compare.backToLoupe') : t('loupe.backToGrid')} (Esc)`}
        >
          {fromLoupe ? t('compare.backToLoupe') : t('loupe.backToGrid')}
        </Button>
        <h2 className="text-[13px] font-semibold text-foreground">{t('compare.title')}</h2>
        <span className="text-xs text-default-400">{t('common.photos', { count: ids.length })}</span>
        <span className="inline-flex items-center gap-1 text-xs text-default-400">
          <Link2 className="size-3.5" aria-hidden />
          {t('compare.syncedZoom')}
        </span>
        <p className="mx-auto hidden text-xs text-default-400 lg:block">{t('compare.hint')}</p>
        {pages > 1 && (
          <div className="flex items-center gap-1">
            <Button isIconOnly size="sm" variant="light" radius="lg" className="h-8 w-8 min-w-8" aria-label={t('compare.previousPage')} isDisabled={page === 0} onPress={() => setPage(page - 1)}>
              <ChevronLeft className="size-4" />
            </Button>
            <span className="tabular text-xs text-default-500">
              {t('compare.page', { from: page * COMPARE_PAGE_SIZE + 1, to: page * COMPARE_PAGE_SIZE + pageIds.length, total: ids.length })}
            </span>
            <Button isIconOnly size="sm" variant="light" radius="lg" className="h-8 w-8 min-w-8" aria-label={t('compare.nextPage')} isDisabled={page >= pages - 1} onPress={() => setPage(page + 1)}>
              <ChevronRight className="size-4" />
            </Button>
          </div>
        )}
      </div>
      <div className={cn('grid min-h-0 flex-1 gap-3 bg-background p-3', cols, rows)}>
        {pageIds.map((id, i) => (
          <Pane key={id} id={id} index={i} best={id === bestId} />
        ))}
      </div>
    </div>
  )
}
