import { Button, Popover, PopoverContent, PopoverTrigger, Spinner } from '@heroui/react'
import { SlidersHorizontal } from 'lucide-react'
import { useRef, useState } from 'react'
import { SensitivityPicker } from '@/components/analysis/SensitivityPicker'
import { CriteriaList } from '@/components/criteria/CriteriaList'
import { toCullError } from '@/lib/api'
import type { CullSettings, DecisionCounts } from '@/lib/contract'
import { disabledDetections, normalizeCriteria } from '@/lib/criteria'
import { formatNumber, formatSignedNumber } from '@/lib/format'
import { t as translate, useT } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { useAnalysisStore } from '@/store/analysis-store'
import { useReviewStore } from '@/store/review-store'

/**
 * The criteria as the photographer edits them: shown at once, sent to the
 * engine one recompute at a time (changes made meanwhile are merged into the
 * next one), and confirmed once with the overall change in selected photos.
 */
function useLiveCriteria(settings: CullSettings | undefined) {
  const recompute = useReviewStore((s) => s.recompute)
  const [draft, setDraft] = useState<CullSettings | null>(null)
  const queue = useRef<{ running: boolean; next: CullSettings | null; before: DecisionCounts | null }>({ running: false, next: null, before: null })

  const run = async () => {
    const q = queue.current
    if (q.running || !q.next) return
    const target = q.next
    q.next = null
    q.running = true
    try {
      const result = await recompute(target)
      if (result) q.before ??= result.before
      if (!q.next) {
        setDraft(null)
        if (result && q.before) {
          const after = result.after
          notify({
            title: translate('review.recomputed', { selected: formatNumber(after.selected), delta: formatSignedNumber(after.selected - q.before.selected) }),
            tone: 'default',
            timeout: 2500,
          })
        }
        q.before = null
      }
    } catch (error) {
      q.next = null
      q.before = null
      setDraft(null)
      notify({ title: translate('errors.generic'), description: toCullError(error).message, tone: 'danger' })
    } finally {
      q.running = false
      void run()
    }
  }

  const value = draft ?? (settings ? normalizeCriteria(settings) : null)
  const apply = (patch: Partial<CullSettings>) => {
    if (!value) return
    const next = { ...value, ...patch }
    setDraft(next)
    queue.current.next = next
    void run()
  }
  return { value, apply }
}

/** Live re-run of the decisions on cached measures: instant, marks untouched. */
export function SensitivityPopover() {
  const t = useT()
  const settings = useReviewStore((s) => s.project?.settings)
  const recomputing = useReviewStore((s) => s.recomputing)
  const running = useAnalysisStore((s) => s.running)
  const [open, setOpen] = useState(false)
  const { value, apply } = useLiveCriteria(settings)
  if (!value) return null
  // Something the AI usually checks is off, or artistic blur is on.
  const custom = disabledDetections(value).length > 0 || value.artisticBlur

  return (
    <Popover placement="bottom-end" isOpen={open} onOpenChange={setOpen} offset={8} radius="lg">
      <PopoverTrigger>
        <Button
          size="sm"
          variant="light"
          radius="lg"
          isDisabled={running}
          className="h-8 gap-1.5 px-2.5 text-[13px] text-default-600"
          startContent={
            recomputing ? (
              <Spinner size="sm" color="current" classNames={{ wrapper: 'size-3.5' }} />
            ) : (
              <span className="relative">
                <SlidersHorizontal className="size-3.5" />
                {/* Badge on the icon: the bar keeps its width. */}
                {custom && <span aria-hidden className="absolute -right-[3px] -top-[3px] size-[7px] rounded-full bg-[#a599f5] ring-2 ring-[#161616]" />}
              </span>
            )
          }
          title={custom ? t('review.criteriaCustom') : undefined}
        >
          {/* Narrow windows keep only the value: long labels (German) would reach the centred tabs. */}
          <span className="max-[1279px]:sr-only">{t('review.sensitivity')}</span>
          <span className="font-semibold text-foreground">{t(`preAnalysis.${value.sensitivity}`)}</span>
          {custom && <span className="sr-only">{t('review.criteriaCustom')}</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[680px] max-w-[calc(100vw-32px)] items-stretch p-5">
        <p className="text-sm font-semibold text-foreground">{t('review.sensitivityTitle')}</p>
        <p className="mb-3 text-xs text-default-500">{t('review.sensitivityBody')}</p>
        <SensitivityPicker size="sm" value={value.sensitivity} onChange={(sensitivity) => apply({ sensitivity })} />
        <div className="mt-1 border-t border-default-200/60 pt-3 dark:border-white/[0.06]">
          <CriteriaList value={value} onChange={apply} density="compact" columns />
        </div>
      </PopoverContent>
    </Popover>
  )
}
