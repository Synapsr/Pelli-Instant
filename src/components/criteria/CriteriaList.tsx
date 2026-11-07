import { Aperture, type LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { REASON_ICON } from '@/components/ui/ReasonIcon'
import { SettingRow } from '@/components/ui/SettingRow'
import { Stepper } from '@/components/ui/Stepper'
import { Toggle } from '@/components/ui/Toggle'
import { cn } from '@/lib/cn'
import type { CullSettings } from '@/lib/contract'
import { checksSharpness, CRITERIA_GROUPS, type CriteriaGroupId, type Criterion, type CriterionKey } from '@/lib/criteria'
import { useT, type TKey } from '@/lib/i18n'

/** Label and one-line hint of each criterion. */
export const CRITERION_TEXT: Record<CriterionKey, { label: TKey; hint: TKey }> = {
  detectBlur: { label: 'preAnalysis.blurry', hint: 'preAnalysis.blurryHint' },
  detectMotionBlur: { label: 'preAnalysis.motionBlur', hint: 'preAnalysis.motionBlurHint' },
  detectSubjectFocus: { label: 'preAnalysis.subjectOutOfFocus', hint: 'preAnalysis.subjectOutOfFocusHint' },
  artisticBlur: { label: 'preAnalysis.artisticBlur', hint: 'preAnalysis.artisticBlurHint' },
  detectEyesClosed: { label: 'preAnalysis.eyesClosed', hint: 'preAnalysis.eyesClosedHint' },
  detectUnderexposed: { label: 'preAnalysis.underexposed', hint: 'preAnalysis.underexposedHint' },
  detectOverexposed: { label: 'preAnalysis.overexposed', hint: 'preAnalysis.overexposedHint' },
  groupSimilar: { label: 'preAnalysis.groupSimilar', hint: 'preAnalysis.groupSimilarHint' },
  detectExactDuplicates: { label: 'preAnalysis.exactDuplicates', hint: 'preAnalysis.exactDuplicatesHint' },
}

export function criterionIcon(c: Criterion): LucideIcon {
  return c.reason ? REASON_ICON[c.reason] : Aperture
}

/**
 * Artistic blur only shapes the blur detections (inactive with all of them
 * off), and the subject's focus is not judged while it is on.
 */
export function criterionDisabled(key: CriterionKey, value: CullSettings): boolean {
  if (key === 'artisticBlur') return !checksSharpness(value)
  if (key === 'detectSubjectFocus') return value.artisticBlur
  return false
}

/** Hint of a criterion, saying why it is inactive when it is. */
export function criterionHint(key: CriterionKey, value: CullSettings): TKey {
  return key === 'detectSubjectFocus' && value.artisticBlur ? 'preAnalysis.subjectOutOfFocusArtistic' : CRITERION_TEXT[key].hint
}

type Density = 'comfortable' | 'compact'

interface Props {
  value: CullSettings
  onChange: (patch: Partial<CullSettings>) => void
  /** comfortable: the settings rows; compact: dense rows for a popover or a dialog */
  density?: Density
  /** two columns (Sharpness and Eyes | Exposure and Similar photos) */
  columns?: boolean
  disabled?: boolean
}

/**
 * The culling criteria, grouped the way a photographer looks at a photo:
 * sharpness, eyes, exposure, similar photos. One switch per thing the AI
 * flags, with its one-line hint.
 */
export function CriteriaList({ value, onChange, density = 'comfortable', columns = false, disabled = false }: Props) {
  const groups = (ids: CriteriaGroupId[]) => CRITERIA_GROUPS.filter((g) => ids.includes(g.id)).map((g) => <Group key={g.id} id={g.id} criteria={g.criteria} value={value} onChange={onChange} density={density} disabled={disabled} />)
  if (!columns) return <div className={density === 'compact' ? 'space-y-3' : 'space-y-6'}>{groups(['sharpness', 'eyes', 'exposure', 'similar'])}</div>
  return (
    <div className="grid grid-cols-2 gap-x-6">
      <div className="space-y-3">{groups(['sharpness', 'eyes'])}</div>
      <div className="space-y-3">{groups(['exposure', 'similar'])}</div>
    </div>
  )
}

function Group({ id, criteria, value, onChange, density, disabled }: { id: CriteriaGroupId; criteria: Criterion[] } & Required<Omit<Props, 'columns'>>) {
  const t = useT()
  const titleId = `criteria-${id}-${density}`
  return (
    <section aria-labelledby={titleId}>
      <h4 id={titleId} className={cn('font-semibold text-default-400', density === 'compact' ? 'mb-0.5 text-[11px] uppercase tracking-[0.06em]' : 'mb-1 text-[11px] uppercase tracking-[0.06em]')}>
        {t(`preAnalysis.groups.${id}`)}
      </h4>
      <div>
        {criteria.map((c) => (
          <CriterionLine key={c.key} criterion={c} value={value} onChange={onChange} density={density} disabled={disabled}>
            {/* Kept count right under the grouping it belongs to. */}
            {c.key === 'groupSimilar' && <KeepPerGroup value={value} onChange={onChange} density={density} disabled={disabled} />}
          </CriterionLine>
        ))}
      </div>
    </section>
  )
}

function CriterionLine({ criterion, value, onChange, density, disabled, children }: { criterion: Criterion; children?: ReactNode } & Required<Omit<Props, 'columns'>>) {
  const t = useT()
  const { key } = criterion
  const text = CRITERION_TEXT[key]
  const Icon = criterionIcon(criterion)
  const off = disabled || criterionDisabled(key, value)
  const toggle = <Toggle isSelected={value[key]} isDisabled={off} onValueChange={(on) => onChange({ [key]: on })} aria-label={t(text.label)} />
  const hint = t(criterionHint(key, value))
  // In the settings, artistic blur also says exactly what it changes.
  const fullHint =
    key === 'artisticBlur' ? (
      <>
        <p>{hint}</p>
        <p className="mt-1 text-default-400">{t('preAnalysis.artisticBlurDetail')}</p>
      </>
    ) : (
      hint
    )
  return (
    <>
      {density === 'comfortable' ? (
        <SettingRow label={t(text.label)} hint={fullHint} icon={<Icon className="size-4" strokeWidth={1.9} />} disabled={off}>
          {toggle}
        </SettingRow>
      ) : (
        <CompactLine icon={Icon} label={t(text.label)} hint={hint} disabled={off} accent={key === 'artisticBlur' && value.artisticBlur}>
          {toggle}
        </CompactLine>
      )}
      {children}
    </>
  )
}

function KeepPerGroup({ value, onChange, density, disabled }: Required<Omit<Props, 'columns'>>) {
  const t = useT()
  const off = disabled || !value.groupSimilar
  const stepper = (
    <Stepper label={t('preAnalysis.keepPerGroup')} value={value.keepPerGroup} min={1} max={5} disabled={off} size={density === 'compact' ? 'sm' : 'md'} onChange={(keepPerGroup) => onChange({ keepPerGroup })} />
  )
  return density === 'comfortable' ? (
    <SettingRow label={t('preAnalysis.keepPerGroup')} hint={t('preAnalysis.keepPerGroupHint')} icon={<span className="block size-4" />} disabled={off}>
      {stepper}
    </SettingRow>
  ) : (
    // A setting of the grouping above it: one line, no hint.
    <CompactLine label={t('preAnalysis.keepPerGroup')} disabled={off}>
      {stepper}
    </CompactLine>
  )
}

/** Dense line: icon, label over its hint, control. */
function CompactLine({ icon: Icon, label, hint, disabled, accent = false, children }: { icon?: LucideIcon; label: string; hint?: string; disabled: boolean; accent?: boolean; children: ReactNode }) {
  return (
    <div className={cn('flex items-center gap-2.5 py-1.5 transition-opacity duration-200 short:py-[5px]', disabled && 'opacity-45')}>
      <span className={cn('flex size-4 shrink-0 items-center justify-center transition-colors duration-200', hint && 'self-start pt-[3px]', accent ? 'text-[#a599f5]' : 'text-default-400')} aria-hidden>
        {Icon && <Icon className="size-4" strokeWidth={1.9} />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] font-medium leading-snug text-foreground">{label}</span>
        {hint && <span className="block text-[11.5px] leading-snug text-default-500">{hint}</span>}
      </span>
      <span className="shrink-0">{children}</span>
    </div>
  )
}
