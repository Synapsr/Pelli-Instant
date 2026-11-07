import { AnimatePresence, motion, type Variants } from 'framer-motion'
import { Check, X } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { playSound } from '@/lib/sound'
import { PHOTO_FRAME, Photo } from './Scene'
import { PHOTOS } from './photos'

type Arrow = 'ArrowUp' | 'ArrowDown' | 'ArrowLeft' | 'ArrowRight'
type Verdict = 'kept' | 'rejected'

const GLYPH: Record<Arrow, string> = { ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' }
const SHOOT = [PHOTOS.dress, PHOTOS.portraitBlurry, PHOTOS.dance, PHOTOS.eyesClosed, PHOTOS.portraitA, PHOTOS.couple, PHOTOS.rings]
/** The demo plays itself once if nobody touches a key: keep, then set aside. */
const DEMO: { at: number; arrow: Arrow }[] = [
  { at: 1400, arrow: 'ArrowUp' },
  { at: 2900, arrow: 'ArrowDown' },
]

/** Where the leaving card goes: up when kept, down when set aside, sideways otherwise. */
const CARD: Variants = {
  enter: { opacity: 0, scale: 0.92, y: 10, filter: 'blur(6px)' },
  center: { opacity: 1, scale: 1, x: 0, y: 0, rotate: 0, filter: 'blur(0px)', transition: { type: 'spring', duration: 0.45, bounce: 0 } },
  leave: (arrow: Arrow) => ({
    opacity: 0,
    filter: 'blur(4px)',
    ...(arrow === 'ArrowUp'
      ? { y: -90, rotate: -4, scale: 0.96 }
      : arrow === 'ArrowDown'
        ? { y: 90, rotate: 4, scale: 0.96 }
        : { x: arrow === 'ArrowLeft' ? 70 : -70, scale: 0.96 }),
    transition: { duration: 0.28, ease: [0.4, 0, 1, 1] },
  }),
}

function Keycap({ arrow, active, label, onPress }: { arrow: Arrow; active: boolean; label?: string; onPress: () => void }) {
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={label ?? GLYPH[arrow]}
      className={cn(
        'focus-ring flex size-14 items-center justify-center rounded-2xl text-xl font-medium transition-[background-color,color,box-shadow,scale] duration-150 ease-out',
        active
          ? 'scale-[0.94] bg-[#5840f0] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.25),0_8px_24px_-6px_rgba(88,64,240,0.9)]'
          : 'bg-white/[0.06] text-white/80 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.1),inset_0_-3px_0_rgba(0,0,0,0.3)] hover:bg-white/[0.1]',
      )}
    >
      {GLYPH[arrow]}
    </button>
  )
}

/**
 * Hands-on part of the keyboard step: real photos sorted with the arrows,
 * exactly like the loupe (↑ keep, ↓ set aside, ← → move). Keyboard or
 * clicks on the keycaps; it demonstrates itself once if left alone.
 */
