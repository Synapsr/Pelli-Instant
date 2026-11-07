import { Button } from '@heroui/react'
import { AnimatePresence, motion, useReducedMotion, useSpring, useTransform, type Variants } from 'framer-motion'
import { ArrowRight, Check, ExternalLink, Share, WifiOff } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { PlanCards } from '@/components/plans/PlanCards'
import { AnimatedNumber } from '@/components/ui/AnimatedNumber'
import { Backdrop } from '@/components/ui/Backdrop'
import { DECISION_BG } from '@/components/ui/Decision'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { InstantLogo } from '@/components/ui/Logo'
import { ReasonIcon } from '@/components/ui/ReasonIcon'
import { WindowBar } from '@/components/ui/WindowBar'
import { UpdateNotice } from '@/components/update/UpdateNotice'
import { useMediaQuery } from '@/hooks/use-media-query'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import type { DecisionCounts, FinalDecision, Progress, Reason } from '@/lib/contract'
import { formatDuration, formatLongDay, formatNumber, formatPercent } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { billingUrl, upgradesFor } from '@/lib/plans'
import { type RecentPhoto, useAnalysisStore } from '@/store/analysis-store'
import { useAppStore } from '@/store/app-store'
import { useReviewStore } from '@/store/review-store'

const EASE = [0.22, 1, 0.36, 1] as const
const DECISIONS: FinalDecision[] = ['selected', 'toReview', 'rejected']

/** Glow of each pile, in the dark theme's verdict colours. */
const GLOW: Record<FinalDecision, string> = {
  selected: 'rgba(52,199,89,0.55)',
  toReview: 'rgba(255,179,64,0.5)',
  rejected: 'rgba(255,91,79,0.45)',
}

/** Size of a photo card in the piles (and of the photos flying to them). */
const CARD_W = 124
const CARD_H = 88
/** Photos kept visible on each pile. */
const PILE_DEPTH = 3
/** At most one photo takes off every FLIGHT_EVERY_MS; the others land directly. */
const FLIGHT_EVERY_MS = 170
const FLIGHT_QUEUE = 3

const PHOTO_FRAME = 'overflow-hidden shadow-[0_18px_40px_-18px_rgba(0,0,0,0.9)] outline outline-1 -outline-offset-1 outline-white/10'

/** Overall progress: reading 0–5 %, analysis 5–95 %, grouping 95–100 %. */
function overall(p: Progress | null): number {
  if (!p) return 0
  const f = p.total ? p.done / p.total : 0
  if (p.stage === 'scanning') return f * 0.05
  if (p.stage === 'loading') return 0.05
  if (p.stage === 'analyzing') return 0.05 + f * 0.9
  if (p.stage === 'grouping') return 0.95 + f * 0.05
  return 1
}

const BLOCK: Variants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.08, delayChildren: 0.05 } },
  exit: { opacity: 0, y: -8, filter: 'blur(4px)', transition: { duration: 0.15, ease: [0.4, 0, 1, 1] } },
}
const RISE: Variants = {
  hidden: { opacity: 0, y: 10, filter: 'blur(6px)' },
  show: { opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.5, ease: EASE } },
}
const WORDS: Variants = { hidden: {}, show: { transition: { staggerChildren: 0.05 } } }

/** Headline revealed word by word. */
function Headline({ children }: { children: string }) {
  const words = children.split(' ')
  return (
    <motion.h1 variants={WORDS} aria-label={children} className="text-balance text-[34px] font-semibold leading-tight tracking-[-0.03em] text-white short:text-[28px]">
      {words.map((word, i) => (
        <span key={i} aria-hidden>
          <motion.span variants={RISE} className="inline-block">
            {word}
          </motion.span>
          {i < words.length - 1 && ' '}
        </span>
      ))}
    </motion.h1>
  )
}

// ---- The orb -------------------------------------------------------------------------

