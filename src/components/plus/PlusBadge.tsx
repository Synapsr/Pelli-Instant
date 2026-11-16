import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'

/** Small "Plus" badge on what Pelli Instant Plus unlocks (Maximum, AI search). */
export function PlusBadge({ className }: { className?: string }) {
  const t = useT()
  return (
    <span
      className={cn(
        'inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-gradient-to-r from-[#5840f0]/35 to-[#7c6af0]/25 px-2 text-[10px] font-semibold text-[#d4ceff] ring-1 ring-inset ring-[#a599f5]/30',
        className,
      )}
    >
      <Sparkles className="size-2.5" strokeWidth={2.4} aria-hidden />
      {t('plus.badge')}
    </span>
  )
}
