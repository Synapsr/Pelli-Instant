import { AnimatePresence, motion } from 'framer-motion'
import { Check } from 'lucide-react'
import { PackBar, PackStatus } from '@/components/precision/PackStatus'
import { PRECISIONS, usePrecision } from '@/components/precision/PrecisionChoice'
import { Toggle } from '@/components/ui/Toggle'
import { cn } from '@/lib/cn'
import { formatBytes } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'

/**
 * Precision step: Standard or Maximum as two big cards. Picking Maximum
 * starts its download right away, in the background, while the tour goes
 * on. Under them, the switch that leaves the computer to the other apps.
 */
export function PrecisionVisual() {
  const t = useT()
  const precision = usePrecision()
  const background = useAppStore((s) => s.settings?.background ?? false)
  const update = useAppStore((s) => s.updateSettings)
  if (!precision) return null
  const { current, pack, choose } = precision
  // An engine without the pack: Standard only, nothing to pick.
  const options = pack ? PRECISIONS : PRECISIONS.filter((o) => o.value === 'standard')

  return (
    <div className="flex w-[616px] max-w-full flex-col items-center short:scale-[0.85]">
      <div role="radiogroup" aria-label={t('settings.analysis.precision.title')} className="flex gap-4">
        {options.map(({ value, icon: Icon }, i) => {
          const active = current === value || options.length === 1
          const maximum = value === 'maximum'
          return (
            <motion.button
              key={value}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => void choose(value)}
              initial={{ opacity: 0, y: 18, filter: 'blur(6px)' }}
              animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
              transition={{ type: 'spring', duration: 0.7, bounce: 0, delay: 0.2 + i * 0.08 }}
              whileTap={{ scale: 0.97 }}
              className={cn('focus-ring glass glass-hover relative flex h-[232px] w-[300px] flex-col rounded-[26px] p-6 text-left', active && 'glass-selected')}
            >
              <span className="flex items-start justify-between">
                <span
                  className={cn(
                    'flex size-12 items-center justify-center rounded-2xl transition-[background-color,color] duration-300',
                    active ? 'brand-fill' : 'bg-white/[0.06] text-white/60',
                  )}
                >
                  <Icon className="size-[22px]" strokeWidth={1.9} />
                </span>
                <span className="relative flex size-6 items-center justify-center">
                  <span className={cn('absolute inset-0 rounded-full border-[1.5px] transition-colors duration-300', active ? 'border-transparent' : 'border-white/20')} />
                  <AnimatePresence initial={false}>
                    {active && (
                      <motion.span
                        key="check"
                        className="absolute inset-0 flex items-center justify-center rounded-full bg-[#5840f0] text-white"
                        initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
                        animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
                        exit={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
                        transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
                      >
                        <Check className="size-3.5" strokeWidth={3} />
                      </motion.span>
                    )}
                  </AnimatePresence>
                </span>
              </span>
              <span className="mt-5 flex items-center gap-2">
                <span className="text-[20px] font-semibold tracking-[-0.01em] text-white">{t(`settings.analysis.precision.${value}.title`)}</span>
                {!maximum && <span className="rounded-full bg-success/15 px-2 py-0.5 text-[10px] font-semibold text-success">{t('settings.analysis.precision.recommended')}</span>}
              </span>
              <span className="mt-1 text-pretty text-[13px] leading-snug text-white/55">{t(`onboarding.precision.${value}Caption`)}</span>
              <span className="mt-auto flex items-center gap-3">
                {maximum && pack && pack.state.status !== 'absent' && pack.state.status !== 'ready' ? (
                  <PackBar state={pack.state} className="flex-1" />
                ) : (
                  <span className="rounded-full bg-white/[0.06] px-2.5 py-1 text-[11px] font-medium text-white/55">
                    {maximum && pack ? (pack.state.status === 'ready' ? t('settings.analysis.precision.ready') : formatBytes(pack.sizeBytes)) : t('settings.analysis.precision.included')}
                  </span>
                )}
              </span>
            </motion.button>
          )
        })}
      </div>
      {pack && <PackStatus pack={pack} wanted={current === 'maximum'} className="w-[616px] max-w-full px-2" />}
      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ type: 'spring', duration: 0.7, bounce: 0, delay: 0.4 }}
        className="glass mt-4 flex w-full items-center justify-between gap-6 rounded-2xl px-5 py-3.5"
      >
        <div className="min-w-0">
          <p className="text-[13px] font-medium text-white">{t('settings.general.background.title')}</p>
          <p className="mt-0.5 text-pretty text-xs leading-snug text-white/50">{t('onboarding.precision.backgroundCaption')}</p>
        </div>
        <Toggle isSelected={background} onValueChange={(on) => void update({ background: on })} aria-label={t('settings.general.background.title')} />
      </motion.div>
    </div>
  )
}
