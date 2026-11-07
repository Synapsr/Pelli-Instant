import { useEffect, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { FolderOpen, FolderPlus } from 'lucide-react'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { useMediaQuery } from '@/hooks/use-media-query'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { ARRIVAL_S } from '@/lib/sound'
import { useAppStore } from '@/store/app-store'

/** Orb diameter, smaller on short windows so the recent shoots still fit. */
const ORB = 224
const ORB_SHORT = 184
const RADIUS = 32
const ICON_SWAP = {
  initial: { opacity: 0, scale: 0.25, filter: 'blur(4px)' },
  animate: { opacity: 1, scale: 1, filter: 'blur(0px)' },
  exit: { opacity: 0, scale: 0.25, filter: 'blur(4px)' },
  transition: { type: 'spring', duration: 0.3, bounce: 0 },
} as const

type ZoneState = 'idle' | 'over' | 'busy'

/** Text that changes with the state: soft blur-in, quicker exit. */
function SwapText({ id, children, className }: { id: string; children: string; className: string }) {
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      <motion.span
        key={id}
        initial={{ opacity: 0, y: 6, filter: 'blur(4px)' }}
        animate={{ opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.3, ease: [0.22, 1, 0.36, 1] } }}
        exit={{ opacity: 0, y: -6, filter: 'blur(4px)', transition: { duration: 0.15, ease: 'easeIn' } }}
        className={className}
      >
        {children}
      </motion.span>
    </AnimatePresence>
  )
}

/**
 * The outline of the drop target: hidden at rest, a faint dashed frame when
 * the pointer (or keyboard focus) is on the zone, violet with marching
 * dashes while a folder is dragged over the window.
 */
function DropFrame({ state }: { state: ZoneState }) {
  return (
    <svg
      aria-hidden
      className={cn(
        'pointer-events-none absolute inset-0 size-full overflow-visible transition-[opacity,scale] ease-[cubic-bezier(0.2,0,0,1)]',
        state === 'over' && 'scale-100 opacity-100 duration-300',
        state === 'idle' && 'scale-[0.985] opacity-0 duration-200 group-hover:scale-100 group-hover:opacity-100 group-hover:duration-300 group-focus-visible:scale-100 group-focus-visible:opacity-100',
        state === 'busy' && 'scale-[0.985] opacity-0 duration-200',
      )}
    >
      <rect
        width="100%"
        height="100%"
        rx={RADIUS}
        strokeWidth={1.5}
        strokeDasharray="6 10"
        strokeLinecap="round"
        className={cn(
          'transition-[stroke,fill] duration-300',
          state === 'over' ? 'fill-[#5840f0]/[0.07] stroke-[#a599f5]/60 [animation:dash-march_1.6s_linear_infinite]' : 'fill-white/[0.02] stroke-white/[0.16]',
        )}
      />
    </svg>
  )
}

/**
 * The one action of the home screen: a liquid orb that reaches for the
 * pointer, in a zone that outlines itself on hover. Drop a shoot folder on
 * the window, or click anywhere in the zone to choose one; the analysis
 * starts right away.
 */
/** The orb's arrival: it comes from afar (tiny, out of focus) and lands at ARRIVAL_S. */
const ARRIVAL = { duration: ARRIVAL_S + 0.4, ease: [0.18, 0.9, 0.22, 1] } as const

/** A thin ring spreading from the orb as it lands, once. */
function LandingRipple() {
  return (
    <motion.span
      aria-hidden
      className="pointer-events-none absolute inset-0 rounded-full border border-[#a599f5]/60"
      initial={{ opacity: 0, scale: 0.92 }}
      animate={{ opacity: [0, 0.7, 0], scale: [0.92, 1.18, 1.7] }}
      transition={{ duration: 1.3, delay: ARRIVAL_S - 0.08, ease: [0.22, 1, 0.36, 1], times: [0, 0.2, 1] }}
    />
  )
}

