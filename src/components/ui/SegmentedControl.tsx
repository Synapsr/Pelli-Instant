import { useRef, type KeyboardEvent, type ReactNode } from 'react'
import { cn } from '@/lib/cn'

export interface SegmentOption<T extends string> {
  value: T
  label: ReactNode
  /** extra content after the label (a count…) */
  meta?: ReactNode
  disabled?: boolean
  title?: string
}

/**
 * Accessible segmented control (radio group): arrow keys move the choice,
 * the active segment slides with a soft violet gradient.
 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  label,
  size = 'md',
  fullWidth = false,
  className,
}: {
  value: T
  options: SegmentOption<T>[]
  onChange: (value: T) => void
  label: string
  size?: 'sm' | 'md'
  fullWidth?: boolean
  className?: string
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([])
  const enabled = options.filter((o) => !o.disabled)

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return
    e.preventDefault()
    e.stopPropagation()
    const index = enabled.findIndex((o) => o.value === value)
    let next = index
    if (e.key === 'ArrowLeft') next = Math.max(0, index - 1)
    if (e.key === 'ArrowRight') next = Math.min(enabled.length - 1, index + 1)
    if (e.key === 'Home') next = 0
    if (e.key === 'End') next = enabled.length - 1
    const option = enabled[next]
    if (option) {
      onChange(option.value)
      refs.current[options.indexOf(option)]?.focus()
    }
  }

  return (
    <div
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      className={cn(
        'inline-flex items-center gap-0.5 rounded-xl border border-default-200/70 bg-default-100/60 p-0.5 dark:border-white/[0.06] dark:bg-white/[0.03]',
        fullWidth && 'flex w-full',
        className,
      )}
    >
      {options.map((option, i) => {
        const active = option.value === value
        return (
          <button
            key={option.value}
            ref={(el) => {
              refs.current[i] = el
            }}
            type="button"
            role="radio"
            aria-checked={active}
            title={option.title}
            disabled={option.disabled}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(option.value)}
            className={cn(
              'focus-ring relative inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-[10px] font-medium transition-all duration-200 ease-out',
              size === 'sm' ? 'h-7 px-2.5 text-xs' : 'h-8 px-3 text-[13px]',
              fullWidth && 'flex-1',
              active
                ? 'bg-background text-foreground shadow-sm shadow-black/5 ring-1 ring-[#5840f0]/15 dark:bg-white/[0.09] dark:shadow-black/30 dark:ring-[#5840f0]/30'
                : 'text-default-500 hover:bg-default-200/50 hover:text-foreground dark:hover:bg-white/[0.04]',
              option.disabled && 'pointer-events-none opacity-40',
            )}
          >
            {option.label}
            {option.meta !== undefined && (
              <span className={cn('tabular text-[11px]', active ? 'text-default-500' : 'text-default-400')}>{option.meta}</span>
            )}
          </button>
        )
      })}
    </div>
  )
}
