import { motion } from 'framer-motion'
import { Check, X } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '@/lib/cn'
import { useT, type TKey } from '@/lib/i18n'
import { EASE, PHOTO_FRAME, Photo } from './Scene'
import { PHOTOS } from './photos'

/** Area the analysis looks at, in % of the frame. */
interface Focus {
  x: number
  y: number
  w: number
  h: number
}

/** When each card is scanned (s after the step appears) and how long a scan lasts. */
const SCAN_AT = 0.7
const SCAN_STEP = 0.45
const SCAN_FOR = 1

const scanEnd = (i: number) => SCAN_AT + i * SCAN_STEP + SCAN_FOR

/** A light sweeping the photo from top to bottom, once. */
function ScanBeam({ index }: { index: number }) {
  return (
    <motion.div
      aria-hidden
      className="pointer-events-none absolute inset-x-0 top-0 h-1/3"
      initial={{ y: '-100%', opacity: 0 }}
      animate={{ y: ['-100%', '300%'], opacity: [0, 1, 1, 0] }}
      transition={{ duration: SCAN_FOR, ease: 'easeInOut', delay: SCAN_AT + index * SCAN_STEP, times: [0, 0.15, 0.85, 1] }}
    >
      <div className="absolute inset-0 bg-gradient-to-b from-transparent via-[#7c6af0]/25 to-[#7c6af0]/45" />
      <div className="absolute inset-x-0 bottom-0 h-px bg-[#c9c1fb] shadow-[0_0_12px_2px_rgba(165,153,245,0.8)]" />
    </motion.div>
  )
}

/** Corner marks framing what the analysis found (a face, the eyes). */
function FocusMarks({ focus, index, tone }: { focus: Focus; index: number; tone: 'reject' | 'keep' }) {
  const corner = cn('absolute size-3.5 border-[2px]', tone === 'reject' ? 'border-[#ff8a80]' : 'border-[#7ee2a0]')
  return (
    <motion.div
      aria-hidden
      className="pointer-events-none absolute"
      style={{ left: `${focus.x}%`, top: `${focus.y}%`, width: `${focus.w}%`, height: `${focus.h}%` }}
      initial={{ opacity: 0, scale: 1.25 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ type: 'spring', duration: 0.45, bounce: 0, delay: scanEnd(index) - 0.25 }}
    >
      <span className={cn(corner, 'left-0 top-0 rounded-tl-md border-b-0 border-r-0')} />
      <span className={cn(corner, 'right-0 top-0 rounded-tr-md border-b-0 border-l-0')} />
      <span className={cn(corner, 'bottom-0 left-0 rounded-bl-md border-r-0 border-t-0')} />
      <span className={cn(corner, 'bottom-0 right-0 rounded-br-md border-l-0 border-t-0')} />
    </motion.div>
  )
}

