import { motion } from 'framer-motion'
import { Check, Star, X } from 'lucide-react'
import type { FinalDecision } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { EASE, PHOTO_FRAME, Photo } from './Scene'
import { PHOTOS } from './photos'

/** Stars and colour written for each decision by default (see the export settings). */
const MARKS: Record<FinalDecision, { stars: number; dot: string }> = {
  selected: { stars: 3, dot: 'bg-success' },
  toReview: { stars: 2, dot: 'bg-warning' },
  rejected: { stars: 0, dot: 'bg-danger' },
}

const LIBRARY: { src: string; decision: FinalDecision }[] = [
  { src: PHOTOS.dress, decision: 'selected' },
  { src: PHOTOS.portraitBlurry, decision: 'rejected' },
  { src: PHOTOS.couple, decision: 'selected' },
  { src: PHOTOS.rings, decision: 'toReview' },
  { src: PHOTOS.eyesClosed, decision: 'rejected' },
  { src: PHOTOS.dance, decision: 'selected' },
  { src: PHOTOS.groupDark, decision: 'rejected' },
  { src: PHOTOS.mountains, decision: 'toReview' },
]

const MARK_AT = 1
const MARK_STEP = 0.14
const DONE_AT = MARK_AT + LIBRARY.length * MARK_STEP + 0.3

/** The marks under one photo: stars filling one by one, or the reject flag, then the colour. */
function Marks({ decision, at }: { decision: FinalDecision; at: number }) {
  const { stars, dot } = MARKS[decision]
  return (
    <div className="mt-2 flex h-4 items-center justify-between px-0.5">
      {decision === 'rejected' ? (
        <motion.span
          className="flex size-4 items-center justify-center rounded-full bg-danger text-[#2a0705]"
          initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
          animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
          transition={{ type: 'spring', duration: 0.3, bounce: 0, delay: at }}
        >
          <X className="size-2.5" strokeWidth={3.5} />
        </motion.span>
      ) : (
        <span className="flex gap-0.5">
          {Array.from({ length: 5 }, (_, i) => (
            <span key={i} className="relative size-3">
              <Star className="absolute inset-0 size-3 text-white/15" fill="currentColor" strokeWidth={0} />
              {i < stars && (
                <motion.span
                  className="absolute inset-0"
                  initial={{ opacity: 0, scale: 0.25 }}
                  animate={{ opacity: 1, scale: 1 }}
                  transition={{ type: 'spring', duration: 0.3, bounce: 0, delay: at + i * 0.06 }}
                >
                  <Star className="size-3 text-white" fill="currentColor" strokeWidth={0} />
                </motion.span>
              )}
            </span>
          ))}
        </span>
      )}
      <motion.span
        className={cn('size-2.5 rounded-full', dot)}
        initial={{ opacity: 0, scale: 0.25 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ type: 'spring', duration: 0.3, bounce: 0, delay: at + 0.2 }}
      />
    </div>
  )
}

/**
 * Export step: a photo library window where, photo after photo, the stars,
 * reject flags and colour labels written by Pelli Instant appear, as
 * Lightroom, Capture One or Bridge show them.
 */
export function ExportVisual() {
  const t = useT()
  return (
    <div className="glass-strong w-[620px] overflow-hidden rounded-[22px] shadow-[0_40px_100px_-40px_rgba(0,0,0,0.9)] outline outline-1 -outline-offset-1 outline-white/10 short:scale-[0.8]">
      <div className="flex h-11 items-center gap-2 border-b border-white/[0.06] px-4">
        <span className="flex gap-1.5" aria-hidden>
          <span className="size-2.5 rounded-full bg-white/15" />
          <span className="size-2.5 rounded-full bg-white/15" />
          <span className="size-2.5 rounded-full bg-white/15" />
        </span>
        <span className="flex-1 text-center text-[13px] font-medium text-white/55">{t('onboarding.export.window')}</span>
        <motion.span
          className="inline-flex h-6 items-center gap-1 rounded-full bg-success/15 px-2.5 text-[11px] font-semibold text-success"
          initial={{ opacity: 0, x: 6, filter: 'blur(4px)' }}
          animate={{ opacity: 1, x: 0, filter: 'blur(0px)' }}
          transition={{ duration: 0.4, ease: EASE, delay: DONE_AT }}
        >
          <Check className="size-3" strokeWidth={3} />
          {t('onboarding.export.done')}
        </motion.span>
      </div>
      <div className="grid grid-cols-4 gap-x-3.5 gap-y-4 p-4">
        {LIBRARY.map((photo, i) => {
          const at = MARK_AT + i * MARK_STEP
          return (
            <motion.div
              key={photo.src}
              initial={{ opacity: 0, y: 10, filter: 'blur(4px)' }}
              animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
              transition={{ duration: 0.45, ease: EASE, delay: 0.3 + i * 0.05 }}
            >
              <motion.div
                className={cn('aspect-[3/2] rounded-[10px]', PHOTO_FRAME)}
                animate={photo.decision === 'rejected' ? { opacity: 0.4, filter: 'saturate(0.4)' } : undefined}
                transition={{ duration: 0.5, ease: 'easeOut', delay: at + 0.1 }}
              >
                <Photo src={photo.src} />
              </motion.div>
              <Marks decision={photo.decision} at={at} />
            </motion.div>
          )
        })}
      </div>
    </div>
  )
}
