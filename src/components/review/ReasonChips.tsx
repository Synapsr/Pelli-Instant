import { AnimatePresence, motion } from 'framer-motion'
import { useMemo } from 'react'
import { ReasonIcon } from '@/components/ui/ReasonIcon'
import type { Reason } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { REASONS } from '@/lib/review/view'
import { useReviewStore } from '@/store/review-store'

/**
 * Why photos are in this tab ("12 Floues · 5 Yeux fermés"), as filters. Only
 * on the "to review" and "rejected" tabs, only when there is a reason.
 */
export function ReasonChips() {
  const t = useT()
  const tab = useReviewStore((s) => s.filters.tab)
  const active = useReviewStore((s) => s.filters.reason)
  const photos = useReviewStore((s) => s.photos)
  const setReason = useReviewStore((s) => s.setReason)
  const shown = tab === 'toReview' || tab === 'rejected'

  const counts = useMemo(() => {
    const c = new Map<Reason, number>()
    if (!shown) return c
    for (const p of Object.values(photos)) {
      if (p.status !== 'analyzed' || p.decision !== tab) continue
      const reasons = new Set((p.assessment?.reasons ?? []).map((r) => (r === 'exactDuplicate' ? 'duplicate' : r)))
      reasons.forEach((r) => c.set(r, (c.get(r) ?? 0) + 1))
    }
    return c
  }, [photos, tab, shown])

  const reasons = REASONS.filter((r) => (counts.get(r) ?? 0) > 0)
  const visible = shown && reasons.length > 0

  return (
    <AnimatePresence initial={false}>
      {visible && (
        <motion.nav
          key={tab}
          aria-label={t('review.sidebar.reasons')}
          initial={{ opacity: 0, height: 0 }}
          animate={{ opacity: 1, height: 'auto' }}
          exit={{ opacity: 0, height: 0 }}
          transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
          className="shrink-0 overflow-hidden"
        >
          <div className="flex flex-wrap items-center justify-center gap-1.5 px-5 pb-1 pt-3">
            <Chip active={active === null} onPress={() => setReason(null)}>
              {t('review.chips.all')}
            </Chip>
            {reasons.map((r) => (
              <Chip key={r} active={active === r} onPress={() => setReason(active === r ? null : r)}>
                <ReasonIcon reason={r} className="size-3.5 opacity-70" />
                <span className="tabular font-semibold">{formatNumber(counts.get(r) ?? 0)}</span>
                {t(`reasonPlural.${r}`)}
              </Chip>
            ))}
          </div>
        </motion.nav>
      )}
    </AnimatePresence>
  )
}

function Chip({ active, onPress, children }: { active: boolean; onPress: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onPress}
      className={cn(
        'focus-ring inline-flex h-7 items-center gap-1.5 rounded-full px-3 text-xs transition-all duration-200 ease-out',
        active
          ? 'bg-[#5840f0]/25 text-white ring-1 ring-[#7c6af0]/50'
          : 'bg-white/[0.04] text-default-500 ring-1 ring-white/[0.06] hover:bg-white/[0.08] hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}
