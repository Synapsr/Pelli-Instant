import { motion } from 'framer-motion'
import { Infinity as InfinityIcon } from 'lucide-react'
import type { Account } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatLongDay, formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'

/**
 * Photos sorted this month against the plan's quota: a thin bar that turns
 * amber near the end, the reset date under it. Unlimited plans say so.
 */
export function UsageMeter({ account, className }: { account: Account; className?: string }) {
  const t = useT()
  const { monthlyQuota, usedThisMonth, resetsAt } = account.entitlements

  if (monthlyQuota === null) {
    return (
      <p className={cn('flex items-center gap-2 text-[13px] text-white/70', className)}>
        <InfinityIcon className="size-4 text-[#a599f5]" aria-hidden />
        {t('plans.unlimited')}
      </p>
    )
  }

  const fraction = monthlyQuota > 0 ? Math.min(1, usedThisMonth / monthlyQuota) : 1
  const low = fraction >= 0.85
  return (
    <div className={className}>
      <div className="flex items-baseline justify-between gap-3 text-[13px]">
        <span className="text-white/80">
          <span className="tabular font-semibold text-white">{formatNumber(usedThisMonth)}</span>
          <span className="text-white/45"> / {t('plans.photosPerMonth', { count: monthlyQuota, n: formatNumber(monthlyQuota) })}</span>
        </span>
        <span className="text-[11px] text-white/40">{t('plans.resets', { date: formatLongDay(resetsAt) })}</span>
      </div>
      <div
        className="mt-2 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"
        role="meter"
        aria-label={t('plans.usageLabel')}
        aria-valuemin={0}
        aria-valuemax={monthlyQuota}
        aria-valuenow={usedThisMonth}
      >
        <motion.div
          className={cn('h-full rounded-full', low ? 'bg-warning' : 'bg-[#7c6af0]')}
          initial={{ width: 0 }}
          animate={{ width: `${fraction * 100}%` }}
          transition={{ type: 'spring', duration: 0.8, bounce: 0 }}
        />
      </div>
    </div>
  )
}
