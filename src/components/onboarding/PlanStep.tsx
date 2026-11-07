import { Button } from '@heroui/react'
import { motion } from 'framer-motion'
import { Check, ExternalLink, Infinity as InfinityIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { PlanCards } from '@/components/plans/PlanCards'
import { AnimatedNumber } from '@/components/ui/AnimatedNumber'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { PlanBadge } from '@/components/ui/PlanBadge'
import { api } from '@/lib/api'
import type { Account } from '@/lib/contract'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { billingUrl, isUnlimited, PELLI_URL, photosLeft, trialDaysLeft, upgradesFor } from '@/lib/plans'
import { useAppStore } from '@/store/app-store'
import { EASE, Scene } from './Scene'

const COMPACT = 'h-[210px] short:h-[170px]'
/** Free and Lite also show the plans: the ring gets smaller on short windows. */
const RING_AREA = 'h-[210px] short:h-[92px]'

/** The orb holding the infinity sign: nothing is counted. */
function UnlimitedOrb({ badge }: { badge?: ReactNode }) {
  return (
    <div className="flex flex-col items-center short:scale-[0.8]">
      <LiquidOrb size={168}>
        <motion.span
          initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
          animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
          transition={{ type: 'spring', duration: 0.5, bounce: 0, delay: 0.45 }}
        >
          <InfinityIcon className="size-14 text-white" strokeWidth={1.5} />
        </motion.span>
      </LiquidOrb>
      {badge && (
        <motion.div className="mt-5" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.4, ease: EASE, delay: 0.6 }}>
          {badge}
        </motion.div>
      )}
    </div>
  )
}

