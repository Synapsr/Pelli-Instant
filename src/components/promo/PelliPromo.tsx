import { motion } from 'framer-motion'
import { ArrowRight, ArrowUpRight } from 'lucide-react'
import { PelliLogo } from '@/components/ui/Logo'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { pelliPromoUrl } from '@/lib/links'
import { useT } from '@/lib/i18n'
import { usePlusStore } from '@/store/plus-store'

/** Hidden with Plus (and until the engine said whether Plus is active). */
function usePromoVisible(): boolean {
  return usePlusStore((s) => s.loaded && !s.status.active)
}

/**
 * The free version's Pelli card on the home screen: one line about Pelli,
 * a link to its free trial. Never a pop-up, never in the way.
 */
export function PelliHomePromo({ delay = 0, className }: { delay?: number; className?: string }) {
  const t = useT()
  const visible = usePromoVisible()
  if (!visible) return null
  return (
    <motion.button
      type="button"
      onClick={() => void api().openExternal(pelliPromoUrl('home'))}
      initial={{ opacity: 0, y: 8, filter: 'blur(4px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay }}
      whileTap={{ scale: 0.98 }}
      className={cn(
        'focus-ring glass glass-hover group flex items-center gap-3.5 rounded-2xl py-2.5 pl-3 pr-4 text-left',
        className,
      )}
    >
      <span className="flex h-9 w-12 shrink-0 items-center justify-center rounded-xl bg-white/[0.06] text-white/85" aria-hidden>
        <PelliLogo className="h-3.5" title="" />
      </span>
      <span className="min-w-0">
        <span className="block truncate text-[13px] font-semibold text-white/90">{t('promo.homeTitle')}</span>
        <span className="block truncate text-[12px] text-white/45">{t('promo.homeBody')}</span>
      </span>
      <span className="ml-1 inline-flex shrink-0 items-center gap-1 text-[12px] font-medium text-[#b8adf8] transition-colors duration-150 group-hover:text-[#d4ceff]">
        {t('promo.homeCta')}
        <ArrowRight className="size-3.5 transition-transform duration-200 ease-out group-hover:translate-x-0.5" aria-hidden />
      </span>
    </motion.button>
  )
}

/** The free version's line under a finished export: the selection, in a Pelli gallery. */
export function PelliExportPromo({ className }: { className?: string }) {
  const t = useT()
  const visible = usePromoVisible()
  if (!visible) return null
  return (
    <button
      type="button"
      onClick={() => void api().openExternal(pelliPromoUrl('export'))}
      className={cn(
        'focus-ring group flex w-full items-center gap-3 rounded-2xl border border-[#5840f0]/20 bg-gradient-to-r from-[#5840f0]/[0.12] to-[#5840f0]/[0.03] px-4 py-3 text-left transition-[background-color,border-color,scale] duration-200 hover:border-[#5840f0]/35 active:scale-[0.99]',
        className,
      )}
    >
      <span className="flex h-8 w-11 shrink-0 items-center justify-center rounded-lg bg-white/[0.07] text-white/85" aria-hidden>
        <PelliLogo className="h-3" title="" />
      </span>
      <span className="min-w-0 flex-1 text-pretty text-[13px] leading-snug text-default-600">{t('promo.export')}</span>
      <ArrowUpRight className="size-4 shrink-0 text-[#b8adf8] transition-transform duration-200 ease-out group-hover:-translate-y-0.5 group-hover:translate-x-0.5" aria-hidden />
    </button>
  )
}