/** Sparks flying out of the ring when the analysis ends. */
function Sparks({ radius }: { radius: number }) {
  const sparks = useMemo(() => Array.from({ length: 16 }, (_, i) => ({ angle: (i / 16) * Math.PI * 2 + (i % 2) * 0.18, distance: radius + 26 + (i % 3) * 18, size: 3 + (i % 3) * 2 })), [radius])
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0">
      {sparks.map((s, i) => (
        <motion.span
          key={i}
          className={cn('absolute left-1/2 top-1/2 rounded-full', i % 3 === 0 ? 'bg-[#a599f5]' : i % 3 === 1 ? 'bg-[#7c6af0]' : 'bg-white/80')}
          style={{ width: s.size, height: s.size, marginLeft: -s.size / 2, marginTop: -s.size / 2 }}
          initial={{ x: Math.cos(s.angle) * radius, y: Math.sin(s.angle) * radius, opacity: 0, scale: 0.4 }}
          animate={{ x: Math.cos(s.angle) * s.distance, y: Math.sin(s.angle) * s.distance, opacity: [0, 1, 0], scale: [0.4, 1, 0.6] }}
          transition={{ duration: 1, ease: EASE, delay: 0.08 + (i % 4) * 0.03 }}
        />
      ))}
    </div>
  )
}

/**
 * The liquid orb of the home screen, now at work: a fine ring around it
 * fills with the progress (a glowing head leads the way), the percentage
 * sits in the orb. When it's over, the orb swells, the ring closes and
 * the percentage turns into a check.
 */
function OrbProgress({ value, done, orb }: { value: number; done: boolean; orb: number }) {
  const side = orb + 64
  const stroke = 3
  const r = side / 2 - 8
  const spring = useSpring(0, { stiffness: 55, damping: 20 })
  const rotate = useTransform(spring, (v) => v * 360)
  const [swell, setSwell] = useState(false)

  useEffect(() => spring.set(Math.max(0.012, value)), [spring, value])
  useEffect(() => {
    if (!done) return
    setSwell(true)
    const timer = window.setTimeout(() => setSwell(false), 700)
    return () => window.clearTimeout(timer)
  }, [done])

  return (
    <div className="relative flex items-center justify-center" style={{ width: side, height: side }}>
      <svg width={side} height={side} className="absolute inset-0 -rotate-90" aria-hidden>
        <defs>
          <linearGradient id="analysis-ring" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor="#a599f5" />
            <stop offset="100%" stopColor="#5840f0" />
          </linearGradient>
        </defs>
        <circle cx={side / 2} cy={side / 2} r={r} fill="none" stroke="rgba(255,255,255,0.07)" strokeWidth={stroke} />
        <motion.circle
          cx={side / 2}
          cy={side / 2}
          r={r}
          fill="none"
          stroke="url(#analysis-ring)"
          strokeWidth={stroke}
          strokeLinecap="round"
          style={{ pathLength: spring, filter: 'drop-shadow(0 0 6px rgba(124,106,240,0.8))' }}
        />
      </svg>
      {/* The head of the progress: a bright dot travelling around the ring. */}
      <motion.div aria-hidden className="pointer-events-none absolute inset-0" style={{ rotate }} animate={{ opacity: done ? 0 : 1 }} transition={{ duration: 0.3 }}>
        <span
          className="absolute left-1/2 size-[9px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_0_10px_3px_rgba(165,153,245,0.9),0_0_24px_6px_rgba(88,64,240,0.6)]"
          style={{ top: side / 2 - r }}
        />
      </motion.div>

      <LiquidOrb size={orb} busy={!done} swell={swell}>
        <AnimatePresence mode="popLayout" initial={false}>
          {done ? (
            <motion.span
              key="done"
              initial={{ scale: 0.25, opacity: 0, filter: 'blur(4px)' }}
              animate={{ scale: 1, opacity: 1, filter: 'blur(0px)' }}
              transition={{ type: 'spring', duration: 0.4, bounce: 0, delay: 0.1 }}
            >
              <Check className="size-16 text-white drop-shadow-[0_2px_12px_rgba(20,10,80,0.5)]" strokeWidth={2.4} />
            </motion.span>
          ) : (
            <motion.span
              key="percent"
              className="flex items-baseline drop-shadow-[0_2px_14px_rgba(20,10,80,0.45)]"
              exit={{ opacity: 0, scale: 0.8, filter: 'blur(4px)', transition: { duration: 0.15, ease: 'easeIn' } }}
            >
              <AnimatedNumber value={Math.round(value * 100)} duration={0.4} className="text-[52px] font-semibold leading-none tracking-[-0.04em] text-white short:text-[44px]" />
              <span className="ml-0.5 text-[20px] font-medium text-white/60">%</span>
            </motion.span>
          )}
        </AnimatePresence>
      </LiquidOrb>
      {done && <Sparks radius={r} />}
    </div>
  )
}

