import { Microscope, Zap, type LucideIcon } from 'lucide-react'
import { cn } from '@/lib/cn'
import type { Precision } from '@/lib/contract'
import { formatBytes } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { usePackStore } from '@/store/pack-store'
import { PackBar, PackStatus } from './PackStatus'

export const PRECISIONS: { value: Precision; icon: LucideIcon }[] = [
  { value: 'standard', icon: Zap },
  { value: 'maximum', icon: Microscope },
]

/** The precision chosen, the pack and the action of choosing. Null before boot. */
export function usePrecision() {
  const settings = useAppStore((s) => s.settings)
  const pack = usePackStore((s) => s.pack)
  const choose = usePackStore((s) => s.choose)
  if (!settings) return null
  return { current: settings.precision, pack, choose }
}

/**
 * Settings version of the precision: Standard (shipped with the app) or
 * Maximum (an optional download of bigger models), then where its download
 * stands. Hidden when the engine has no Maximum pack.
 */
export function PrecisionChoice() {
  const t = useT()
  const precision = usePrecision()
  if (!precision?.pack) return null
  const { current, pack, choose } = precision
  return (
    <div>
      <div role="radiogroup" aria-label={t('settings.analysis.precision.title')} className="grid grid-cols-2 gap-2.5">
        {PRECISIONS.map(({ value, icon: Icon }) => {
          const active = current === value
          const maximum = value === 'maximum'
          return (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => void choose(value)}
              className={cn('focus-ring glass glass-hover relative flex flex-col rounded-2xl p-3.5 text-left transition-[scale] duration-150 active:scale-[0.98]', active && 'glass-selected')}
            >
              <span className="flex items-center gap-2">
                <span className={cn('flex size-7 items-center justify-center rounded-lg transition-[background-color,color] duration-300', active ? 'brand-fill' : 'bg-white/[0.06] text-default-500')}>
                  <Icon className="size-3.5" />
                </span>
                <span className="text-[13px] font-semibold text-foreground">{t(`settings.analysis.precision.${value}.title`)}</span>
                <span className="ml-auto rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-semibold text-default-500">
                  {maximum ? formatBytes(pack.sizeBytes) : t('settings.analysis.precision.included')}
                </span>
              </span>
              <span className="mt-2 block text-pretty text-xs leading-snug text-default-500">{t(`settings.analysis.precision.${value}.body`)}</span>
              {maximum && <PackBar state={pack.state} className="mt-3" />}
            </button>
          )
        })}
      </div>
      <PackStatus pack={pack} wanted={current === 'maximum'} removable />
    </div>
  )
}
