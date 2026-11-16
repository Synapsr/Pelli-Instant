import { Button } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { ArrowLeft, ArrowRight, Laptop, VolumeX, WifiOff, Zap, type LucideIcon } from 'lucide-react'
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Backdrop } from '@/components/ui/Backdrop'
import { InstantLogo } from '@/components/ui/Logo'
import { WindowBar } from '@/components/ui/WindowBar'
import { cn } from '@/lib/cn'
import { useLanguage, useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { usePrefsStore } from '@/store/prefs-store'
import { EyeVisual } from './EyeVisual'
import { ExportVisual } from './ExportVisual'
import { HeroVisual } from './HeroVisual'
import { KeysDemo } from './KeysDemo'
import { PrecisionVisual } from './PrecisionVisual'
import { ReadyStep } from './ReadyStep'
import { Aura } from './Aura'
import { preloadPhotos } from './photos'
import { EASE, Scene, STEP } from './Scene'
import { preloadNarration, useNarration } from './voice'

const STEPS = ['welcome', 'eye', 'keys', 'export', 'precision', 'ready'] as const
type Step = (typeof STEPS)[number]

/** The promise: free, unlimited, no account (the eyebrow), then three facts (copy facts of the 1.0 contract). */
const FACTS: { key: 'local' | 'offline' | 'fast'; icon: LucideIcon }[] = [
  { key: 'local', icon: Laptop },
  { key: 'offline', icon: WifiOff },
  { key: 'fast', icon: Zap },
]

function Welcome() {
  const t = useT()
  return (
    <Scene
      visual={<HeroVisual />}
      eyebrow={<span className="glass mb-5 inline-flex h-7 items-center rounded-full px-3 text-[12px] font-medium text-white/80">{t('onboarding.welcome.eyebrow')}</span>}
      title={t('onboarding.welcome.title')}
      body={t('onboarding.welcome.body')}
    >
      <ul className="mt-6 flex flex-wrap justify-center gap-2 short:mt-4">
        {FACTS.map(({ key, icon: Icon }, i) => (
          <motion.li
            key={key}
            initial={{ opacity: 0, y: 6, filter: 'blur(4px)' }}
            animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
            transition={{ duration: 0.45, ease: EASE, delay: 0.55 + i * 0.07 }}
            className="glass inline-flex h-9 items-center gap-2 rounded-full pl-3 pr-3.5 text-[13px] text-white/80"
          >
            <Icon className="size-3.5 text-[#b8adf8]" strokeWidth={2.2} aria-hidden />
            {t(`onboarding.welcome.facts.${key}`)}
          </motion.li>
        ))}
      </ul>
    </Scene>
  )
}

function Keys() {
  const t = useT()
  const [tried, setTried] = useState(false)
  return (
    <Scene visual={<KeysDemo onTried={useCallback(() => setTried(true), [])} />} title={t('onboarding.keys.title')} body={t('onboarding.keys.body')}>
      <div className="mt-4 h-5 text-[14px]" aria-live="polite">
        <AnimatePresence mode="wait" initial={false}>
          <motion.p
            key={tried ? 'tried' : 'try'}
            initial={{ opacity: 0, y: 4, filter: 'blur(4px)' }}
            animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
            exit={{ opacity: 0, y: -4, filter: 'blur(4px)', transition: { duration: 0.15 } }}
            className={tried ? 'text-white/70' : 'font-medium text-[#a599f5]'}
          >
            {t(tried ? 'onboarding.keys.tried' : 'onboarding.keys.try')}
          </motion.p>
        </AnimatePresence>
      </div>
    </Scene>
  )
}

const CONTENT: Record<Step, () => ReactNode> = {
  welcome: () => <Welcome />,
  eye: () => <EyeStep />,
  keys: () => <Keys />,
  export: () => <ExportStep />,
  precision: () => <PrecisionStep />,
  ready: () => <ReadyStep />,
}

function EyeStep() {
  const t = useT()
  return <Scene visual={<EyeVisual />} title={t('onboarding.eye.title')} body={t('onboarding.eye.body')} />
}

function ExportStep() {
  const t = useT()
  return <Scene visual={<ExportVisual />} title={t('onboarding.export.title')} body={t('onboarding.export.body')} />
}

function PrecisionStep() {
  const t = useT()
  return <Scene visual={<PrecisionVisual />} title={t('onboarding.precision.title')} body={t('onboarding.precision.body')} />
}

/** Three bars that dance while the voice speaks, still when it pauses. */
function VoiceBars({ speaking }: { speaking: boolean }) {
  return (
    <span aria-hidden className="flex h-4 items-center gap-[3px]">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="block h-full w-[3px] origin-center rounded-full bg-current transition-[scale] duration-300"
          style={speaking ? { animation: `voice-bar 0.9s ease-in-out ${i * 0.15}s infinite` } : { scale: `1 ${[0.35, 0.6, 0.35][i]}` }}
        />
      ))}
    </span>
  )
}

/** Discreet switch for the voice-over, next to the main button. */
function VoiceToggle({ speaking }: { speaking: boolean }) {
  const t = useT()
  const voice = usePrefsStore((s) => s.voice)
  const setVoice = usePrefsStore((s) => s.setVoice)
  const label = t(voice ? 'onboarding.voiceOff' : 'onboarding.voiceOn')
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={!voice}
      onClick={() => setVoice(!voice)}
      className="focus-ring flex size-10 items-center justify-center rounded-full text-white/35 transition-[color,background-color,scale] duration-200 hover:bg-white/[0.06] hover:text-white/80 active:scale-[0.96]"
    >
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={voice ? 'on' : 'off'}
          className="flex"
          initial={{ scale: 0.25, opacity: 0, filter: 'blur(4px)' }}
          animate={{ scale: 1, opacity: 1, filter: 'blur(0px)' }}
          exit={{ scale: 0.25, opacity: 0, filter: 'blur(4px)' }}
          transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
        >
          {voice ? <VoiceBars speaking={speaking} /> : <VolumeX className="size-[18px]" />}
        </motion.span>
      </AnimatePresence>
    </button>
  )
}

