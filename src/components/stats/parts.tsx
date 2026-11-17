import { motion, useReducedMotion } from 'framer-motion'
import { useState, type ReactNode } from 'react'
import { DECISION_BG } from '@/components/ui/Decision'
import type { DecisionCounts, FinalDecision } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatMonthShort, formatMonthYear, formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { busiestMonth, type MonthBar } from '@/lib/stats'

export const EASE = [0.22, 1, 0.36, 1] as const

/** "34 %", with a decimal under 10 % ("2,4 %"); a dash without a total. */
export function percent(fraction: number | null): string {
  if (fraction === null) return '—'
  return formatNumber(fraction, undefined, { style: 'percent', maximumFractionDigits: fraction > 0 && fraction < 0.1 ? 1 : 0 })
}

/** A section that rises in after the previous one; still when motion is reduced. */
export function Reveal({ index = 0, className, children }: { index?: number; className?: string; children: ReactNode }) {
  const reduce = useReducedMotion()
  return (
    <motion.div
      className={className}
      initial={reduce ? false : { opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.45, ease: EASE, delay: 0.04 + index * 0.06 }}
    >
      {children}
    </motion.div>
  )
}

/** A headline figure; `hero`: the violet one. */
export function Tile({ label, value, hint, hero = false }: { label: string; value: ReactNode; hint?: ReactNode; hero?: boolean }) {
  return (
    <div
      className={cn(
        'relative min-w-0 overflow-hidden rounded-[20px] px-5 pb-4 pt-[18px]',
        hero ? 'border border-[#a599f5]/25 bg-gradient-to-br from-[#5840f0]/[0.3] via-[#5840f0]/[0.12] to-[#5840f0]/[0.04] shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]' : 'glass',
      )}
    >
      {hero && <span aria-hidden className="pointer-events-none absolute -right-12 -top-16 size-44 rounded-full bg-[radial-gradient(closest-side,rgba(124,106,240,0.42),transparent)]" />}
      <p className="relative truncate text-[11px] font-semibold uppercase tracking-[0.12em] text-white/45">{label}</p>
      <div className="relative mt-2.5 text-[32px] font-semibold leading-none tracking-[-0.03em] text-white tabular short:text-[28px]">{value}</div>
      {hint && <div className="relative mt-3 text-[12px] leading-snug text-white/50">{hint}</div>}
    </div>
  )
}

export function Card({ title, aside, className, children }: { title: string; aside?: ReactNode; className?: string; children: ReactNode }) {
  return (
    <section className={cn('glass flex min-w-0 flex-col rounded-[22px] p-5', className)}>
      <header className="flex items-baseline justify-between gap-3">
        <h3 className="truncate text-[14px] font-semibold tracking-[-0.01em] text-white">{title}</h3>
        {aside && <div className="shrink-0 text-[11.5px] text-white/40">{aside}</div>}
      </header>
      <div className="mt-4 flex min-h-0 flex-1 flex-col">{children}</div>
    </section>
  )
}

export function Muted({ children }: { children: ReactNode }) {
  return <p className="text-pretty text-[12.5px] leading-snug text-white/40">{children}</p>
}

const TONES = {
  brand: 'bg-gradient-to-r from-[#5840f0] to-[#8a7af7]',
  success: 'bg-success/80',
  warning: 'bg-warning/80',
  danger: 'bg-gradient-to-r from-danger/85 to-danger/50',
  muted: 'bg-white/35',
} as const

/** A thin bar filled to `value` (0 … 1). */
export function Meter({ value, tone = 'brand', index = 0, className }: { value: number; tone?: keyof typeof TONES; index?: number; className?: string }) {
  const reduce = useReducedMotion()
  const width = `${Math.max(0, Math.min(1, value)) * 100}%`
  return (
    <div className={cn('h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[0.06]', className)} aria-hidden>
      <motion.span
        className={cn('block h-full rounded-full', TONES[tone])}
        initial={reduce ? false : { width: 0 }}
        animate={{ width }}
        transition={{ duration: 0.7, ease: EASE, delay: 0.15 + index * 0.04 }}
      />
    </div>
  )
}

const DECISIONS: FinalDecision[] = ['selected', 'toReview', 'rejected']

/** Selected, to review, rejected: one bar in the verdict colours of the app. */
export function DecisionBar({ counts, className }: { counts: DecisionCounts; className?: string }) {
  return (
    <div className={cn('flex h-1.5 gap-[2px] overflow-hidden rounded-full bg-white/[0.06]', className)} aria-hidden>
      {DECISIONS.map((d) => counts[d] > 0 && <span key={d} className={cn('h-full', DECISION_BG[d], d === 'rejected' && 'opacity-80')} style={{ flexGrow: counts[d], flexBasis: 0 }} />)}
    </div>
  )
}

/** The three counts under a decision bar, coloured like the project cards. */
export function DecisionCountsRow({ counts }: { counts: DecisionCounts }) {
  const t = useT()
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
      {DECISIONS.map((d) => (
        <span key={d} className="inline-flex items-center gap-1" title={t(`review.tabs.${d}`)}>
          <span className={cn('size-1.5 rounded-full', DECISION_BG[d])} />
          <span className="tabular text-[11px] text-white/55">{formatNumber(counts[d])}</span>
          <span className="sr-only">{t(`review.tabs.${d}`)}</span>
        </span>
      ))}
    </div>
  )
}