export function DropZone({ over, intro = false }: { over: boolean; intro?: boolean }) {
  const t = useT()
  // The surface ripples as the orb lands.
  const [landing, setLanding] = useState(false)
  useEffect(() => {
    if (!intro) return
    const start = window.setTimeout(() => setLanding(true), ARRIVAL_S * 1000 - 60)
    const end = window.setTimeout(() => setLanding(false), ARRIVAL_S * 1000 + 520)
    return () => {
      window.clearTimeout(start)
      window.clearTimeout(end)
    }
  }, [intro])
  const opening = useAppStore((s) => s.opening)
  const pickAndOpen = useAppStore((s) => s.pickAndOpen)
  const [hovered, setHovered] = useState(false)
  const orb = useMediaQuery('(max-height: 820px)') ? ORB_SHORT : ORB
  const busy = opening !== null
  const state: ZoneState = busy ? 'busy' : over ? 'over' : 'idle'

  return (
    // aria-disabled rather than disabled: a disabled button gets no pointer
    // events, and the hover state would stick when the work ends.
    <motion.button
      type="button"
      onClick={() => {
        if (!busy) void pickAndOpen()
      }}
      aria-disabled={busy}
      aria-label={t('home.chooseFolder')}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      whileTap={busy ? undefined : 'pressed'}
      className={cn(
        'group focus-ring relative flex w-[560px] max-w-full flex-col items-center px-12 pb-8 pt-10 text-center outline-none short:pb-5 short:pt-6',
        busy ? 'cursor-progress' : 'cursor-pointer',
      )}
      style={{ borderRadius: RADIUS }}
    >
      <DropFrame state={state} />

      <motion.div
        className="relative flex items-center justify-center"
        style={{ width: orb, height: orb }}
        initial="hidden"
        animate="shown"
        variants={{
          // No CSS blur here: a filter would clip the orb's halo (drawn past this box) to a square.
          hidden: intro ? { opacity: 0, scale: 0.06 } : { opacity: 0, scale: 0.9 },
          shown: { opacity: 1, scale: 1, transition: intro ? ARRIVAL : { type: 'spring', duration: 0.6, bounce: 0 } },
          pressed: { scale: 0.96, transition: { type: 'spring', duration: 0.3, bounce: 0 } },
        }}
      >
        {intro && <LandingRipple />}
        <LiquidOrb size={orb} swell={over || landing} busy={busy} hovered={hovered && !busy} className="pointer-events-none absolute inset-0">
          <AnimatePresence mode="popLayout" initial={false}>
            {busy ? (
              <motion.span key="busy" {...ICON_SWAP} className="block size-12 rounded-full border-[2.5px] border-white/15 border-t-white/90 [animation:spin-slow_0.9s_linear_infinite]" />
            ) : over ? (
              <motion.span key="over" {...ICON_SWAP} className="text-white">
                <FolderPlus className="size-14" strokeWidth={1.4} />
              </motion.span>
            ) : (
              <motion.span key="idle" {...ICON_SWAP} className="text-white/90">
                <FolderOpen className="size-14" strokeWidth={1.4} />
              </motion.span>
            )}
          </AnimatePresence>
        </LiquidOrb>
      </motion.div>

      <motion.div
        className="mt-10 flex min-h-16 w-full flex-col items-center short:mt-7"
        aria-live="polite"
        initial="hidden"
        animate="visible"
        variants={{ visible: { transition: { staggerChildren: 0.1, delayChildren: intro ? ARRIVAL_S + 0.05 : 0.15 } } }}
      >
        {[
          <SwapText key="title" id={state} className="block text-balance text-[28px] font-semibold leading-tight tracking-[-0.02em] text-white">
            {busy ? t('home.opening') : over ? t('home.dropActive') : t('home.dropTitle')}
          </SwapText>,
          <SwapText key="subtitle" id={busy ? `busy:${opening}` : 'hint'} className="mt-2 block max-w-md truncate text-sm text-white/50 transition-colors duration-200 group-hover:text-white/70">
            {busy ? (opening ?? '') : t('home.dropSubtitle')}
          </SwapText>,
        ].map((child) => (
          <motion.div
            key={child.key}
            className="flex max-w-full justify-center"
            variants={{
              hidden: { opacity: 0, y: 12, filter: 'blur(4px)' },
              visible: { opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.5, ease: [0.22, 1, 0.36, 1] } },
            }}
          >
            {child}
          </motion.div>
        ))}
      </motion.div>
    </motion.button>
  )
}
