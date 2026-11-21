import { Heart, Star, X } from 'lucide-react'
import { useState } from 'react'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { shouldAskSupport } from '@/lib/support'
import { usePlusStore } from '@/store/plus-store'
import { usePrefsStore } from '@/store/prefs-store'

/** Whether the export report shows the support card: decided once, so opening a link doesn't make it vanish. */
export function useSupportCard(): boolean {
  const exportsDone = usePrefsStore((s) => s.exportsDone)
  const closed = usePrefsStore((s) => s.supportCardClosed)
  const plusLoaded = usePlusStore((s) => s.loaded)
  const plusActive = usePlusStore((s) => s.status.active)
  const [shown] = useState(() => shouldAskSupport({ exportsDone, closed, plusLoaded, plusActive }))
  return shown
}

/**
 * The one-time card at the end of an export: follow Pelli on Instagram first
 * (photographers rarely have a GitHub account), a GitHub star second. Opening
 * either link or closing the card means it never shows again.
 */
export function SupportCard({ className }: { className?: string }) {
  const t = useT()
  const closeSupportCard = usePrefsStore((s) => s.closeSupportCard)
  const [open, setOpen] = useState(true)
  if (!open) return null

  const visit = (url: string) => {
    closeSupportCard()
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
          closeSupportCard()
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
