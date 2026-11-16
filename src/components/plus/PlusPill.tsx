import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { usePlusStore } from '@/store/plus-store'

/**
 * The one discreet way to Plus in the window bar, for the free version only.
 * A checkout waiting for its payment shows a small dot.
 */
export function PlusPill({ className }: { className?: string }) {
  const t = useT()
  const active = usePlusStore((s) => s.status.active)
  const loaded = usePlusStore((s) => s.loaded)
  const pending = usePlusStore((s) => s.status.pending)
  const openDialog = usePlusStore((s) => s.openDialog)
  if (!loaded || active) return null
  return (
    <button
      type="button"
      onClick={() => openDialog('pill')}
      aria-label={t('plus.name')}
      title={t('plus.name')}
      className={cn(
        // 32 px tall, with a 40 px hit area.
        'focus-ring relative inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-[12px] font-semibold text-[#d4ceff] transition-[background-color,color,scale,box-shadow] duration-200 ease-out after:absolute after:-inset-1 after:content-[""] active:scale-[0.96]',
        'bg-gradient-to-r from-[#5840f0]/25 to-[#7c6af0]/15 shadow-[inset_0_0_0_1px_rgba(165,153,245,0.25)] hover:from-[#5840f0]/40 hover:to-[#7c6af0]/25 hover:text-white',
        className,
      )}
    >
      <Sparkles className="size-3.5" strokeWidth={2.2} aria-hidden />
      {t('plus.badge')}
      {pending && <span aria-hidden className="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-warning shadow-[0_0_0_2px_#0c0b12]" />}
    </button>
  )
}