/** Verdict under a card, popping in once its scan is over. */
function Verdict({ index, keep, label }: { index: number; keep: boolean; label: string }) {
  return (
    <motion.figcaption
      className="mt-4 flex items-center justify-center gap-2 text-[14px] font-medium text-white/85"
      initial={{ opacity: 0, y: 6, filter: 'blur(4px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      transition={{ duration: 0.4, ease: EASE, delay: scanEnd(index) }}
    >
      <motion.span
        className={cn('flex size-5 items-center justify-center rounded-full', keep ? 'bg-success text-[#0b1f10]' : 'bg-danger text-[#2a0705]')}
        initial={{ scale: 0.25, filter: 'blur(4px)' }}
        animate={{ scale: 1, filter: 'blur(0px)' }}
        transition={{ type: 'spring', duration: 0.3, bounce: 0, delay: scanEnd(index) }}
      >
        {keep ? <Check className="size-3" strokeWidth={3.2} /> : <X className="size-3" strokeWidth={3.2} />}
      </motion.span>
      {label}
    </motion.figcaption>
  )
}

/** One photo card: enters, gets scanned, then dims if it's set aside. */
function Card({ index, keep, label, children }: { index: number; keep: boolean; label: string; children: ReactNode }) {
  return (
    <motion.figure
      className="flex w-[178px] flex-col items-center"
      initial={{ opacity: 0, y: 24, filter: 'blur(6px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      transition={{ type: 'spring', duration: 0.8, bounce: 0, delay: 0.15 + index * 0.08 }}
    >
      <motion.div
        className="relative h-[238px] w-full"
        animate={keep ? undefined : { opacity: 0.62, filter: 'saturate(0.5)' }}
        transition={{ duration: 0.6, ease: 'easeOut', delay: scanEnd(index) + 0.35 }}
      >
        {children}
      </motion.div>
      <Verdict index={index} keep={keep} label={label} />
    </motion.figure>
  )
}

/** A single photo in a card, with its scan and, optionally, what was found. */
function Shot({ src, index, focus, keep }: { src: string; index: number; focus?: Focus; keep: boolean }) {
  return (
    <div className={cn('relative size-full rounded-[22px]', PHOTO_FRAME)}>
      <motion.div className="size-full" initial={{ scale: 1.08 }} animate={{ scale: 1 }} transition={{ duration: 6, ease: 'easeOut' }}>
        <Photo src={src} />
      </motion.div>
      <ScanBeam index={index} />
      {focus && <FocusMarks focus={focus} index={index} tone={keep ? 'keep' : 'reject'} />}
    </div>
  )
}

/** Three frames of a burst: stacked, then fanned out with the sharpest one in front. */
function Burst({ index }: { index: number }) {
  const at = scanEnd(index) - 0.2
  const frames = [
    { src: PHOTOS.portraitB, x: -20, rotate: -6 },
    { src: PHOTOS.portraitBlurry, x: 20, rotate: 6 },
  ]
  return (
    <div className="relative size-full">
      {frames.map((f) => (
        <motion.div
          key={f.src}
          className={cn('absolute inset-0 rounded-[22px]', PHOTO_FRAME)}
          initial={{ x: 0, rotate: 0, scale: 0.94, opacity: 0.9 }}
          animate={{ x: f.x, rotate: f.rotate, scale: 0.9, opacity: 0.5 }}
          transition={{ type: 'spring', duration: 0.7, bounce: 0, delay: at }}
        >
          <Photo src={f.src} />
        </motion.div>
      ))}
      <motion.div
        className="absolute inset-0 rounded-[22px]"
        initial={{ y: 0, scale: 1 }}
        animate={{ y: -6, scale: 1.03 }}
        transition={{ type: 'spring', duration: 0.7, bounce: 0, delay: at }}
      >
        <Shot src={PHOTOS.portraitA} index={index} keep />
        <motion.div
          aria-hidden
          className="pointer-events-none absolute inset-0 rounded-[22px] shadow-[0_0_0_2px_rgba(52,199,89,0.75),0_0_40px_-6px_rgba(52,199,89,0.55)]"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.5, delay: scanEnd(index) }}
        />
      </motion.div>
    </div>
  )
}

const LABELS: Record<'blurry' | 'eyes' | 'best' | 'dark', TKey> = {
  blurry: 'onboarding.eye.blurry',
  eyes: 'onboarding.eye.eyesClosed',
  best: 'onboarding.eye.best',
  dark: 'onboarding.eye.dark',
}

/**
 * "A photographer's eye": four real photos, each scanned in turn, with
 * what the analysis sees: a blurry face, closed eyes, the best frame of a
 * burst, a photo too dark to keep.
 */
export function EyeVisual() {
  const t = useT()
  return (
    <div className="flex items-start gap-10 short:scale-[0.8]">
      <Card index={0} keep={false} label={t(LABELS.blurry)}>
        <Shot src={PHOTOS.portraitBlurry} index={0} keep={false} focus={{ x: 22, y: 18, w: 60, h: 46 }} />
      </Card>
      <Card index={1} keep={false} label={t(LABELS.eyes)}>
        <Shot src={PHOTOS.eyesClosed} index={1} keep={false} focus={{ x: 30, y: 33, w: 30, h: 16 }} />
      </Card>
      <Card index={2} keep label={t(LABELS.best)}>
        <Burst index={2} />
      </Card>
      <Card index={3} keep={false} label={t(LABELS.dark)}>
        <Shot src={PHOTOS.groupDark} index={3} keep={false} />
      </Card>
    </div>
  )
}