// ---- The piles -----------------------------------------------------------------------

interface Point {
  x: number
  y: number
}

interface Flight {
  photo: RecentPhoto
  from: Point
  to: Point
}

type Piles = Record<FinalDecision, RecentPhoto[]>

const centre = (r: DOMRect, box: DOMRect): Point => ({ x: r.left + r.width / 2 - box.left, y: r.top + r.height / 2 - box.top })

/**
 * Photos leaving the orb for their pile as the analysis hands its verdicts
 * out. Photos come in faster than they can fly on a quick machine: one
 * takes off every FLIGHT_EVERY_MS, the backlog lands directly.
 */
function useFlights(done: boolean) {
  const recent = useAnalysisStore((s) => s.recent)
  const reduce = useReducedMotion()
  const [piles, setPiles] = useState<Piles>({ selected: [], toReview: [], rejected: [] })
  const [flights, setFlights] = useState<Flight[]>([])
  const seen = useRef(new Set<string>())
  const queue = useRef<RecentPhoto[]>([])
  const stage = useRef<HTMLDivElement>(null)
  const source = useRef<HTMLDivElement>(null)
  const targets = useRef<Record<FinalDecision, HTMLDivElement | null>>({ selected: null, toReview: null, rejected: null })

  const land = useCallback((p: RecentPhoto) => {
    setPiles((s) => ({ ...s, [p.decision]: [...s[p.decision].filter((x) => x.id !== p.id), p].slice(-PILE_DEPTH) }))
  }, [])

  const landed = useCallback(
    (flight: Flight) => {
      setFlights((f) => f.filter((x) => x !== flight))
      land(flight.photo)
    },
    [land],
  )

  useEffect(() => {
    for (const p of recent) {
      if (seen.current.has(p.id)) continue
      seen.current.add(p.id)
      queue.current.push(p)
    }
  }, [recent])

  useEffect(() => {
    if (done) {
      queue.current.splice(0).forEach(land)
      return
    }
    const timer = window.setInterval(() => {
      const q = queue.current
      while (q.length > FLIGHT_QUEUE) land(q.shift()!)
      const next = q.shift()
      if (!next) return
      const box = stage.current?.getBoundingClientRect()
      const from = source.current?.getBoundingClientRect()
      const to = targets.current[next.decision]?.getBoundingClientRect()
      if (reduce || !box || !from || !to) return land(next)
      setFlights((f) => [...f, { photo: next, from: centre(from, box), to: centre(to, box) }])
    }, FLIGHT_EVERY_MS)
    return () => window.clearInterval(timer)
  }, [done, land, reduce])

  return { piles, flights, landed, stage, source, targets }
}