function Legend({ className, label }: { className: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span aria-hidden className={cn('size-2 rounded-[3px]', className)} />
      {label}
    </span>
  )
}

/**
 * Photos per month of capture: the whole bar is every photo sorted, its
 * bright part the selected ones. Hover or focus a month for its numbers; the
 * busiest one shows its total.
 */
export function MonthChart({ bars }: { bars: MonthBar[] }) {
  const t = useT()
  const reduce = useReducedMotion()
  const [hover, setHover] = useState<number | null>(null)
  const max = Math.max(1, ...bars.map((b) => b.photos))
  const best = busiestMonth(bars)
  const monthOf = (b: MonthBar) => formatMonthYear(Date.UTC(b.year, b.month - 1, 1))
  const details = (b: MonthBar) => `${t('common.photos', { count: b.photos })} · ${t('home.kept', { count: b.selected })}`

  return (
    <div className="flex flex-1 flex-col">
      <div className="relative min-h-[132px] flex-1 pt-6">
        <div aria-hidden className="absolute inset-x-0 bottom-0 top-6 flex flex-col justify-between">
          {[0, 1, 2, 3].map((i) => (
            <span key={i} className="border-t border-dashed border-white/[0.05]" />
          ))}
        </div>
        <ul className="relative flex h-full items-end gap-[6px]" aria-label={t('stats.months.title')}>
          {bars.map((b, i) => {
            const height = (b.photos / max) * 100
            return (
              <li
                key={`${b.year}-${b.month}`}
                tabIndex={0}
                aria-label={`${monthOf(b)}, ${details(b)}`}
                onMouseEnter={() => setHover(i)}
                onMouseLeave={() => setHover(null)}
                onFocus={() => setHover(i)}
                onBlur={() => setHover(null)}
                className="focus-ring relative flex h-full min-w-0 flex-1 flex-col justify-end rounded-md outline-none"
              >
                {b.photos > 0 ? (
                  <motion.div
                    className={cn('relative mx-auto w-full max-w-[44px] origin-bottom overflow-hidden rounded-b-[3px] rounded-t-[7px] bg-[#5840f0]/30 transition-[filter] duration-150', hover === i && 'brightness-125')}
                    style={{ height: `${Math.max(height, 2.5)}%` }}
                    initial={reduce ? false : { scaleY: 0 }}
                    animate={{ scaleY: 1 }}
                    transition={{ duration: 0.6, ease: EASE, delay: 0.12 + i * 0.03 }}
                  >
                    <span className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-[#5840f0] to-[#8a7af7]" style={{ height: `${(b.selected / b.photos) * 100}%` }} />
                  </motion.div>
                ) : (
                  <span className="mx-auto h-[2px] w-full max-w-[44px] rounded-full bg-white/[0.07]" />
                )}
                {i === best && hover === null && (
                  <span className="pointer-events-none absolute inset-x-0 text-center text-[11px] font-semibold text-white/80 tabular" style={{ bottom: `calc(${height}% + 6px)` }}>
                    {formatNumber(b.photos)}
                  </span>
                )}
              </li>
            )
          })}
        </ul>
        {hover !== null && (
          <div
            aria-hidden
            className={cn(
              'pointer-events-none absolute z-10 whitespace-nowrap rounded-xl border border-white/10 bg-[#15131f]/95 px-3 py-2 shadow-[0_12px_32px_-12px_rgba(0,0,0,0.8)] backdrop-blur-md',
              hover === 0 ? 'translate-x-0' : hover === bars.length - 1 ? '-translate-x-full' : '-translate-x-1/2',
            )}
            style={{
              left: hover === 0 ? 0 : hover === bars.length - 1 ? '100%' : `${((hover + 0.5) / bars.length) * 100}%`,
              // Above its bar: the bars fill the box below its 24 px top padding.
              bottom: `calc((100% - 24px) * ${Math.max(bars[hover].photos / max, 0.025)} + 10px)`,
            }}
          >
            <p className="text-[12px] font-semibold text-white first-letter:uppercase">{monthOf(bars[hover])}</p>
            <p className="mt-0.5 text-[11px] text-white/60 tabular">{details(bars[hover])}</p>
          </div>
        )}
      </div>
      <div aria-hidden className="mt-2 flex gap-[6px]">
        {bars.map((b) => (
          <span key={`${b.year}-${b.month}`} className="min-w-0 flex-1 truncate text-center text-[10.5px] text-white/35">
            {formatMonthShort(b.year, b.month)}
          </span>
        ))}
      </div>
      <footer className="mt-auto flex flex-wrap items-center justify-between gap-x-4 gap-y-1 pt-4 text-[11.5px] text-white/45">
        <span className="min-w-0 truncate">{best >= 0 && t('stats.months.busiest', { month: monthOf(bars[best]) })}</span>
        <span className="flex items-center gap-3">
          <Legend className="bg-[#5840f0]/40" label={t('stats.months.photos')} />
          <Legend className="bg-[#7c6af0]" label={t('review.tabs.selected')} />
        </span>
      </footer>
    </div>
  )
}
