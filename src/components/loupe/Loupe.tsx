import { Button, Tooltip } from '@heroui/react'
import { Award, ChevronLeft, ChevronRight, Layers, LayoutGrid, Maximize2, Minimize2, PanelRight } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { MarkFlash } from '@/components/review/MarkFlash'
import { useLargePreview } from '@/hooks/use-large-preview'
import { zoomTarget } from '@/hooks/use-workspace-keyboard'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { altLabel } from '@/lib/platform'
import { effectiveSort, framePosition, groupOrder, isBest } from '@/lib/review/view'
import { usePrefsStore } from '@/store/prefs-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { Filmstrip } from './Filmstrip'
import { Inspector } from './Inspector'
import { MarkControls } from './MarkControls'
import { ZoomableImage } from './ZoomableImage'

/** The floating bar: 12 px from the bottom, 48 px high (36 px buttons, 40 px to the pointer). The photo stays above it. */
const BAR_BAND = 60
const PHOTO_PADDING = 20
/** Full screen: the bar comes back on a mouse move and goes after this idle time. */
const CHROME_IDLE_MS = 1500

/** In full screen, whether the bar and the overlays show: after a mouse move, until the mouse rests. */
function useIdleChrome(fullscreen: boolean) {
  const [awake, setAwake] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  const overBar = useRef(false)
  const sleepLater = useCallback(() => {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      if (!overBar.current) setAwake(false)
    }, CHROME_IDLE_MS)
  }, [])
  const wake = useCallback(() => {
    if (!fullscreen) return
    setAwake(true)
    sleepLater()
  }, [fullscreen, sleepLater])
  // Entering full screen shows the photo alone.
  useEffect(() => {
    setAwake(false)
    overBar.current = false
    return () => window.clearTimeout(timer.current)
  }, [fullscreen])
  const barHover = {
    onMouseEnter: () => {
      overBar.current = true
    },
    onMouseLeave: () => {
      overBar.current = false
      if (fullscreen) sleepLater()
    },
  }
  return { shown: !fullscreen || awake, wake, barHover }
}

