import { Button, Tooltip } from '@heroui/react'
import { ChevronLeft, ChevronRight, Layers, LayoutGrid, PanelRight } from 'lucide-react'
import { useState } from 'react'
import { MarkFlash } from '@/components/review/MarkFlash'
import { useLargePreview } from '@/hooks/use-large-preview'
import { zoomTarget } from '@/hooks/use-workspace-keyboard'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { altLabel } from '@/lib/platform'
import { usePrefsStore } from '@/store/prefs-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { Filmstrip } from './Filmstrip'
import { Inspector } from './Inspector'
import { MarkControls } from './MarkControls'
import { ZoomableImage } from './ZoomableImage'

/** Large preview of the focused photo with filmstrip and inspector. */
export function Loupe() {
  const t = useT()
  const projectId = useReviewStore((s) => s.projectId)
  const focusId = useReviewStore((s) => s.focusId)
  const photo = useReviewStore((s) => (s.focusId ? s.photos[s.focusId] : undefined))
  const ids = useReviewStore((s) => s.visibleIds)
  const group = useReviewStore((s) => {
    const gid = s.focusId ? s.photos[s.focusId]?.assessment?.groupId : null
    return gid === null || gid === undefined ? undefined : s.groups[gid]
  })
  const move = useReviewStore((s) => s.move)
  const zoom = useWorkspaceStore((s) => s.zoom)
  const setZoom = useWorkspaceStore((s) => s.setZoom)
  const resetZoom = useWorkspaceStore((s) => s.resetZoom)
  const close = useWorkspaceStore((s) => s.closeOverlay)
  const inspectorOpen = usePrefsStore((s) => s.inspectorOpen)
  const toggleInspector = usePrefsStore((s) => s.toggleInspector)
  const [hoverFace, setHoverFace] = useState<number | null>(null)
  const index = focusId ? ids.indexOf(focusId) : -1
  const url = useLargePreview(projectId, focusId, [ids[index + 1], ids[index - 1], ids[index + 2]])

  if (!photo) {
    return (
      <div className="flex flex-1 items-center justify-center bg-[var(--loupe-bg)]">
        <Button variant="flat" onPress={close}>
          {t('loupe.backToGrid')}
        </Button>
      </div>
    )
  }

  const go = (dir: 'left' | 'right') => {
    move(dir)
    resetZoom()
  }
  const groupSize = group?.photoIds.length ?? 0
  const groupIndex = group ? group.photoIds.indexOf(photo.id) : -1

  return (
    <div className="flex min-h-0 flex-1 animate-fade-in">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="group/loupe relative min-h-0 flex-1 bg-[var(--loupe-bg)]">
          <ZoomableImage
            key={photo.id}
            src={url}
            fallbackSrc={photo.thumbUrl}
            photoWidth={photo.width}
            photoHeight={photo.height}
            zoom={zoom}
            onZoom={setZoom}
            faces={photo.faces}
            highlight={hoverFace}
            label={photo.fileName}
          />
          {!url && (
            <span className="pointer-events-none absolute bottom-4 left-1/2 -translate-x-1/2 rounded-full bg-black/50 px-3 py-1 text-xs text-white/80 backdrop-blur-md">{t('loupe.loading')}</span>
          )}
          {!zoom.active && (
            <>
              <button
                type="button"
                aria-label={t('loupe.previous')}
                onClick={() => go('left')}
                disabled={index <= 0}
                className="focus-ring absolute left-3 top-1/2 flex size-10 -translate-y-1/2 items-center justify-center rounded-full bg-black/40 text-white/90 opacity-0 backdrop-blur-md transition-opacity duration-200 hover:bg-black/60 disabled:hidden group-hover/loupe:opacity-100"
              >
                <ChevronLeft className="size-5" />
              </button>
              <button
                type="button"
                aria-label={t('loupe.next')}
                onClick={() => go('right')}
                disabled={index >= ids.length - 1}
                className="focus-ring absolute right-3 top-1/2 flex size-10 -translate-y-1/2 items-center justify-center rounded-full bg-black/40 text-white/90 opacity-0 backdrop-blur-md transition-opacity duration-200 hover:bg-black/60 disabled:hidden group-hover/loupe:opacity-100"
              >
                <ChevronRight className="size-5" />
              </button>
            </>
          )}
          <div className="pointer-events-none absolute left-4 top-4 flex items-center gap-2">
            {zoom.active && <span className="rounded-full bg-black/55 px-2.5 py-1 text-[11px] font-semibold text-white/90 backdrop-blur-md">{t('loupe.actual')}</span>}
            {/* Place in its burst: ⌥← ⌥→ go through every frame. */}
            {groupSize > 1 && groupIndex >= 0 && (
              <span
                title={t('loupe.groupPosition', { index: formatNumber(groupIndex + 1), total: formatNumber(groupSize), keys: `${altLabel()}← ${altLabel()}→` })}
                className="pointer-events-auto inline-flex h-6 items-center gap-1 rounded-full bg-black/55 pl-2 pr-2.5 text-[11px] font-semibold text-white/90 backdrop-blur-md"
              >
                <Layers className="size-3" aria-hidden />
                <span className="tabular">
                  {formatNumber(groupIndex + 1)}/{formatNumber(groupSize)}
                </span>
              </span>
            )}
          </div>
          <MarkFlash />

          {/* One floating bar: back, photo, keep / reject, zoom, details. */}
          <div className="glass-strong absolute bottom-4 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-full p-1.5 pl-2 animate-rise-in">
            <Tooltip content={`${t('loupe.backToGrid')} (Esc)`} delay={400} closeDelay={0}>
              <button type="button" aria-label={t('loupe.backToGrid')} onClick={close} className="focus-ring flex size-9 items-center justify-center rounded-full text-white/70 hover:bg-white/10 hover:text-white">
                <LayoutGrid className="size-4" />
              </button>
            </Tooltip>
            <div className="min-w-0 max-w-[220px] px-2">
              <p className="truncate text-[13px] font-semibold leading-tight text-white">{photo.fileName}</p>
              <p className="tabular text-[11px] leading-tight text-white/45">{t('loupe.position', { index: formatNumber(index + 1), total: formatNumber(ids.length) })}</p>
            </div>
            <span className="mx-1 h-6 w-px bg-white/10" aria-hidden />
            <MarkControls photo={photo} />
            <span className="mx-1 h-6 w-px bg-white/10" aria-hidden />
            <button
              type="button"
              aria-pressed={zoom.active}
              title={`${t('loupe.actual')} (Z)`}
              onClick={() => setZoom(zoom.active ? { active: false } : { active: true, ...zoomTarget(photo.id) })}
              className={cn('focus-ring h-9 rounded-full px-3 text-[13px] font-medium transition-colors', zoom.active ? 'bg-white text-[#141414]' : 'text-white/70 hover:bg-white/10 hover:text-white')}
            >
              {zoom.active ? t('loupe.actual') : t('loupe.fit')}
            </button>
            <Tooltip content={t('inspector.title')} delay={400} closeDelay={0}>
              <button
                type="button"
                aria-label={t('inspector.title')}
                aria-pressed={inspectorOpen}
                onClick={toggleInspector}
                className={cn('focus-ring flex size-9 items-center justify-center rounded-full transition-colors', inspectorOpen ? 'bg-white/15 text-white' : 'text-white/60 hover:bg-white/10 hover:text-white')}
              >
                <PanelRight className="size-4" />
              </button>
            </Tooltip>
          </div>
        </div>
        <Filmstrip />
      </div>
      {inspectorOpen && (
        <Inspector
          photo={photo}
          previewUrl={url}
          onHoverFace={setHoverFace}
          onFocusFace={(face) => setZoom({ active: true, x: face.bbox.x + face.bbox.w / 2, y: face.bbox.y + face.bbox.h / 2 })}
        />
      )}
    </div>
  )
}
