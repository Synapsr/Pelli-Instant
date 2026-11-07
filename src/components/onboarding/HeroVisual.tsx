import { motion } from 'framer-motion'
import { Check, X } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { cn } from '@/lib/cn'
import { PHOTO_FRAME, Photo } from './Scene'
import { PHOTOS } from './photos'

interface Floating {
  src: string
  /** Position of the centre, in px from the orb's centre. */
  x: number
  y: number
  w: number
  h: number
  rotate: number
  /** 0 far … 1 near: how much it follows the pointer. */
  depth: number
  keep: boolean
}

const SHOOT: Floating[] = [
  { src: PHOTOS.couple, x: -305, y: -72, w: 150, h: 100, rotate: -6, depth: 0.9, keep: true },
  { src: PHOTOS.portraitBlurry, x: -168, y: -104, w: 76, h: 102, rotate: 5, depth: 0.5, keep: false },
  { src: PHOTOS.dress, x: -196, y: 92, w: 92, h: 122, rotate: -3, depth: 0.7, keep: true },
  { src: PHOTOS.groupDark, x: -368, y: 96, w: 104, h: 78, rotate: 4, depth: 0.35, keep: false },
  { src: PHOTOS.mountains, x: 300, y: -78, w: 150, h: 100, rotate: 5, depth: 0.85, keep: true },
  { src: PHOTOS.eyesClosed, x: 172, y: -100, w: 78, h: 104, rotate: -4, depth: 0.45, keep: false },
  { src: PHOTOS.rings, x: 206, y: 96, w: 112, h: 84, rotate: 3, depth: 0.65, keep: true },
  { src: PHOTOS.cityNight, x: 372, y: 92, w: 104, h: 60, rotate: -5, depth: 0.4, keep: true },
]

/** When the photos start receiving their verdict, and the gap between two. */
const SORT_AT = 1.5
const SORT_STEP = 0.16

/**
 * Welcome visual: a shoot bursts out of the orb, floats around it (with a
 * little parallax under the pointer), then each photo gets its verdict:
 * kept, or set aside and dimmed.
 */
export function HeroVisual() {
  const ref = useRef<HTMLDivElement>(null)

  // Pointer → CSS variables (-1 … 1): the photos drift by their depth.
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const onMove = (e: PointerEvent) => {
      const r = el.getBoundingClientRect()
      el.style.setProperty('--mx', (((e.clientX - r.left) / r.width) * 2 - 1).toFixed(3))
      el.style.setProperty('--my', (((e.clientY - r.top) / r.height) * 2 - 1).toFixed(3))
    }
    window.addEventListener('pointermove', onMove)
    return () => window.removeEventListener('pointermove', onMove)
  }, [])

  return (
    <div ref={ref} className="relative h-[360px] w-[820px] shrink-0 short:scale-[0.76]" style={{ ['--mx' as string]: 0, ['--my' as string]: 0 }}>
      {SHOOT.map((p, i) => {
        const sortAt = SORT_AT + i * SORT_STEP
        return (
          <motion.div
            key={p.src}
            className="absolute left-1/2 top-1/2"
            style={{ width: p.w, height: p.h, marginLeft: -p.w / 2, marginTop: -p.h / 2 }}
            initial={{ x: 0, y: 0, scale: 0.3, opacity: 0, rotate: 0 }}
            animate={{ x: p.x, y: p.y, scale: 1, opacity: 1, rotate: p.rotate }}
            transition={{ type: 'spring', duration: 1.1, bounce: 0, delay: 0.25 + i * 0.05 }}
          >
            {/* Parallax, then a slow float of its own. */}
            <div
              className="size-full transition-[translate] duration-700 ease-[cubic-bezier(0.22,1,0.36,1)]"
              style={{ translate: `calc(var(--mx) * ${p.depth * 16}px) calc(var(--my) * ${p.depth * 12}px)` }}
            >
              <div className="relative size-full [animation:float-y_6s_ease-in-out_infinite]" style={{ animationDelay: `${-i * 0.9}s`, animationDuration: `${5.5 + (i % 3)}s` }}>
                <motion.div
                  className={cn('size-full rounded-[14px]', PHOTO_FRAME)}
                  animate={p.keep ? undefined : { opacity: 0.42, filter: 'saturate(0.35)' }}
                  transition={{ duration: 0.6, ease: 'easeOut', delay: sortAt }}
                >
                  <Photo src={p.src} />
                </motion.div>
                <motion.span
                  aria-hidden
                  className={cn(
                    'absolute -right-2 -top-2 flex size-6 items-center justify-center rounded-full shadow-[0_0_0_3px_#0c0b12]',
                    p.keep ? 'bg-success text-[#0b1f10]' : 'bg-danger text-[#2a0705]',
                  )}
                  initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
                  animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
                  transition={{ type: 'spring', duration: 0.3, bounce: 0, delay: sortAt }}
                >
                  {p.keep ? <Check className="size-3.5" strokeWidth={3} /> : <X className="size-3.5" strokeWidth={3} />}
                </motion.span>
              </div>
            </div>
          </motion.div>
        )
      })}

      <div className="absolute left-1/2 top-1/2 z-10 -translate-x-1/2 -translate-y-1/2">
        <LiquidOrb size={150} />
      </div>
    </div>
  )
}
