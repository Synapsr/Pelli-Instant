import { Heart, Star, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { isSupportCardDue, shouldShowSupportCard } from '@/lib/support'
import { usePlusStore } from '@/store/plus-store'
import { usePrefsStore } from '@/store/prefs-store'

/**
 * Whether the export report shows the support card, for a report whose run
 * `succeeded`. Its turn is decided once, when the report opens, so the card
 * stays while the report is open; Plus is checked live. As soon as a report
 * shows it, it is recorded: it never comes back, even if the report closes
 * with Done.
 */
export function useSupportCard(succeeded: boolean): boolean {
  const plusLoaded = usePlusStore((s) => s.loaded)
  const plusActive = usePlusStore((s) => s.status.active)
  const markShown = usePrefsStore((s) => s.markSupportCardShown)
  const [due] = useState(() => {
    const { exportsDone, supportCardShown } = usePrefsStore.getState()
    return isSupportCardDue({ exportsDone, shown: supportCardShown })
  })
  const show = shouldShowSupportCard({ due, succeeded, plusLoaded, plusActive })
  useEffect(() => {
    if (show) markShown()
  }, [show, markShown])
  return show
}

/**
 * The one-time card at the end of an export: follow Pelli on Instagram first
 * (photographers rarely have a GitHub account), a GitHub star second. Shown
 * once; opening either link or closing the card also records it.
 */
export function SupportCard({ className }: { className?: string }) {
  const t = useT()
  const markShown = usePrefsStore((s) => s.markSupportCardShown)
  const [open, setOpen] = useState(true)
  if (!open) return null

  const visit = (url: string) => {
    markShown()
    void api().openExternal(url)
  }

  return (
    <div
      className={cn(
        'relative rounded-2xl border border-[#5840f0]/20 bg-gradient-to-r from-[#5840f0]/[0.12] to-[#5840f0]/[0.03] px-4 py-3.5 text-left',
        className,
      )}
    >
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={() => {
          markShown()
          setOpen(false)
        }}
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
            onClick={() => visit(INSTAGRAM_URL)}
            className="focus-ring btn-brand mt-2.5 inline-flex h-8 items-center rounded-full px-4 text-[12px] font-semibold"
          >
            {t('support.instagram')}
          </button>
          <button
            type="button"
            onClick={() => visit(GITHUB_URL)}
            className="focus-ring group mt-2 flex items-start gap-1.5 text-left text-[12px] leading-snug text-default-500 transition-colors duration-150 hover:text-default-700"
          >
            <Star className="mt-px size-3.5 shrink-0 text-[#b8adf8]" aria-hidden />
            <span className="text-pretty underline-offset-2 group-hover:underline">{t('support.github')}</span>
          </button>
        </div>
      </div>
    </div>
  )
}
