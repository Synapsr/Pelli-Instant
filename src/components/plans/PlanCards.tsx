import { motion } from 'framer-motion'
import { Check } from 'lucide-react'
import type { Offers, PlanOffer } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { formatPrice } from '@/lib/plans'

/** Plan recommended to someone sorting real shoots: unlimited at the lowest price. */
const RECOMMENDED = 'pro'

/**
 * Pelli plans side by side (prices and selling points come from Pelli). A
 * card opens Pelli's pricing page in the browser: the subscription itself
 * always happens on Pelli.
 */
export function PlanCards({ offers, plans, className }: { offers: Offers; plans: PlanOffer[]; className?: string }) {
  const t = useT()
  if (!plans.length) return null
  return (
    <div className={cn('grid gap-2.5', plans.length >= 3 ? 'grid-cols-3' : 'grid-cols-2', className)}>
      {plans.map((plan, i) => {
        const recommended = plan.slug === RECOMMENDED
        return (
          <motion.button
            key={plan.slug}
            type="button"
            onClick={() => void api().openExternal(offers.links.pricing)}
            initial={{ opacity: 0, y: 12, filter: 'blur(4px)' }}
            animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
            transition={{ duration: 0.45, ease: [0.22, 1, 0.36, 1], delay: 0.1 + i * 0.08 }}
            className={cn('focus-ring glass glass-hover relative flex flex-col rounded-2xl p-4 text-left active:scale-[0.98]', recommended && 'glass-selected')}
          >
            <span className="flex items-center justify-between gap-2">
              <span className="text-[15px] font-semibold text-white">{plan.name}</span>
              {recommended && <span className="rounded-full bg-[#5840f0]/30 px-2 py-0.5 text-[10px] font-semibold text-[#d4ceff]">{t('plans.recommended')}</span>}
            </span>
            <span className="mt-2 flex items-baseline gap-1">
              <span className="tabular text-[22px] font-semibold tracking-tight text-white">{formatPrice(plan.monthlyPrice, offers.currency)}</span>
              <span className="text-xs text-white/45">{t('plans.perMonth')}</span>
            </span>
            {plan.yearlyPricePerMonth > 0 && plan.yearlyPricePerMonth < plan.monthlyPrice && (
              <span className="text-[11px] text-white/40">{t('plans.yearly', { price: formatPrice(plan.yearlyPricePerMonth, offers.currency) })}</span>
            )}
            <span className="mt-3 text-[13px] font-medium text-[#c9c1fb]">
              {plan.photosPerMonth === null ? t('plans.unlimitedShort') : t('plans.photosPerMonth', { count: plan.photosPerMonth, n: formatNumber(plan.photosPerMonth) })}
            </span>
            <ul className="mt-2 space-y-1">
              {plan.highlights.map((h) => (
                <li key={h} className="flex items-start gap-1.5 text-[12px] leading-snug text-white/55">
                  <Check className="mt-0.5 size-3 shrink-0 text-white/40" aria-hidden />
                  {h}
                </li>
              ))}
            </ul>
          </motion.button>
        )
      })}
    </div>
  )
}
