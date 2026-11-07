import { Minus, Plus } from 'lucide-react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'

export function Stepper({
  value,
  min,
  max,
  onChange,
  label,
  disabled = false,
  size = 'md',
}: {
  value: number
  min: number
  max: number
  onChange: (value: number) => void
  label: string
  disabled?: boolean
  size?: 'sm' | 'md'
}) {
  const t = useT()
  const button = cn(
    'focus-ring flex items-center justify-center text-default-500 transition-colors duration-150 hover:bg-default-200/60 hover:text-foreground disabled:pointer-events-none disabled:opacity-30 dark:hover:bg-white/[0.06]',
    size === 'sm' ? 'size-7 rounded-[9px]' : 'size-8 rounded-[10px]',
  )
  return (
    <div
      role="group"
      aria-label={label}
      className={cn('inline-flex items-center gap-0.5 rounded-xl border border-default-200/70 bg-default-100/60 p-0.5 dark:border-white/[0.06] dark:bg-white/[0.03]', disabled && 'opacity-50')}
    >
      <button type="button" className={button} onClick={() => onChange(Math.max(min, value - 1))} disabled={disabled || value <= min} aria-label={t('preAnalysis.decrease')}>
        <Minus className="size-3.5" />
      </button>
      <output aria-live="polite" className={cn('tabular text-center font-semibold text-foreground', size === 'sm' ? 'w-6 text-[13px]' : 'w-7 text-sm')}>
        {value}
      </output>
      <button type="button" className={button} onClick={() => onChange(Math.min(max, value + 1))} disabled={disabled || value >= max} aria-label={t('preAnalysis.increase')}>
        <Plus className="size-3.5" />
      </button>
    </div>
  )
}