/** A photo on its way from the orb to its pile: born small and blurred in the orb, it grows and settles. */
function Flyer({ flight, onLanded }: { flight: Flight; onLanded: (flight: Flight) => void }) {
  const { from, to, photo } = flight
  const mid = { x: from.x + (to.x - from.x) * 0.45, y: from.y + (to.y - from.y) * 0.3 }
  return (
    <motion.div
      className={cn('absolute left-0 top-0 rounded-[14px] bg-white/5', PHOTO_FRAME)}
      style={{ width: CARD_W, height: CARD_H, marginLeft: -CARD_W / 2, marginTop: -CARD_H / 2 }}
      initial={{ x: from.x, y: from.y, scale: 0.15, opacity: 0, filter: 'blur(10px)' }}
      animate={{
        x: [from.x, mid.x, to.x],
        y: [from.y, mid.y, to.y],
        scale: [0.15, 1.12, 1],
        opacity: [0, 1, 1],
        filter: ['blur(10px)', 'blur(0px)', 'blur(0px)'],
      }}
      transition={{ duration: 0.9, times: [0, 0.4, 1], ease: ['easeOut', EASE] }}
      onAnimationComplete={() => onLanded(flight)}
    >
      <img src={photo.thumbUrl} alt="" draggable={false} className="size-full object-cover" />
    </motion.div>
  )
}

/** Depth placement of the cards in a pile: the latest on top, the others fanned behind. */
const DEPTH = [
  { rotate: 0, x: 0, y: 0, scale: 1, opacity: 1 },
  { rotate: -6, x: -8, y: 3, scale: 0.95, opacity: 0.85 },
  { rotate: 7, x: 9, y: 5, scale: 0.9, opacity: 0.6 },
]

function Pile({
  decision,
  photos,
  count,
  share,
  label,
  done,
  targetRef,
}: {
  decision: FinalDecision
  photos: RecentPhoto[]
  count: number
  share: number
  label: string
  done: boolean
  targetRef: (el: HTMLDivElement | null) => void
}) {
  const top = photos[photos.length - 1]?.id
  return (
    <motion.div variants={RISE} className="flex w-[148px] flex-col items-center">
      <div ref={targetRef} className="relative" style={{ width: CARD_W, height: CARD_H }}>
        {/* A soft glow of the verdict colour, pulsing as each photo lands. */}
        <motion.span
          key={top ?? 'empty'}
          aria-hidden
          className="pointer-events-none absolute -inset-6 rounded-[32px] blur-2xl"
          style={{ background: GLOW[decision] }}
          initial={{ opacity: top ? 0.55 : 0.12 }}
          animate={{ opacity: 0.16 }}
          transition={{ duration: 0.9, ease: 'easeOut' }}
        />
        {photos.length === 0 && <span aria-hidden className="absolute inset-0 rounded-[14px] border border-dashed border-white/[0.14] bg-white/[0.02]" />}
        <AnimatePresence initial={false}>
          {photos.map((p, i) => {
            const depth = DEPTH[photos.length - 1 - i] ?? DEPTH[DEPTH.length - 1]
            return (
              <motion.div
                key={p.id}
                className={cn('absolute inset-0 rounded-[14px] bg-white/5', PHOTO_FRAME)}
                initial={false}
                animate={depth}
                exit={{ opacity: 0, scale: 0.85, transition: { duration: 0.2 } }}
                transition={{ type: 'spring', duration: 0.5, bounce: 0 }}
                style={{ zIndex: i }}
              >
                <img src={p.thumbUrl} alt="" draggable={false} className={cn('size-full object-cover', decision === 'rejected' && 'opacity-60 grayscale-[55%]')} />
              </motion.div>
            )
          })}
        </AnimatePresence>
      </div>
      <AnimatedNumber value={count} duration={0.5} className="mt-5 text-[26px] font-semibold leading-none tracking-[-0.03em] text-white" />
      <span className="mt-1.5 flex items-center gap-1.5 text-[13px] text-white/55">
        <span className={cn('size-1.5 rounded-full', DECISION_BG[decision])} />
        {label}
        <AnimatePresence initial={false}>
          {done && (
            <motion.span
              className="tabular text-white/35"
              initial={{ opacity: 0, filter: 'blur(4px)' }}
              animate={{ opacity: 1, filter: 'blur(0px)' }}
              transition={{ duration: 0.4, delay: 0.5 }}
            >
              · {formatPercent(share)}
            </motion.span>
          )}
        </AnimatePresence>
      </span>
    </motion.div>
  )
}

