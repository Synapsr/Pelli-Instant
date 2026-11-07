import { Check, X } from 'lucide-react'
import type { PhotoView } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { useReviewStore } from '@/store/review-store'

/**
 * Keep / Reject of the focused photo, also on ↑ / ↓ (P / X for Lightroom
 * hands). Stars (1-5) and colour labels (6-9) stay on the keyboard, out of
 * the way of everyone else.
 */
export function MarkControls({ photo }: { photo: PhotoView }) {
  const t = useT()
  const mark = useReviewStore((s) => s.mark)
  const ids = [photo.id]
  const picked = photo.mark.flag === 'pick'
  const rejected = photo.mark.flag === 'reject'
  const pill = 'focus-ring inline-flex h-9 items-center gap-1.5 rounded-full pl-3 pr-3.5 text-[13px] font-semibold transition-[background-color,color,box-shadow,scale] duration-200 ease-out active:scale-[0.96]'
  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        aria-pressed={picked}
        aria-keyshortcuts="ArrowUp P"
        onClick={() => void mark({ flag: picked ? 'none' : 'pick' }, { ids, advance: false })}
        className={cn(pill, picked ? 'bg-success text-[#0b1f10] shadow-[0_4px_16px_rgba(52,199,89,0.35)]' : 'text-white/75 hover:bg-white/10 hover:text-white')}
      >
        <Check className="size-4" strokeWidth={2.6} />
        {t('marks.pick')}
        <kbd aria-hidden className="font-sans text-[11px] font-medium opacity-55">↑</kbd>
      </button>
      <button
        type="button"
        aria-pressed={rejected}
        aria-keyshortcuts="ArrowDown X"
        onClick={() => void mark({ flag: rejected ? 'none' : 'reject' }, { ids, advance: false })}
        className={cn(pill, rejected ? 'bg-danger text-[#2a0705] shadow-[0_4px_16px_rgba(255,91,79,0.35)]' : 'text-white/75 hover:bg-white/10 hover:text-white')}
      >
        <X className="size-4" strokeWidth={2.6} />
        {t('marks.reject')}
        <kbd aria-hidden className="font-sans text-[11px] font-medium opacity-55">↓</kbd>
      </button>
    </div>
  )
}