/** Large preview of the focused photo with filmstrip and inspector; F shows the photo alone. */
export function Loupe() {
  const t = useT()
  const projectId = useReviewStore((s) => s.projectId)
  const focusId = useReviewStore((s) => s.focusId)
  const focusSlot = useReviewStore((s) => s.focusSlot)
  const photo = useReviewStore((s) => (s.focusId ? s.photos[s.focusId] : undefined))
  const ids = useReviewStore((s) => s.visibleIds)
  const group = useReviewStore((s) => {
    const gid = s.focusId ? s.photos[s.focusId]?.assessment?.groupId : null
    return gid === null || gid === undefined ? undefined : s.groups[gid]
  })
  // Place in its burst, in the grid's order, and the frame after it (preloaded while walking).
  const groupIndex = useReviewStore((s) => (s.focusId ? framePosition(group, s.focusId, s.filters.sort, s.search.hits, s) : -1))
  const nextFrame = useReviewStore((s) =>
    group && s.walkGroupId === group.id && groupIndex >= 0 ? groupOrder(group, effectiveSort(s.filters.sort, s.search.hits), s)[groupIndex + 1] : undefined,
  )
  const move = useReviewStore((s) => s.move)
  const zoom = useWorkspaceStore((s) => s.zoom)
  const setZoom = useWorkspaceStore((s) => s.setZoom)
  const resetZoom = useWorkspaceStore((s) => s.resetZoom)
  const close = useWorkspaceStore((s) => s.closeOverlay)
  const fullscreen = useWorkspaceStore((s) => s.fullscreen)
  const toggleFullscreen = useWorkspaceStore((s) => s.toggleFullscreen)
  const inspectorOpen = usePrefsStore((s) => s.inspectorOpen)
  const toggleInspector = usePrefsStore((s) => s.toggleInspector)
  const [hoverFace, setHoverFace] = useState<number | null>(null)
  const chrome = useIdleChrome(fullscreen)
  const index = focusSlot ? ids.indexOf(focusSlot) : -1
  const url = useLargePreview(projectId, focusId, [nextFrame, ids[index + 1], ids[index - 1], ids[index + 2]])

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
  const best = isBest(group, photo.id)
  const fullscreenLabel = fullscreen ? t('loupe.exitFullscreen') : t('loupe.fullscreen')

  return (
    <div className="flex min-h-0 flex-1 animate-fade-in">
      <div className="flex min-w-0 flex-1 flex-col">
        <div onMouseMove={chrome.wake} className={cn('group/loupe relative min-h-0 flex-1 bg-[var(--loupe-bg)]', !chrome.shown && '[&_*]:!cursor-none')}>
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
            // Full screen: the photo alone, the bar only passes over it.
            padding={fullscreen ? 0 : PHOTO_PADDING}
            band={fullscreen ? 0 : BAR_BAND}
            label={photo.fileName}
          />
          {!url && chrome.shown && (
            <span className="pointer-events-none absolute left-1/2 top-4 -translate-x-1/2 rounded-full bg-black/50 px-3 py-1 text-xs text-white/80 backdrop-blur-md">{t('loupe.loading')}</span>
          )}
          {!zoom.active && chrome.shown && (
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
          {chrome.shown && (
            <div className="pointer-events-none absolute left-4 top-4 flex items-center gap-2">
              {zoom.active && <span className="rounded-full bg-black/55 px-2.5 py-1 text-[11px] font-semibold text-white/90 backdrop-blur-md">{t('loupe.actual')}</span>}
              {/* Place in its burst: ⌥← ⌥→ go through every frame, the stack staying closed. */}
              {groupSize > 1 && groupIndex >= 0 && (
                <span
                  title={`${t('loupe.groupPosition', { index: formatNumber(groupIndex + 1), total: formatNumber(groupSize), keys: `${altLabel()}← ${altLabel()}→` })}${best ? ` · ${t('compare.best')}` : ''}`}
                  className="pointer-events-auto inline-flex h-6 items-center gap-1 rounded-full bg-black/55 pl-2 pr-2.5 text-[11px] font-semibold text-white/90 backdrop-blur-md"
                >
                  <Layers className="size-3" aria-hidden />
                  <span className="tabular">
                    {formatNumber(groupIndex + 1)}/{formatNumber(groupSize)}
                  </span>
                  {best && <Award className="size-3 text-[#c3b9fa]" aria-label={t('compare.best')} />}
                </span>
              )}
            </div>
          )}
          <MarkFlash />

          {/* One floating bar: back, photo, keep / reject, zoom, full screen, details.
              The wrapper fades it in full screen (the bar's entrance animation holds its own opacity). */}
          <div
            {...chrome.barHover}
            data-loupe-bar
            inert={!chrome.shown}
            className={cn('absolute bottom-3 left-1/2 z-10 -translate-x-1/2 transition-opacity duration-300', chrome.shown ? 'opacity-100' : 'pointer-events-none opacity-0')}
          >
            <div className="glass-strong flex items-center gap-1 rounded-full p-1.5 pl-2 animate-rise-in">
              <Tooltip content={`${t('loupe.backToGrid')} (Esc)`} delay={400} closeDelay={0}>
                <button type="button" aria-label={t('loupe.backToGrid')} onClick={close} className="focus-ring relative flex size-9 items-center justify-center after:absolute after:-inset-0.5 rounded-full text-white/70 hover:bg-white/10 hover:text-white">
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
                className={cn('focus-ring relative h-9 rounded-full px-3 after:absolute after:-inset-0.5 text-[13px] font-medium transition-colors', zoom.active ? 'bg-white text-[#141414]' : 'text-white/70 hover:bg-white/10 hover:text-white')}
              >
                {zoom.active ? t('loupe.actual') : t('loupe.fit')}
              </button>
              <Tooltip content={`${fullscreenLabel} (F)`} delay={400} closeDelay={0}>
                <button
                  type="button"
                  aria-label={fullscreenLabel}
                  aria-pressed={fullscreen}
                  aria-keyshortcuts="F"
                  onClick={toggleFullscreen}
                  className="focus-ring relative flex size-9 items-center justify-center after:absolute after:-inset-0.5 rounded-full text-white/60 transition-colors hover:bg-white/10 hover:text-white"
                >
                  {fullscreen ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
                </button>
              </Tooltip>
              {!fullscreen && (
                <Tooltip content={t('inspector.title')} delay={400} closeDelay={0}>
                  <button
                    type="button"
                    aria-label={t('inspector.title')}
                    aria-pressed={inspectorOpen}
                    onClick={toggleInspector}
                    className={cn('focus-ring relative flex size-9 items-center justify-center after:absolute after:-inset-0.5 rounded-full transition-colors', inspectorOpen ? 'bg-white/15 text-white' : 'text-white/60 hover:bg-white/10 hover:text-white')}
                  >
                    <PanelRight className="size-4" />
                  </button>
                </Tooltip>
              )}
            </div>
          </div>
        </div>
        {!fullscreen && <Filmstrip />}
      </div>
      {inspectorOpen && !fullscreen && (
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