/** Up to three photos of a decision, best first, for piles the flights didn't fill (fast runs). */
function bestOf(decision: FinalDecision): RecentPhoto[] {
  return Object.values(useReviewStore.getState().photos)
    .filter((p): p is typeof p & { thumbUrl: string } => p.decision === decision && p.thumbUrl !== null)
    .sort((a, b) => (a.assessment?.score ?? 0) - (b.assessment?.score ?? 0))
    .slice(-PILE_DEPTH)
    .map((p) => ({ id: p.id, thumbUrl: p.thumbUrl, decision }))
}

// ---- The screen ----------------------------------------------------------------------

const SHOWN_REASONS: Reason[] = ['blurry', 'motionBlur', 'eyesClosed', 'underexposed', 'overexposed', 'duplicate', 'subjectOutOfFocus']

function RunningText() {
  const t = useT()
  const progress = useAnalysisStore((s) => s.progress)
  // Says why this one takes longer: the Maximum models are at work.
  const maximum = useAppStore((s) => s.settings?.precision === 'maximum' && s.appInfo?.capabilities.maximum === true)
  const stage = progress?.stage ?? 'scanning'
  return (
    <motion.div key="running" variants={BLOCK} initial="hidden" animate="show" exit="exit" className="flex h-[76px] flex-col items-center">
      <motion.h1 variants={RISE} className="relative grid h-8 place-items-center text-[22px] font-semibold tracking-[-0.02em] text-white" aria-live="polite">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={stage}
            className="col-start-1 row-start-1 whitespace-nowrap"
            initial={{ opacity: 0, y: 8, filter: 'blur(4px)' }}
            animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
            exit={{ opacity: 0, y: -8, filter: 'blur(4px)', transition: { duration: 0.15 } }}
            transition={{ duration: 0.4, ease: EASE }}
          >
            {t(`analysis.stage.${stage}`)}
          </motion.span>
        </AnimatePresence>
      </motion.h1>
      <motion.p variants={RISE} className="tabular mt-2 h-5 text-sm text-white/50">
        {progress && progress.total > 0 && (
          <>
            {t('analysis.progress', { done: formatNumber(stage === 'analyzing' ? progress.done : progress.total), total: formatNumber(progress.total) })}
            {stage === 'analyzing' && progress.photosPerSecond > 0 && (
              <>
                <span className="mx-2 text-white/20">·</span>
                {progress.etaMs !== null ? t('analysis.eta', { duration: formatDuration(progress.etaMs) }) : t('analysis.etaUnknown')}
              </>
            )}
            {maximum && (
              <>
                <span className="mx-2 text-white/20">·</span>
                <span className="text-[#a599f5]">{t('analysis.maximum')}</span>
              </>
            )}
          </>
        )}
      </motion.p>
    </motion.div>
  )
}

function ResultText({ counts, total, durationMs }: { counts: DecisionCounts; total: number; durationMs: number }) {
  const t = useT()
  return (
    <motion.div key="result" variants={BLOCK} initial="hidden" animate="show" exit="exit" className="flex flex-col items-center text-center">
      <motion.p variants={RISE} className="text-[15px] font-medium text-[#a599f5]">
        {t('analysisScreen.doneTitle')}
      </motion.p>
      <div className="mt-1.5">
        <Headline>{t('analysisScreen.headline', { count: counts.selected })}</Headline>
      </div>
      <motion.p variants={RISE} className="mt-1.5 text-sm text-white/50">
        {durationMs < 1000 ? t('analysisScreen.sublineFast', { total: formatNumber(total) }) : t('analysisScreen.subline', { total: formatNumber(total), duration: formatDuration(durationMs) })}
      </motion.p>
    </motion.div>
  )
}