/** Segments that fill as the tour goes; each one jumps to its step. */
function Progress({ index, onGo }: { index: number; onGo: (to: number) => void }) {
  const t = useT()
  return (
    <div className="flex items-center gap-1.5" role="tablist" aria-label={t('onboarding.progress')}>
      {STEPS.map((s, i) => (
        <button
          key={s}
          type="button"
          role="tab"
          aria-selected={i === index}
          aria-label={t('onboarding.stepOf', { n: i + 1, total: STEPS.length })}
          onClick={() => onGo(i)}
          className="focus-ring group flex h-6 items-center rounded-full px-0.5"
        >
          <span className="relative block h-[3px] w-7 overflow-hidden rounded-full bg-white/15 transition-colors duration-200 group-hover:bg-white/25">
            <span
              className={cn(
                'absolute inset-0 origin-left rounded-full transition-[scale,background-color] duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]',
                i < index ? 'scale-x-100 bg-white/55' : i === index ? 'scale-x-100 bg-white' : 'scale-x-0 bg-white',
              )}
            />
          </span>
        </button>
      ))}
    </div>
  )
}

/**
 * The welcome, shown at the first launch (and from Settings → About). Apple
 * keynote style: the promise first (free, unlimited, no account, on your
 * computer), then one idea per step, shown live with real photos (what the
 * analysis catches, the keyboard, the export to Lightroom), the precision
 * (Standard free, Maximum with Plus), and ready to start.
 */
export function Onboarding() {
  const t = useT()
  const finishOnboarding = useAppStore((s) => s.finishOnboarding)
  // The statistics keep their current setting (on unless turned off in Settings › Privacy).
  const finish = useCallback(() => finishOnboarding(useAppStore.getState().settings?.telemetry ?? true), [finishOnboarding])
  const [index, setIndex] = useState(0)
  const step = STEPS[index]
  const last = index === STEPS.length - 1

  const language = useLanguage()
  const voice = usePrefsStore((s) => s.voice)

  useEffect(preloadPhotos, [])
  useEffect(() => preloadNarration(language), [language])
  const speaking = useNarration(step, language, voice)

  const go = useCallback(
    (to: number) => {
      if (to < 0) return
      if (to >= STEPS.length) return finish()
      setIndex(to)
    },
    [finish],
  )

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      const onControl = (e.target as HTMLElement | null)?.closest?.('button, a, input, pre, [role="radio"], [role="switch"]')
      if (e.key === 'Enter' && !onControl) {
        e.preventDefault()
        go(index + 1)
      }
      // The keyboard step uses the arrows for its demo.
      if (step !== 'keys' && !onControl && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
        e.preventDefault()
        go(index + (e.key === 'ArrowRight' ? 1 : -1))
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [go, index, step])

  return (
    <div className="dark relative flex h-full flex-col overflow-hidden text-white">
      <Backdrop>
        <Aura step={index} />
      </Backdrop>
      <WindowBar>
        <InstantLogo size="sm" className="text-white" />
        <AnimatePresence initial={false}>
          {!last && (
            <motion.div className="ml-auto" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0, transition: { duration: 0.15 } }}>
              {/* Skipping lands on the last step, which starts the app. */}
              <Button size="sm" radius="full" variant="light" className="text-white/50 hover:text-white" onPress={() => go(STEPS.length - 1)}>
                {t('onboarding.skip')}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </WindowBar>

      <main className="relative z-10 flex min-h-0 flex-1 flex-col items-center overflow-y-auto overflow-x-hidden px-8 py-4">
        <AnimatePresence mode="wait" initial={false}>
          <motion.section
            key={step}
            aria-label={t('onboarding.stepOf', { n: index + 1, total: STEPS.length })}
            variants={STEP}
            initial="hidden"
            animate="show"
            exit="exit"
            className="my-auto w-full"
          >
            {CONTENT[step]()}
          </motion.section>
        </AnimatePresence>
      </main>

      <footer className="relative z-10 grid shrink-0 grid-cols-[1fr_auto_1fr] items-center px-8 pb-7 pt-2">
        <div>
          <AnimatePresence initial={false}>
            {index > 0 && (
              <motion.div initial={{ opacity: 0, x: 6 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 6, transition: { duration: 0.15 } }}>
                <Button radius="full" variant="light" className="h-11 pl-3 pr-4 text-white/60 hover:text-white" startContent={<ArrowLeft className="size-4" />} onPress={() => go(index - 1)}>
                  {t('onboarding.back')}
                </Button>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
        <Progress index={index} onGo={go} />
        <div className="flex items-center justify-end gap-2">
          <VoiceToggle speaking={speaking} />
          <Button radius="full" className="btn-brand h-11 min-w-[148px] pl-6 pr-5 font-semibold" endContent={<ArrowRight className="size-4" />} onPress={() => go(index + 1)}>
            <span className="relative inline-grid">
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={index === 0 ? 'discover' : last ? 'start' : 'next'}
                  initial={{ opacity: 0, y: 8, filter: 'blur(4px)' }}
                  animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
                  exit={{ opacity: 0, y: -8, filter: 'blur(4px)', transition: { duration: 0.15 } }}
                  transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
                >
                  {index === 0 ? t('onboarding.discover') : last ? t('onboarding.start') : t('onboarding.next')}
                </motion.span>
              </AnimatePresence>
            </span>
          </Button>
        </div>
      </footer>
    </div>
  )
}
