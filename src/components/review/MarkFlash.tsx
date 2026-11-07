import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import { Check, FlagOff, Star, X } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { LABEL_COLOR } from '@/components/ui/Marks'
import type { UserMark } from '@/lib/contract'
import { useT, type Translate } from '@/lib/i18n'
import { useReviewStore, type MarkFlash as Flash } from '@/store/review-store'

const SHOWN_MS = 800

function content(t: Translate, patch: Partial<UserMark>): ReactNode {
  if (patch.flag === 'pick') {
    return (
      <>
        <Check className="size-[18px] text-success" strokeWidth={2.6} aria-hidden />
        {t('marks.flash.picked')}
      </>
    )
  }
  if (patch.flag === 'reject') {
    return (
      <>
        <X className="size-[18px] text-danger" strokeWidth={2.6} aria-hidden />
        {t('marks.flash.rejected')}
      </>
    )
  }
  if (patch.flag === 'none') {
    return (
      <>
        <FlagOff className="size-4 text-white/60" aria-hidden />
        {t('marks.flash.unflagged')}
      </>
    )
  }
  if (patch.rating !== undefined) {
    if (patch.rating <= 0) return t('marks.noRating')
    return (
      <>
        <span aria-hidden className="flex gap-0.5 text-[#eab308]">
          {Array.from({ length: patch.rating }, (_, i) => (
            <Star key={i} className="size-[18px] fill-current" strokeWidth={0} />
          ))}
        </span>
        <span className="sr-only">{t('marks.stars', { count: patch.rating })}</span>
      </>
    )
  }
  if (patch.label !== undefined) {
    if (patch.label === 'none') return t('marks.noLabel')
    return (
      <>
        <span aria-hidden className="size-3 rounded-[3px]" style={{ background: LABEL_COLOR[patch.label] }} />
        {t(`marks.flash.labels.${patch.label}`)}
      </>
    )
  }
  return null
}

/**
 * Brief confirmation centred over the photos when one photo is marked ("Gardée",
 * "★★★", "Étiquette rouge"…), like Lightroom. Marks of a multi-selection get a
 * toast instead (review store).
 */
export function MarkFlash() {
  const t = useT()
  const flash = useReviewStore((s) => s.flash)
  const reduce = useReducedMotion()
  const [shown, setShown] = useState<Flash | null>(null)
  // Only the marks made while this view is up, not the last one when it mounts.
  const mountSeq = useRef(flash?.seq ?? 0)

  useEffect(() => {
    if (!flash || flash.seq <= mountSeq.current) return
    setShown(flash)
    const timer = window.setTimeout(() => setShown(null), SHOWN_MS)
    return () => window.clearTimeout(timer)
  }, [flash])

  return (
    <div role="status" className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center">
      <AnimatePresence initial={false}>
        {shown && (
          <motion.div
            key="flash"
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, transition: { duration: reduce ? 0 : 0.28, ease: 'easeOut' } }}
            transition={reduce ? { duration: 0 } : { duration: 0.14, ease: [0.22, 1, 0.36, 1] }}
            className="flex h-11 items-center gap-2 rounded-2xl bg-black/65 px-4 text-[14px] font-semibold text-white shadow-lg shadow-black/25 backdrop-blur-md"
          >
            {content(t, shown.patch)}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
