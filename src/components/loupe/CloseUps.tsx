import { Eye, EyeOff, ScanFace } from 'lucide-react'
import type { FaceView, NormRect } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatPercent } from '@/lib/format'
import { useT } from '@/lib/i18n'

/** Square crop region around a face, in normalised image coordinates. */
export function faceRegion(bbox: NormRect, aspect: number, padding = 1.8): NormRect {
  const sidePx = Math.max(bbox.w * aspect, bbox.h) * padding // in image-height units
  const w = Math.min(1, sidePx / aspect)
  const h = Math.min(1, sidePx)
  const cx = bbox.x + bbox.w / 2
  const cy = bbox.y + bbox.h / 2
  return {
    x: Math.min(1 - w, Math.max(0, cx - w / 2)),
    y: Math.min(1 - h, Math.max(0, cy - h / 2)),
    w,
    h,
  }
}

function cropStyle(src: string, r: NormRect) {
  const px = r.w >= 1 ? 0 : (r.x / (1 - r.w)) * 100
  const py = r.h >= 1 ? 0 : (r.y / (1 - r.h)) * 100
  return {
    backgroundImage: `url("${src}")`,
    backgroundSize: `${100 / r.w}% ${100 / r.h}%`,
    backgroundPosition: `${px}% ${py}%`,
  }
}

/**
 * Close-ups of every detected face (cropped from the large preview) with the
 * eye state and focus of each, like Narrative's close-ups panel.
 */
export function CloseUps({
  faces,
  src,
  aspect,
  onFocusFace,
  onHover,
}: {
  faces: FaceView[]
  src: string | null
  aspect: number
  onFocusFace: (face: FaceView) => void
  onHover: (index: number | null) => void
}) {
  const t = useT()
  if (!faces.length) {
    return (
      <p className="flex items-center gap-2 text-[13px] text-default-400">
        <ScanFace className="size-4" aria-hidden />
        {t('inspector.noFaces')}
      </p>
    )
  }
  const shown = faces.slice(0, 6)
  return (
    <div>
      <ul className="grid grid-cols-3 gap-2">
        {shown.map((face, i) => {
          const closed = face.eyesClosed === true
          const soft = face.sharpness !== null && face.sharpness < 0.45
          return (
            <li key={i}>
              <button
                type="button"
                onClick={() => onFocusFace(face)}
                onMouseEnter={() => onHover(i)}
                onMouseLeave={() => onHover(null)}
                onFocus={() => onHover(i)}
                onBlur={() => onHover(null)}
                aria-label={`${t('inspector.face', { n: i + 1 })} · ${closed ? t('inspector.eyesClosed') : face.eyesClosed === false ? t('inspector.eyesOpen') : t('inspector.eyesUnknown')}`}
                className={cn(
                  'focus-ring group block w-full overflow-hidden rounded-lg text-left ring-1 transition-all duration-200 hover:-translate-y-0.5',
                  closed || soft ? 'ring-danger/50' : 'ring-default-200 dark:ring-white/10',
                )}
              >
                <span className="photo-well block aspect-square bg-no-repeat" style={src ? cropStyle(src, faceRegion(face.bbox, aspect)) : undefined} />
                <span className="flex items-center justify-between gap-1 bg-default-50 px-1.5 py-1 dark:bg-white/[0.03]">
                  <span className={cn('inline-flex items-center gap-1 text-[10px] font-medium', closed ? 'text-danger' : face.eyesClosed === false ? 'text-success' : 'text-default-400')}>
                    {closed ? <EyeOff className="size-3" aria-hidden /> : <Eye className="size-3" aria-hidden />}
                    {closed ? t('inspector.eyesClosed') : face.eyesClosed === false ? t('inspector.eyesOpen') : t('inspector.eyesUnknown')}
                  </span>
                </span>
                {face.sharpness !== null && (
                  <span className="flex items-center gap-1.5 bg-default-50 px-1.5 pb-1.5 dark:bg-white/[0.03]">
                    <span className="h-1 flex-1 overflow-hidden rounded-full bg-default-200 dark:bg-white/10">
                      <span className={cn('block h-full rounded-full', soft ? 'bg-danger/80' : 'bg-success/80')} style={{ width: `${face.sharpness * 100}%` }} />
                    </span>
                    <span className={cn('tabular text-[10px]', soft ? 'text-danger' : 'text-default-500')}>{formatPercent(face.sharpness)}</span>
                  </span>
                )}
              </button>
            </li>
          )
        })}
      </ul>
      {faces.length > shown.length && <p className="mt-2 text-xs text-default-400">+{faces.length - shown.length}</p>}
      <p className="mt-2 text-[11px] text-default-400">{t('inspector.closeUpsHint')}</p>
    </div>
  )
}