export function KeysDemo({ onTried }: { onTried: () => void }) {
  const t = useT()
  const [index, setIndex] = useState(0)
  const [stamp, setStamp] = useState<Verdict | null>(null)
  const [pressed, setPressed] = useState<Arrow | null>(null)
  const [leaving, setLeaving] = useState<Arrow>('ArrowRight')
  const busy = useRef(false)
  const demo = useRef<number[]>([])

  const stopDemo = useCallback(() => {
    demo.current.forEach((id) => window.clearTimeout(id))
    demo.current = []
  }, [])

  const press = useCallback((arrow: Arrow) => {
    if (busy.current) return
    setPressed(arrow)
    window.setTimeout(() => setPressed((p) => (p === arrow ? null : p)), 170)
    setLeaving(arrow)
    if (arrow === 'ArrowLeft' || arrow === 'ArrowRight') {
      setIndex((i) => (i + (arrow === 'ArrowRight' ? 1 : SHOOT.length - 1)) % SHOOT.length)
      return
    }
    // Show the verdict on the photo, then let it fly off (as the loupe advances).
    busy.current = true
    playSound(arrow === 'ArrowUp' ? 'keep' : 'reject')
    setStamp(arrow === 'ArrowUp' ? 'kept' : 'rejected')
    window.setTimeout(() => {
      setStamp(null)
      setIndex((i) => (i + 1) % SHOOT.length)
      busy.current = false
    }, 420)
  }, [])

  const userPress = useCallback(
    (arrow: Arrow) => {
      stopDemo()
      press(arrow)
      if (arrow === 'ArrowUp' || arrow === 'ArrowDown') onTried()
    },
    [onTried, press, stopDemo],
  )

  useEffect(() => {
    demo.current = DEMO.map(({ at, arrow }) => window.setTimeout(() => press(arrow), at))
    return stopDemo
  }, [press, stopDemo])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (e.key in GLYPH) {
        e.preventDefault()
        userPress(e.key as Arrow)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [userPress])

  const next = (n: number) => SHOOT[(index + n) % SHOOT.length]

  return (
    <div className="flex items-center justify-center gap-20 short:scale-[0.8]">
      <div className="relative h-[320px] w-[240px]">
        {/* The next photos, waiting behind. */}
        <div aria-hidden className={cn('absolute inset-0 translate-x-5 rotate-[5deg] scale-[0.9] rounded-[24px] opacity-35', PHOTO_FRAME)}>
          <Photo src={next(2)} />
        </div>
        <div aria-hidden className={cn('absolute inset-0 translate-x-2.5 rotate-[2.5deg] scale-[0.95] rounded-[24px] opacity-60', PHOTO_FRAME)}>
          <Photo src={next(1)} />
        </div>
        <AnimatePresence initial={false} custom={leaving}>
          <motion.div
            key={index}
            custom={leaving}
            variants={CARD}
            initial="enter"
            animate="center"
            exit="leave"
            className={cn('absolute inset-0 rounded-[24px]', PHOTO_FRAME)}
          >
            <Photo src={SHOOT[index]} />
            <AnimatePresence>
              {stamp && (
                <motion.div
                  key={stamp}
                  className={cn(
                    'absolute inset-0 flex items-center justify-center rounded-[24px]',
                    stamp === 'kept' ? 'bg-success/25 shadow-[inset_0_0_0_3px_rgba(52,199,89,0.9)]' : 'bg-danger/25 shadow-[inset_0_0_0_3px_rgba(255,91,79,0.9)]',
                  )}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.15 }}
                >
                  <motion.span
                    className={cn(
                      'inline-flex h-9 items-center gap-1.5 rounded-full px-4 text-[14px] font-semibold shadow-lg',
                      stamp === 'kept' ? 'bg-success text-[#0b1f10]' : 'bg-danger text-[#2a0705]',
                    )}
                    initial={{ scale: 0.25, filter: 'blur(4px)' }}
                    animate={{ scale: 1, filter: 'blur(0px)' }}
                    transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
                  >
                    {stamp === 'kept' ? <Check className="size-4" strokeWidth={3} /> : <X className="size-4" strokeWidth={3} />}
                    {t(stamp === 'kept' ? 'onboarding.keys.kept' : 'onboarding.keys.rejected')}
                  </motion.span>
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>
        </AnimatePresence>
      </div>

      {/* Arrow cluster, laid out like the keyboard, with what each key does. */}
      <div className="flex flex-col items-center" role="group" aria-label={t('onboarding.keys.title')}>
        <span className="mb-2.5 text-[13px] font-medium text-success">{t('onboarding.keys.keep')}</span>
        <Keycap arrow="ArrowUp" label={t('onboarding.keys.keep')} active={pressed === 'ArrowUp'} onPress={() => userPress('ArrowUp')} />
        <div className="mt-2 flex items-center gap-2">
          <span className="mr-1.5 w-20 text-right text-[13px] text-white/45">{t('onboarding.keys.prev')}</span>
          <Keycap arrow="ArrowLeft" label={t('onboarding.keys.prev')} active={pressed === 'ArrowLeft'} onPress={() => userPress('ArrowLeft')} />
          <Keycap arrow="ArrowDown" label={t('onboarding.keys.reject')} active={pressed === 'ArrowDown'} onPress={() => userPress('ArrowDown')} />
          <Keycap arrow="ArrowRight" label={t('onboarding.keys.next')} active={pressed === 'ArrowRight'} onPress={() => userPress('ArrowRight')} />
          <span className="ml-1.5 w-20 text-[13px] text-white/45">{t('onboarding.keys.next')}</span>
        </div>
        <span className="mt-2.5 text-[13px] font-medium text-danger">{t('onboarding.keys.reject')}</span>
      </div>
    </div>
  )
}
