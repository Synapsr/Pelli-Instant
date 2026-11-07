import { motion, type Variants } from 'framer-motion'
import type { ImgHTMLAttributes, ReactNode } from 'react'
import { cn } from '@/lib/cn'

export const EASE = [0.22, 1, 0.36, 1] as const

/** A step: its children enter one after the other, the step leaves as a whole. */
export const STEP: Variants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.08, delayChildren: 0.04 } },
  exit: { opacity: 0, y: -10, filter: 'blur(6px)', transition: { duration: 0.18, ease: [0.4, 0, 1, 1] } },
}

/** Text blocks: rise, fade and un-blur. */
export const RISE: Variants = {
  hidden: { opacity: 0, y: 14, filter: 'blur(6px)' },
  show: { opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.55, ease: EASE } },
}

/** The visual of a step: settles from slightly smaller and out of focus. */
const VISUAL: Variants = {
  hidden: { opacity: 0, scale: 0.96, filter: 'blur(10px)' },
  show: { opacity: 1, scale: 1, filter: 'blur(0px)', transition: { type: 'spring', duration: 0.8, bounce: 0 } },
}

const TITLE: Variants = { hidden: {}, show: { transition: { staggerChildren: 0.045 } } }
const WORD: Variants = {
  hidden: { opacity: 0, y: 10, filter: 'blur(6px)' },
  show: { opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.5, ease: EASE } },
}

/** Headline revealed word by word. */
function Title({ children }: { children: string }) {
  const words = children.split(' ')
  return (
    <motion.h1
      variants={TITLE}
      aria-label={children}
      className="text-balance text-[42px] font-semibold leading-[1.06] tracking-[-0.035em] text-white short:text-[34px]"
    >
      {words.map((word, i) => (
        <span key={i} aria-hidden>
          <motion.span variants={WORD} className="inline-block">
            {word}
          </motion.span>
          {i < words.length - 1 && ' '}
        </span>
      ))}
    </motion.h1>
  )
}

/**
 * Layout shared by the steps of the welcome tour, Apple keynote style: a
 * large live visual, a short headline, one or two sentences, then optional
 * extras. The visual area has the same height on every step, so the text
 * doesn't jump from one step to the next.
 */
export function Scene({
  visual,
  eyebrow,
  title,
  body,
  children,
  visualClassName,
}: {
  visual: ReactNode
  eyebrow?: ReactNode
  title: string
  body: ReactNode
  children?: ReactNode
  visualClassName?: string
}) {
  return (
    <div className="flex w-full flex-col items-center">
      {/* `cn` concatenates without merging: a custom height replaces the default one. */}
      <motion.div variants={VISUAL} className={cn('relative flex w-full items-center justify-center', visualClassName ?? 'h-[360px] short:h-[270px]')}>
        {visual}
      </motion.div>
      <div className="mt-9 flex max-w-[680px] flex-col items-center text-center short:mt-6">
        {eyebrow && <motion.div variants={RISE}>{eyebrow}</motion.div>}
        <Title>{title}</Title>
        <motion.p variants={RISE} className="mt-4 max-w-[560px] text-pretty text-[16px] leading-relaxed text-white/55 short:mt-3 short:text-[15px]">
          {body}
        </motion.p>
        {children && <motion.div variants={RISE}>{children}</motion.div>}
      </div>
    </div>
  )
}

/** A photo of the tour: covers its frame, with the faint outline photos get in the app. */
export function Photo({ className, alt = '', ...props }: ImgHTMLAttributes<HTMLImageElement>) {
  return <img alt={alt} draggable={false} decoding="async" className={cn('size-full object-cover', className)} {...props} />
}

/** Thin white outline over a photo frame (images on dark surfaces). */
export const PHOTO_FRAME = 'overflow-hidden shadow-[0_24px_60px_-24px_rgba(0,0,0,0.85)] outline outline-1 -outline-offset-1 outline-white/10'