/**
 * The analysis, then its result, on one stage: the orb works, each photo
 * it judges flies to its pile (selected, to review, rejected); at the end
 * the orb turns into a check, steps back, and the summary comes in around
 * the same piles.
 */
function Session({ phase }: { phase: 'running' | 'result' }) {
  const t = useT()
  const done = phase === 'result'
  const short = useMediaQuery('(max-height: 820px)')
  const progress = useAnalysisStore((s) => s.progress)
  const summary = useAnalysisStore((s) => s.lastSummary)
  const cancel = useAnalysisStore((s) => s.cancel)
  const liveCounts = useAnalysisStore((s) => s.counts)
  const reviewResults = useAppStore((s) => s.reviewResults)
  const openDialog = useAppStore((s) => s.openDialog)
  const goHome = useAppStore((s) => s.goHome)
  const { piles, flights, landed, stage, source, targets } = useFlights(done)

  const counts: DecisionCounts = done && summary ? summary.counts : liveCounts
  const total = counts.selected + counts.toReview + counts.rejected
  const reasons = SHOWN_REASONS.map((r) => [r, summary?.reasons[r] ?? 0] as const)
    .filter(([, n]) => n > 0)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 4)

  // Fast runs leave some piles empty: the result shows the best photos instead.
  const shown = useMemo<Piles>(() => {
    if (!done) return piles
    const filled = { ...piles }
    for (const d of DECISIONS) if (filled[d].length === 0) filled[d] = bestOf(d)
    return filled
  }, [done, piles])

  useEffect(() => {
    if (!done) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Enter' && !useAppStore.getState().dialog) {
        e.preventDefault()
        reviewResults()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [done, reviewResults])

  const orb = short ? 156 : 196
  const ring = orb + 64
  const shrink = 0.72

  return (
    <motion.div
      ref={stage}
      className="relative flex w-full max-w-[680px] flex-col items-center"
      variants={BLOCK}
      initial="hidden"
      animate="show"
      exit="exit"
    >
      <motion.div
        className="flex items-center justify-center"
        initial={{ opacity: 0, scale: 0.9, filter: 'blur(10px)' }}
        animate={{ opacity: 1, scale: 1, filter: 'blur(0px)', height: done ? ring * shrink : ring }}
        transition={{ type: 'spring', duration: 0.8, bounce: 0 }}
      >
        <motion.div ref={source} animate={{ scale: done ? shrink : 1 }} transition={{ type: 'spring', duration: 0.8, bounce: 0, delay: done ? 0.35 : 0 }}>
          <OrbProgress value={done ? 1 : overall(progress)} done={done} orb={orb} />
        </motion.div>
      </motion.div>

      <div className="mt-7 short:mt-5">
        <AnimatePresence mode="wait" initial={false}>
          {done ? <ResultText key="result" counts={counts} total={total} durationMs={summary?.durationMs ?? 0} /> : <RunningText key="running" />}
        </AnimatePresence>
      </div>

      <motion.div variants={BLOCK} className="mt-9 flex items-start justify-center gap-10 short:mt-6">
        {DECISIONS.map((d) => (
          <Pile
            key={d}
            decision={d}
            photos={shown[d]}
            count={counts[d]}
            share={total ? counts[d] / total : 0}
            label={t(`review.tabs.${d}`)}
            done={done}
            targetRef={(el) => {
              targets.current[d] = el
            }}
          />
        ))}
      </motion.div>

      <div className="mt-9 flex min-h-12 flex-col items-center short:mt-6">
        <AnimatePresence mode="wait" initial={false}>
          {done ? (
            <motion.div key="actions" variants={BLOCK} initial="hidden" animate="show" exit="exit" className="flex flex-col items-center">
              {reasons.length > 0 && (
                <motion.ul variants={RISE} className="mb-7 flex flex-wrap justify-center gap-2 short:hidden" aria-label={t('review.sidebar.reasons')}>
                  {reasons.map(([reason, n]) => (
                    <li key={reason} className="inline-flex items-center gap-1.5 rounded-full bg-white/[0.05] px-3 py-1 text-xs text-white/60 ring-1 ring-white/[0.06]">
                      <ReasonIcon reason={reason} className="size-3.5 text-white/45" />
                      <span className="tabular font-semibold text-white/85">{formatNumber(n)}</span>
                      {t(`reasonPlural.${reason}`)}
                    </li>
                  ))}
                </motion.ul>
              )}
              <motion.div variants={RISE} className="flex items-center gap-3">
                <Button radius="full" className="btn-brand h-12 px-7 text-[15px] font-semibold" endContent={<ArrowRight className="size-4" />} onPress={reviewResults}>
                  {t('analysisScreen.review')}
                </Button>
                <Button
                  radius="full"
                  className="glass glass-hover h-12 px-6 text-[15px] font-medium text-white/90"
                  startContent={<Share className="size-4" />}
                  onPress={() => {
                    reviewResults()
                    openDialog('export')
                  }}
                >
                  {t('review.export')}
                </Button>
              </motion.div>
              <motion.button variants={RISE} type="button" onClick={goHome} className="focus-ring mt-3 rounded-full px-3 py-1.5 text-[13px] text-white/40 transition-colors hover:text-white/75">
                {t('analysisScreen.back')}
              </motion.button>
            </motion.div>
          ) : (
            <motion.button
              key="cancel"
              type="button"
              onClick={() => void cancel()}
              className="focus-ring rounded-full px-4 py-2 text-[13px] text-white/45 transition-colors hover:bg-white/5 hover:text-white/80 active:scale-[0.96]"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { delay: 0.6 } }}
              exit={{ opacity: 0, transition: { duration: 0.15 } }}
            >
              {t('analysis.cancel')}
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      <div aria-hidden className="pointer-events-none absolute inset-0 z-20">
        {flights.map((f) => (
          <Flyer key={f.photo.id} flight={f} onLanded={landed} />
        ))}
      </div>
    </motion.div>
  )
}

