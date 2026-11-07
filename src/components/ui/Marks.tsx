import { Star } from 'lucide-react'
import type { ColorLabel } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'

export const LABEL_COLOR: Record<Exclude<ColorLabel, 'none'>, string> = {
  red: '#ef4444',
  yellow: '#eab308',
  green: '#22c55e',
  blue: '#3b82f6',
  purple: '#a855f7',
}

export function LabelSwatch({ label, size = 8, className }: { label: ColorLabel; size?: number; className?: string }) {
  if (label === 'none') return null
  return <span aria-hidden className={cn('inline-block shrink-0 rounded-[3px]', className)} style={{ width: size, height: size, background: LABEL_COLOR[label] }} />
}

/** Compact read-only star rating. */
export function StarsBadge({ rating, className }: { rating: number; className?: string }) {
  const t = useT()
  if (rating <= 0) return null
  return (
    <span className={cn('inline-flex items-center gap-0.5 text-[#eab308]', className)} title={t('marks.stars', { count: rating })}>
      <Star className="size-3 fill-current" strokeWidth={0} aria-hidden />
      <span className="tabular text-[11px] font-medium">{rating}</span>
    </span>
  )
}

/** Editable rating (inspector, loupe header). */
export function StarsInput({ rating, onChange, className }: { rating: number; onChange: (rating: number) => void; className?: string }) {
  const t = useT()
  return (
    <div role="radiogroup" aria-label={t('marks.rating')} className={cn('inline-flex items-center', className)}>
      {[1, 2, 3, 4, 5].map((n) => {
        const on = n <= rating
        return (
          <button
            key={n}
            type="button"
            role="radio"
            aria-checked={rating === n}
            aria-label={t('marks.stars', { count: n })}
            title={`${t('marks.stars', { count: n })} (${n})`}
            onClick={() => onChange(rating === n ? 0 : n)}
            className="focus-ring rounded p-0.5 text-default-300 transition-colors duration-150 hover:text-[#eab308]"
          >
            <Star className={cn('size-4', on && 'fill-[#eab308] text-[#eab308]')} strokeWidth={1.6} />
          </button>
        )
      })}
    </div>
  )
}

export function LabelInput({ label, onChange }: { label: ColorLabel; onChange: (label: ColorLabel) => void }) {
  const t = useT()
  const options: Exclude<ColorLabel, 'none'>[] = ['red', 'yellow', 'green', 'blue']
  return (
    <div role="radiogroup" aria-label={t('marks.label')} className="inline-flex items-center gap-1">
      {options.map((l, i) => (
        <button
          key={l}
          type="button"
          role="radio"
          aria-checked={label === l}
          aria-label={t(`marks.labels.${l}`)}
          title={`${t(`marks.labels.${l}`)} (${i + 6})`}
          onClick={() => onChange(label === l ? 'none' : l)}
          className={cn(
            'focus-ring flex size-5 items-center justify-center rounded-md transition-transform duration-150 hover:scale-110',
            label === l && 'ring-2 ring-foreground/70 ring-offset-1 ring-offset-background',
          )}
        >
          <span className="size-3 rounded-[3px]" style={{ background: LABEL_COLOR[l] }} />
        </button>
      ))}
    </div>
  )
}
