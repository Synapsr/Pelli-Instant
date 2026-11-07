import { Tooltip } from '@heroui/react'
import { cn } from '@/lib/cn'
import type { CullSettings } from '@/lib/contract'
import { CRITERIA_GROUPS, type Criterion, type CriterionKey } from '@/lib/criteria'
import { useT, type TKey } from '@/lib/i18n'
import { CRITERION_TEXT, criterionDisabled, criterionHint, criterionIcon } from './CriteriaList'

/** Short names, the review's own words for what it flags. */
const CHIP_TEXT: Record<CriterionKey, TKey> = {
  detectBlur: 'reasonPlural.blurry',
  detectMotionBlur: 'reasonPlural.motionBlur',
  detectSubjectFocus: 'reasonPlural.subjectOutOfFocus',
  artisticBlur: 'preAnalysis.artisticBlurShort',
  detectEyesClosed: 'reasonPlural.eyesClosed',
  detectUnderexposed: 'reasonPlural.underexposed',
  detectOverexposed: 'reasonPlural.overexposed',
  groupSimilar: 'reasonPlural.duplicate',
  detectExactDuplicates: 'reasonPlural.exactDuplicate',
}

const ALL: Criterion[] = CRITERIA_GROUPS.flatMap((g) => g.criteria)

/**
 * Every criterion at a glance, each chip a switch: a check is lit when on
 * and struck through when off; artistic blur, an option rather than a check,
 * comes last and lights up in violet. All chips are always there, so turning
 * one off never moves the others.
 */
export function CriteriaChips({ value, onChange }: { value: CullSettings; onChange: (patch: Partial<CullSettings>) => void }) {
  const checks = ALL.filter((c) => c.reason !== null)
  const options = ALL.filter((c) => c.reason === null)
  return (
    <ul className="flex flex-wrap items-center gap-1.5">
      {checks.map((c) => (
        <Chip key={c.key} criterion={c} value={value} onChange={onChange} />
      ))}
      <li aria-hidden className="mx-1 h-4 w-px bg-white/10" />
      {options.map((c) => (
        <Chip key={c.key} criterion={c} value={value} onChange={onChange} />
      ))}
    </ul>
  )
}

function Chip({ criterion, value, onChange }: { criterion: Criterion; value: CullSettings; onChange: (patch: Partial<CullSettings>) => void }) {
  const t = useT()
  const { key } = criterion
  const Icon = criterionIcon(criterion)
  const on = value[key]
  const option = criterion.reason === null
  const disabled = criterionDisabled(key, value)
  return (
    <li>
      <Tooltip content={t(criterionHint(key, value))} delay={500} closeDelay={0} placement="bottom">
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label={t(CRITERION_TEXT[key].label)}
          disabled={disabled}
          onClick={() => onChange({ [key]: !on })}
          className={cn(
            'focus-ring inline-flex h-7 items-center gap-1.5 rounded-full px-2.5 text-xs font-medium transition-[background-color,color,box-shadow,border-color,opacity] duration-200 active:scale-[0.97]',
            option
              ? on
                ? 'border border-transparent bg-[#5840f0]/20 text-foreground ring-1 ring-[#7c6af0]/45'
                : 'border border-dashed border-white/15 text-default-500 hover:border-white/25 hover:text-default-700'
              : on
                ? 'bg-white/[0.07] text-foreground ring-1 ring-white/[0.09] hover:bg-white/[0.1]'
                : 'text-default-400 ring-1 ring-white/[0.06] hover:text-default-600',
            disabled && 'pointer-events-none opacity-40',
          )}
        >
          <Icon className={cn('size-3.5 shrink-0', on ? 'text-[#a599f5]' : 'text-default-400')} strokeWidth={1.9} aria-hidden />
          <span className={cn(!on && !option && 'line-through decoration-default-400/70')}>{t(CHIP_TEXT[key])}</span>
          {key === 'groupSimilar' && on && value.keepPerGroup > 1 && <span className="tabular text-[11px] text-default-500">×{value.keepPerGroup}</span>}
        </button>
      </Tooltip>
    </li>
  )
}
