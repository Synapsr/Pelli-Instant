import { SegmentedControl } from '@/components/ui/SegmentedControl'
import type { Sensitivity } from '@/lib/contract'
import { useT } from '@/lib/i18n'

export const SENSITIVITIES: Sensitivity[] = ['strict', 'balanced', 'lenient']

/** Stricte / Équilibrée / Souple with the explanation of the active choice. */
export function SensitivityPicker({ value, onChange, size = 'md' }: { value: Sensitivity; onChange: (value: Sensitivity) => void; size?: 'sm' | 'md' }) {
  const t = useT()
  return (
    <div>
      <SegmentedControl
        label={t('preAnalysis.sensitivity')}
        value={value}
        onChange={onChange}
        fullWidth
        size={size}
        options={SENSITIVITIES.map((s) => ({ value: s, label: t(`preAnalysis.${s}`) }))}
      />
      <p key={value} className="mt-2 min-h-[2.6em] text-[13px] leading-snug text-default-500 animate-fade-in">
        {t(`preAnalysis.${value}Hint`)}
      </p>
    </div>
  )
}