/**
 * The analysis needs more photos than the month has left, or a limited plan
 * is offline: say it plainly, show what's left and when it resets, and the
 * way out (the Pelli trial for free accounts, the plans for everyone).
 */
function QuotaReached() {
  const t = useT()
  const error = useAnalysisStore((s) => s.error)
  const project = useAnalysisStore((s) => s.project)
  const auth = useAppStore((s) => s.auth)
  const offers = useAppStore((s) => s.offers)
  const analyze = useAppStore((s) => s.analyze)
  const goHome = useAppStore((s) => s.goHome)
  const account = auth.status === 'signedIn' ? auth.account : null
  const details = error?.details
  const offline = error?.code === 'quotaOffline'
  const canTry = account?.plan === 'free' && account.trialAvailable === true && offers !== null
  const upgrades = offers ? upgradesFor(offers, account) : []
  const needed = formatNumber(details?.needed ?? 0)

  return (
    <motion.div key="quota" className="flex w-full max-w-[680px] flex-col items-center text-center" initial={{ opacity: 0, y: 12, filter: 'blur(4px)' }} animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }} transition={{ duration: 0.5, ease: EASE }}>
      {offline && (
        <span className="glass mb-5 flex size-14 items-center justify-center rounded-full">
          <WifiOff className="size-6 text-white/80" />
        </span>
      )}
      <h1 className="text-balance text-[28px] font-semibold tracking-[-0.02em] text-white">{t(offline ? 'quota.offlineTitle' : 'quota.exceededTitle')}</h1>
      <p className="mt-3 max-w-[520px] text-pretty text-[15px] leading-relaxed text-white/55">
        {offline
          ? t('quota.offlineBody', { plan: account?.planLabel ?? '' })
          : t('quota.exceededBody', {
              count: details?.needed ?? 0,
              needed,
              remaining: formatNumber(details?.remaining ?? 0),
              quota: formatNumber(details?.monthlyQuota ?? 0),
            })}
        {!offline && details?.resetsAt ? ` ${t('quota.resets', { date: formatLongDay(details.resetsAt) })}` : ''}
      </p>
      <p className="mt-2 text-[13px] text-[#a599f5]">{t(offline ? 'quota.offlineHint' : 'quota.unlimitedHint')}</p>
      {offers && upgrades.length > 0 && <PlanCards offers={offers} plans={upgrades} className="mt-7 w-full" />}
      <div className="mt-7 flex flex-wrap justify-center gap-3">
        {offline
          ? project && (
              <Button radius="full" className="btn-brand h-11 px-6 font-semibold" onPress={() => void analyze(project)}>
                {t('common.retry')}
              </Button>
            )
          : (
              <Button
                radius="full"
                className="btn-brand h-11 pl-6 pr-5 font-semibold"
                endContent={<ExternalLink className="size-3.5" />}
                onPress={() => void api().openExternal(canTry && offers ? offers.links.trial : (offers?.links.pricing ?? billingUrl(offers)))}
              >
                {canTry && offers ? t('quota.tryPelli', { days: formatNumber(offers.trialDays) }) : t('quota.seePlans')}
              </Button>
            )}
        <Button radius="full" className="glass glass-hover h-11 px-6 text-white/90" onPress={goHome}>
          {t('analysisScreen.back')}
        </Button>
      </div>
    </motion.div>
  )
}

