import { Check, Crown, X } from 'lucide-react'
import type { PhotoView } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { useReviewStore } from '@/store/review-store'

/**
 * Keep / Reject of the focused photo, also on ↑ / ↓ (P / X for Lightroom
 * hands). "Choose" (S) shows when another frame of its burst is selected:
 * keeping this one rejects it. Stars (1-5) and colour labels (6-9) stay on
 * the keyboard, out of the way of everyone else.
 */
export function MarkControls({ photo }: { photo: PhotoView }) {
  const t = useT()
  const mark = useReviewStore((s) => s.mark)
  const choose = useReviewStore((s) => s.choose)
  const rival = useReviewStore((s) => {
    const gid = photo.assessment?.groupId
    const group = gid === null || gid === undefined ? undefined : s.groups[gid]
    return group?.photoIds.some((id) => id !== photo.id && s.photos[id]?.decision === 'selected') ?? false
  })
  const ids = [photo.id]
  const picked = photo.mark.flag === 'pick'
  const rejected = photo.mark.flag === 'reject'
  // 36 px tall, 40 px to the pointer.
  const pill =
    'focus-ring relative inline-flex h-9 items-center gap-1.5 rounded-full pl-3 pr-3.5 text-[13px] font-semibold transition-[background-color,color,box-shadow,scale] duration-200 ease-out after:absolute after:-inset-y-0.5 after:inset-x-0 active:scale-[0.96]'
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
      {rival && (
        <button
          type="button"
          aria-keyshortcuts="S"
          title={`${t('marks.chooseHint')} (S)`}
          onClick={() => void choose({ id: photo.id, advance: false })}
          className={cn(pill, 'animate-fade-in bg-[#5840f0]/25 text-[#ddd7ff] hover:bg-[#5840f0]/40 hover:text-white')}
        >
          <Crown className="size-4" strokeWidth={2.2} />
          {t('marks.choose')}
          <kbd aria-hidden className="font-sans text-[11px] font-medium opacity-60">S</kbd>
        </button>
      )}
    </div>
  )
}
