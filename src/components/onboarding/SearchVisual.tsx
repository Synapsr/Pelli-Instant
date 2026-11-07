import { AnimatePresence, motion } from 'framer-motion'
import { Search } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { playSound } from '@/lib/sound'
import { PHOTO_FRAME, Photo } from './Scene'
import { PHOTOS } from './photos'

type Topic = 'bride' | 'rings'

const GRID: { src: string; topic: Topic | null }[] = [
  { src: PHOTOS.mountains, topic: null },
  { src: PHOTOS.rings, topic: 'rings' },
  { src: PHOTOS.drink, topic: null },
  { src: PHOTOS.dress, topic: 'bride' },
  { src: PHOTOS.cityNight, topic: null },
  { src: PHOTOS.dance, topic: 'bride' },
  { src: PHOTOS.ringsRoses, topic: 'rings' },
  { src: PHOTOS.couple, topic: 'bride' },
]

const TYPE_MS = 80
const ERASE_MS = 35
const HOLD_MS = 2600

/**
 * Search step: a query types itself, and the photos that match glide to
 * the front while the others fade back. Two queries take turns, forever.
 */
export function SearchVisual() {
  const t = useT()
  const queries = useMemo(
    () => [
      { text: t('onboarding.search.query1'), topic: 'bride' as const },
      { text: t('onboarding.search.query2'), topic: 'rings' as const },
    ],
    [t],
  )
  const [query, setQuery] = useState(0)
  const [typed, setTyped] = useState(0)
  const [found, setFound] = useState(false)

  // Type, show the results, erase, next query.
  useEffect(() => {
    const text = queries[query].text
    let timer = 0
    let n = 0
    const type = () => {
      n += 1
      setTyped(n)
      playSound('key')
      if (n < text.length) timer = window.setTimeout(type, TYPE_MS)
      else timer = window.setTimeout(show, 260)
    }
    const show = () => {
      setFound(true)
      timer = window.setTimeout(erase, HOLD_MS)
    }
    const erase = () => {
      setFound(false)
      // One key for the whole erase, as if the field were cleared at once.
      playSound('key')
      const step = () => {
        n -= 1
        setTyped(Math.max(0, n))
        if (n > 0) timer = window.setTimeout(step, ERASE_MS)
        else timer = window.setTimeout(() => setQuery((q) => (q + 1) % queries.length), 500)
      }
      timer = window.setTimeout(step, 200)
    }
    timer = window.setTimeout(type, query === 0 ? 900 : 300)
    return () => window.clearTimeout(timer)
  }, [query, queries])

  const topic = found ? queries[query].topic : null
  const ordered = topic ? [...GRID.filter((p) => p.topic === topic), ...GRID.filter((p) => p.topic !== topic)] : GRID
  const matches = topic ? GRID.filter((p) => p.topic === topic).length : 0

  return (
    <div className="flex w-[560px] flex-col items-center short:scale-[0.8]">
      <div className="glass flex h-12 w-full items-center gap-3 rounded-full pl-4 pr-5">
        <Search className="size-[18px] shrink-0 text-white/50" aria-hidden />
        <span className="flex min-w-0 flex-1 items-center text-[16px] text-white">
          {typed === 0 ? <span className="text-white/35">{t('onboarding.search.placeholder')}</span> : queries[query].text.slice(0, typed)}
          <span aria-hidden className="ml-px inline-block h-5 w-[2px] rounded-full bg-[#a599f5] [animation:caret-blink_1.05s_steps(1)_infinite]" />
        </span>
        <AnimatePresence initial={false}>
          {topic && (
            <motion.span
              key={topic}
              className="tabular text-[13px] text-white/50"
              initial={{ opacity: 0, filter: 'blur(4px)' }}
              animate={{ opacity: 1, filter: 'blur(0px)' }}
              exit={{ opacity: 0, filter: 'blur(4px)', transition: { duration: 0.15 } }}
            >
              {t('onboarding.search.results', { count: matches, n: matches })}
            </motion.span>
          )}
        </AnimatePresence>
      </div>

      <div className="mt-6 grid w-full grid-cols-4 gap-2.5">
        {ordered.map((p) => {
          const hit = topic !== null && p.topic === topic
          const dim = topic !== null && !hit
          return (
            <motion.div
              key={p.src}
              layout
              transition={{ layout: { type: 'spring', duration: 0.7, bounce: 0 } }}
              className={cn('relative aspect-[4/3] transition-[opacity,scale] duration-500 ease-out', dim && 'scale-[0.96] opacity-30')}
            >
              <div className={cn('size-full rounded-[14px]', PHOTO_FRAME)}>
                <Photo src={p.src} />
              </div>
              <div
                aria-hidden
                className={cn(
                  'pointer-events-none absolute inset-0 rounded-[14px] shadow-[0_0_0_2px_#7c6af0,0_14px_40px_-12px_rgba(88,64,240,0.8)] transition-opacity duration-500',
                  hit ? 'opacity-100' : 'opacity-0',
                )}
              />
            </motion.div>
          )
        })}
      </div>
    </div>
  )
}