function Failed() {
  const t = useT()
  const error = useAnalysisStore((s) => s.error)
  const project = useAnalysisStore((s) => s.project)
  const analyze = useAppStore((s) => s.analyze)
  const goHome = useAppStore((s) => s.goHome)
  if (error?.code === 'quotaExceeded' || error?.code === 'quotaOffline') return <QuotaReached />
  return (
    <motion.div key="failed" className="flex max-w-md flex-col items-center text-center" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
      <h1 className="text-[22px] font-semibold text-white">{t('analysis.errorTitle')}</h1>
      <p className="selectable mt-2 text-sm text-white/55">{error?.message}</p>
      <div className="mt-8 flex gap-3">
        {project && (
          <Button radius="full" className="btn-brand h-11 px-6 font-semibold" onPress={() => void analyze(project)}>
            {t('common.retry')}
          </Button>
        )}
        <Button radius="full" className="glass glass-hover h-11 px-6 text-white/90" onPress={goHome}>
          {t('analysisScreen.back')}
        </Button>
      </div>
    </motion.div>
  )
}

/** Full-window analysis: the orb at work and the photos sorted into piles, then the result. */
export function AnalysisScreen() {
  const project = useAnalysisStore((s) => s.project)
  const running = useAnalysisStore((s) => s.running)
  const summary = useAnalysisStore((s) => s.lastSummary)
  const error = useAnalysisStore((s) => s.error)
  const phase = running ? 'running' : summary ? 'result' : error ? 'failed' : 'running'

  return (
    <div className="dark relative flex h-full flex-col overflow-hidden text-white">
      <Backdrop />
      <WindowBar>
        <InstantLogo size="sm" className="text-white" />
        {project && <span className="ml-3 truncate text-[13px] text-white/45">{project.name}</span>}
        <UpdateNotice className="ml-auto" />
      </WindowBar>
      <main className="relative z-10 flex min-h-0 flex-1 flex-col items-center overflow-y-auto overflow-x-hidden px-8 py-6">
        <div className="my-auto flex w-full justify-center">
          <AnimatePresence mode="wait">{phase === 'failed' ? <Failed key="failed" /> : <Session key="session" phase={phase} />}</AnimatePresence>
        </div>
      </main>
    </div>
  )
}