/** What's left this month: the ring shows the share of the quota still available. */
function UsageRing({ account }: { account: Account }) {
  const t = useT()
  const quota = account.entitlements.monthlyQuota ?? 0
  const left = photosLeft(account) ?? 0
  const used = quota > 0 ? Math.min(1, account.entitlements.usedThisMonth / quota) : 1
  const size = 176
  const stroke = 10
  const r = (size - stroke) / 2
  const length = 2 * Math.PI * r
  return (
    <div className="relative short:scale-50" style={{ width: size, height: size }}>
      <span aria-hidden className="absolute inset-6 rounded-full bg-[#5840f0]/25 blur-2xl" />
      <svg width={size} height={size} className="relative -rotate-90" aria-hidden>
        <defs>
          <linearGradient id="usage-ring" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor="#5840f0" />
            <stop offset="100%" stopColor="#a599f5" />
          </linearGradient>
        </defs>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth={stroke} />
        <motion.circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="url(#usage-ring)"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={length}
          initial={{ strokeDashoffset: length }}
          animate={{ strokeDashoffset: length * used }}
          transition={{ type: 'spring', duration: 1.4, bounce: 0, delay: 0.35 }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center text-center">
        <AnimatedNumber value={left} from={quota} duration={1.4} className="text-[40px] font-semibold leading-none tracking-[-0.03em] text-white" />
        <span className="mt-1.5 text-[12px] text-white/55 short:hidden">{t('onboarding.plan.leftLabel', { count: left })}</span>
        <span className="text-[11px] text-white/35 short:hidden">{t('onboarding.plan.ofQuota', { n: formatNumber(quota) })}</span>
      </div>
    </div>
  )
}

/**
 * Last step of the welcome tour: what the photographer's Pelli plan gives.
 * Subscribers hear that it's unlimited thanks to their plan (and what else
 * their plan brings); free and Lite accounts see what's left this month,
 * the Pelli trial and the plans.
 */
export function PlanStep() {
  const t = useT()
  const info = useAppStore((s) => s.appInfo)
  const auth = useAppStore((s) => s.auth)
  const offers = useAppStore((s) => s.offers)

  if (info?.noAccount || auth.status !== 'signedIn') {
    return (
      <Scene visualClassName={COMPACT} visual={<UnlimitedOrb />} title={t('onboarding.plan.noAccountTitle')} body={t('onboarding.plan.noAccountBody')}>
        <Button radius="full" variant="light" className="mt-5 text-white/70" endContent={<ExternalLink className="size-3.5" />} onPress={() => void api().openExternal(PELLI_URL)}>
          {t('onboarding.plan.discoverPelli')}
        </Button>
      </Scene>
    )
  }

  const { account } = auth
  const trialDays = trialDaysLeft(account)
  const upgrades = offers ? upgradesFor(offers, account) : []
  const own = offers?.plans.find((p) => p.slug === account.plan)

  // Paid plan (or lifetime): unlimited thanks to the subscription.
  if (isUnlimited(account) && trialDays === null) {
    return (
      <Scene
        visualClassName={COMPACT}
        visual={<UnlimitedOrb badge={<PlanBadge plan={account.plan} label={account.planLabel} />} />}
        title={t('onboarding.plan.unlimitedTitle', { plan: account.planLabel })}
        body={t('onboarding.plan.unlimitedBody', { plan: account.planLabel })}
      >
        {own && own.highlights.length > 0 && (
          <div className="glass mt-7 w-[480px] max-w-full rounded-2xl p-4 text-left">
            <p className="text-[11px] font-semibold uppercase tracking-[0.1em] text-white/40">{t('onboarding.plan.includedTitle', { plan: account.planLabel })}</p>
            <ul className="mt-2.5 grid grid-cols-2 gap-x-4 gap-y-1.5">
              {own.highlights.map((h) => (
                <li key={h} className="flex items-start gap-2 text-[13px] text-white/75">
                  <Check className="mt-0.5 size-3.5 shrink-0 text-[#a599f5]" aria-hidden />
                  {h}
                </li>
              ))}
            </ul>
          </div>
        )}
      </Scene>
    )
  }

  // Trial: unlimited for now, then the plans.
  if (trialDays !== null) {
    return (
      <Scene
        visualClassName={COMPACT}
        visual={<UnlimitedOrb />}
        title={t('onboarding.plan.trialTitle')}
        body={t('onboarding.plan.trialBody', { count: trialDays, days: formatNumber(trialDays) })}
      >
        {offers && (
          <div className="mt-7 flex flex-col items-center">
            <PlanCards offers={offers} plans={upgrades} className="w-[640px] max-w-full" />
            <Button radius="full" variant="light" className="mt-4 text-white/70" endContent={<ExternalLink className="size-3.5" />} onPress={() => void api().openExternal(billingUrl(offers))}>
              {t('onboarding.plan.choosePlan')}
            </Button>
          </div>
        )}
      </Scene>
    )
  }

  // Free or Lite: what's left this month, the trial (when it's still open to them), the plans.
  const free = account.plan === 'free'
  const canTry = free && account.trialAvailable === true && offers !== null
  const n = formatNumber(account.entitlements.monthlyQuota ?? 0)
  return (
    <Scene
      visualClassName={RING_AREA}
      visual={<UsageRing account={account} />}
      title={free ? t('onboarding.plan.freeTitle', { n }) : t('onboarding.plan.liteTitle', { n })}
      body={
        free
          ? canTry
            ? t('onboarding.plan.freeBody', { n, days: formatNumber(offers.trialDays) })
            : t('onboarding.plan.freeBodyNoTrial', { n })
          : t('onboarding.plan.liteBody', { n })
      }
    >
      {offers ? (
        <div className="mt-7 flex flex-col items-center short:mt-5">
          <PlanCards offers={offers} plans={upgrades} className="w-[640px] max-w-full" />
          <Button
            radius="full"
            className="glass glass-hover glass-selected mt-5 h-10 pl-5 pr-4 font-semibold text-white"
            endContent={<ExternalLink className="size-3.5" />}
            onPress={() => void api().openExternal(canTry ? offers.links.trial : offers.links.pricing)}
          >
            {canTry ? t('onboarding.plan.tryPelli', { days: formatNumber(offers.trialDays) }) : t('quota.seePlans')}
          </Button>
          <p className="mt-4 max-w-[460px] text-pretty text-[11px] leading-snug text-white/35 short:hidden">{t('onboarding.plan.countNote')}</p>
        </div>
      ) : (
        <p className="mt-4 text-xs text-white/40">{t('onboarding.plan.offline')}</p>
      )}
    </Scene>
  )
}
