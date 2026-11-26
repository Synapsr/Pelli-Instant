import { Heart, Star, X } from 'lucide-react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { closeSupportCard, followFromSupportCard, getPlusFromSupportCard, useSupportCardVisible } from '@/store/support-card'

/**
 * The free version's card under the result of a sort: follow Pelli on
 * Instagram first (photographers rarely have a GitHub account), a GitHub star
 * second, and a discreet line to Plus, which hides it. Inline, never in the
 * way of the review; closing it or opening a link hides it until the next sort.
 */
export function SupportCard({ className }: { className?: string }) {
  const t = useT()
  const visible = useSupportCardVisible()
  if (!visible) return null

  return (
    <div
      className={cn(
        'relative w-full rounded-2xl border border-[#5840f0]/20 bg-gradient-to-r from-[#5840f0]/[0.12] to-[#5840f0]/[0.03] px-4 py-3.5 text-left',
        className,
      )}
    >
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={closeSupportCard}
        className="focus-ring absolute right-2 top-2 rounded-full p-1 text-default-400 transition-colors duration-150 hover:text-default-600"
      >
        <X className="size-3.5" aria-hidden />
      </button>
      <div className="flex items-start gap-3 pr-5">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white/[0.07] text-[#b8adf8]" aria-hidden>
          <Heart className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-semibold text-foreground">{t('support.title')}</p>
          <p className="mt-0.5 text-pretty text-[12px] leading-snug text-default-500">{t('support.body')}</p>
          <button
            type="button"
            onClick={() => followFromSupportCard(INSTAGRAM_URL)}
            className="focus-ring btn-brand mt-2.5 inline-flex h-8 items-center rounded-full px-4 text-[12px] font-semibold"
          >
            {t('support.instagram')}
          </button>
          <button
            type="button"
            onClick={() => followFromSupportCard(GITHUB_URL)}
            className="focus-ring group mt-2 flex items-start gap-1.5 text-left text-[12px] leading-snug text-default-500 transition-colors duration-150 hover:text-default-700"
          >
            <Star className="mt-px size-3.5 shrink-0 text-[#b8adf8]" aria-hidden />
            <span className="text-pretty underline-offset-2 group-hover:underline">{t('support.github')}</span>
          </button>
          <button
            type="button"
            onClick={getPlusFromSupportCard}
            className="focus-ring mt-2 block rounded text-left text-[11px] leading-snug text-default-400 underline-offset-2 transition-colors duration-150 hover:text-[#b8adf8] hover:underline"
          >
            {t('support.plus')}
          </button>
        </div>
      </div>
    </div>
  )
}
