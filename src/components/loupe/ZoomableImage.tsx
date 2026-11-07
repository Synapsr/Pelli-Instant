import { useRef, useState, type PointerEvent } from 'react'
import { useElementSize } from '@/hooks/use-element-size'
import type { FaceView, NormRect } from '@/lib/contract'
import { cn } from '@/lib/cn'
import type { ZoomState } from '@/store/workspace-store'

const PREVIEW_EDGE = 2048
const PADDING = 24

export interface ViewTransform {
  scale: number
  tx: number
  ty: number
  width: number
  height: number
}

/** Fit / 100 % transform keeping the zoom centre in view (clamped to the image). */
export function computeTransform(cw: number, ch: number, iw: number, ih: number, zoom: ZoomState, padding = PADDING): ViewTransform {
  const fit = Math.min((cw - padding * 2) / iw, (ch - padding * 2) / ih)
  // "100 %": one preview pixel per CSS pixel (at least 2.5× the fitted size).
  const actual = Math.max(1, fit * 2.5)
  const scale = zoom.active ? actual : Math.max(0.01, fit)
  const width = iw * scale
  const height = ih * scale
  const place = (container: number, size: number, center: number) => {
    if (!zoom.active || size <= container) return (container - size) / 2
    const t = container / 2 - center * size
    return Math.min(0, Math.max(container - size, t))
  }
  return { scale, width, height, tx: place(cw, width, zoom.x), ty: place(ch, height, zoom.y) }
}

/** Preview box of a photo in CSS px (long edge = preview size), from its aspect. */
export function previewBox(width: number | null, height: number | null): { iw: number; ih: number } {
  const aspect = width && height ? width / height : 3 / 2
  return aspect >= 1 ? { iw: PREVIEW_EDGE, ih: PREVIEW_EDGE / aspect } : { iw: PREVIEW_EDGE * aspect, ih: PREVIEW_EDGE }
}

/**
 * Large preview with click-to-zoom (100 %) and drag-to-pan. The zoom state is
 * owned by the caller, so compare panes can share it (synced zoom/pan).
 */
export function ZoomableImage({
  src,
  fallbackSrc,
  photoWidth,
  photoHeight,
  zoom,
  onZoom,
  dim = false,
  highlight,
  faces = [],
  padding = PADDING,
  label,
}: {
  src: string | null
  fallbackSrc: string | null
  photoWidth: number | null
  photoHeight: number | null
  zoom: ZoomState
  onZoom: (zoom: Partial<ZoomState>) => void
  dim?: boolean
  highlight?: number | null
  faces?: FaceView[]
  padding?: number
  label: string
}) {
  const ref = useRef<HTMLDivElement>(null)
  const { width: cw, height: ch } = useElementSize(ref)
  const drag = useRef<{ x: number; y: number; cx: number; cy: number; moved: boolean } | null>(null)
  const [dragging, setDragging] = useState(false)
  const { iw, ih } = previewBox(photoWidth, photoHeight)
  const view = cw && ch ? computeTransform(cw, ch, iw, ih, zoom, padding) : null
  const shown = src ?? fallbackSrc

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    drag.current = { x: e.clientX, y: e.clientY, cx: zoom.x, cy: zoom.y, moved: false }
    e.currentTarget.setPointerCapture(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d || !view || !zoom.active) return
    const dx = e.clientX - d.x
    const dy = e.clientY - d.y
    if (!d.moved && Math.hypot(dx, dy) < 4) return
    d.moved = true
    setDragging(true)
    onZoom({ x: Math.min(1, Math.max(0, d.cx - dx / view.width)), y: Math.min(1, Math.max(0, d.cy - dy / view.height)) })
  }
  const onPointerUp = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    drag.current = null
    setDragging(false)
    if (!d || d.moved || !view) return
    if (zoom.active) {
      onZoom({ active: false })
      return
    }
    const rect = e.currentTarget.getBoundingClientRect()
    const u = (e.clientX - rect.left - view.tx) / view.width
    const v = (e.clientY - rect.top - view.ty) / view.height
    if (u < 0 || u > 1 || v < 0 || v > 1) return
    onZoom({ active: true, x: u, y: v })
  }

  const box = (r: NormRect) =>
    view && {
      left: view.tx + r.x * view.width,
      top: view.ty + r.y * view.height,
      width: r.w * view.width,
      height: r.h * view.height,
    }
  const highlighted = highlight !== null && highlight !== undefined ? faces[highlight] : undefined
  const hbox = highlighted ? box(highlighted.bbox) : null

  return (
    <div
      ref={ref}
      role="img"
      aria-label={label}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        drag.current = null
        setDragging(false)
      }}
      className={cn('relative h-full w-full touch-none overflow-hidden', zoom.active ? (dragging ? 'cursor-grabbing' : 'cursor-grab') : 'cursor-zoom-in')}
    >
      {view && shown && (
        <img
          src={shown}
          alt=""
          draggable={false}
          className={cn('absolute left-0 top-0 max-w-none origin-top-left select-none', dim && 'opacity-60', !src && 'blur-[1.5px]')}
          style={{
            width: iw,
            height: ih,
            transform: `translate3d(${view.tx}px, ${view.ty}px, 0) scale(${view.scale})`,
            transition: dragging ? 'none' : 'transform 280ms cubic-bezier(0.22, 1, 0.36, 1), opacity 200ms',
            boxShadow: '0 12px 40px rgba(0,0,0,0.25)',
          }}
        />
      )}
      {hbox && (
        <span
          aria-hidden
          className="pointer-events-none absolute rounded-md border-2 border-[#a599f5] shadow-[0_0_0_9999px_rgba(0,0,0,0.28)] transition-all duration-200"
          style={{ left: hbox.left - 4, top: hbox.top - 4, width: hbox.width + 8, height: hbox.height + 8 }}
        />
      )}
    </div>
  )
}
