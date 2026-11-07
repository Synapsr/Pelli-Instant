import { AnimatePresence, motion } from 'framer-motion'
import { Ban, Star } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { DecisionDot } from '@/components/ui/Decision'
import { LABEL_COLOR } from '@/components/ui/Marks'
import type { ColorLabel, FinalDecision } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { usePrefsStore } from '@/store/prefs-store'

const DECISIONS: FinalDecision[] = ['selected', 'toReview', 'rejected']
const LABELS: Exclude<ColorLabel, 'none'>[] = ['red', 'yellow', 'green', 'blue', 'purple']

/** Small toggle of the editor (24 px wide, 40 px tall to the pointer); `plain`: no background when on (stars). */
function Choice({ on, label, onPress, children, plain = false, className }: { on: boolean; label: string; onPress: () => void; children: ReactNode; plain?: boolean; className?: string }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      aria-label={label}
      title={label}
      onClick={onPress}
      className={cn(
        'focus-ring relative flex h-7 min-w-6 items-center justify-center rounded-md transition-[background-color,color,scale] duration-150 ease-out after:absolute after:inset-x-0 after:-inset-y-1.5 active:scale-[0.96]',
        on ? 'text-foreground' : 'text-default-400 hover:text-default-600',
        on && !plain ? 'bg-default-200/80 dark:bg-white/[0.12]' : 'hover:bg-default-200/50 dark:hover:bg-white/[0.06]',
        className,
      )}
    >
      {children}
    </button>
  )
}

/** Stars (or "reject") and colour of one decision, as the summary line shows them. */
function Written({ rating, label }: { rating: number; label: ColorLabel }) {
  const t = useT()
  if (rating === 0 && label === 'none') return <span className="text-default-400">–</span>
  return (
    <span className="inline-flex items-center gap-1.5">
      {rating > 0 && (
        <span role="img" aria-label={t('marks.stars', { count: rating })} className="flex text-[#eab308]">
          {Array.from({ length: rating }, (_, i) => (
            <Star key={i} className="size-3 fill-current" strokeWidth={0} />
          ))}
        </span>
      )}
      {rating < 0 && <span className="text-default-500">{t('export.xmpMap.reject')}</span>}
      {label !== 'none' && <span role="img" aria-label={t(`marks.labels.${label}`)} className="size-2.5 rounded-[3px]" style={{ background: LABEL_COLOR[label] }} />}
    </span>
  )
}

/**
 * What the XMP sidecars get for each decision, in one line, and a tiny editor
 * behind "Edit". Kept across sessions; the photographer's own stars and
 * labels always win (`keepUserMarks`).
 */
export function XmpMapping() {
  const t = useT()
  const mapping = usePrefsStore((s) => s.xmpMapping)
  const setMapping = usePrefsStore((s) => s.setXmpMapping)
  const [editing, setEditing] = useState(false)

  return (
    <div>
      <div className="flex items-center gap-3">
        <div role="group" aria-label={t('export.xmpMap.title')} className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3.5 gap-y-1 text-xs text-default-600">
          {DECISIONS.map((d) => (
            <span key={d} className="inline-flex items-center gap-1.5">
              <DecisionDot decision={d} size={6} />
              {t(`review.tabs.${d}`)}
              <Written rating={mapping.rating[d]} label={mapping.label[d]} />
            </span>
          ))}
        </div>
        <button
          type="button"
          aria-expanded={editing}
          onClick={() => setEditing(!editing)}
          className="focus-ring relative h-7 shrink-0 rounded-full px-2.5 text-xs font-semibold text-[#5840f0] transition-[background-color,scale] duration-150 ease-out after:absolute after:inset-x-0 after:-inset-y-1.5 hover:bg-[#5840f0]/10 active:scale-[0.96] dark:text-[#a599f5]"
        >
          {editing ? t('common.done') : t('export.xmpMap.edit')}
        </button>
      </div>
      <AnimatePresence initial={false}>
        {editing && (
          <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }} className="overflow-hidden">
            <div className="mt-2.5 grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-4 gap-y-1 rounded-xl bg-default-100/70 px-3 py-2 dark:bg-white/[0.04]">
              {DECISIONS.map((d) => {
                const name = t(`review.tabs.${d}`)
                const rating = mapping.rating[d]
                const label = mapping.label[d]
                return (
                  <div key={d} className="contents">
                    <span className="flex min-w-0 items-center gap-1.5 text-xs font-medium text-default-700">
                      <DecisionDot decision={d} size={6} />
                      <span className="truncate">{name}</span>
                    </span>
                    <div role="group" aria-label={`${name} · ${t('marks.rating')}`} className="flex items-center">
                      <Choice on={rating === 0} label={t('marks.noRating')} onPress={() => setMapping(d, { rating: 0 })}>
                        <Ban className="size-3.5" aria-hidden />
                      </Choice>
                      {[1, 2, 3, 4, 5].map((n) => (
                        <Choice key={n} plain on={rating === n} label={t('marks.stars', { count: n })} onPress={() => setMapping(d, { rating: n })}>
                          <Star className={cn('size-3.5', n <= rating ? 'fill-[#eab308] text-[#eab308]' : 'text-default-400')} strokeWidth={n <= rating ? 0 : 1.8} aria-hidden />
                        </Choice>
                      ))}
                      {d === 'rejected' && (
                        <Choice on={rating < 0} label={t('export.xmpMap.reject')} onPress={() => setMapping(d, { rating: -1 })} className="ml-1 px-2 text-[11px] font-semibold">
                          {t('export.xmpMap.reject')}
                        </Choice>
                      )}
                    </div>
                    <div role="group" aria-label={`${name} · ${t('marks.label')}`} className="flex items-center">
                      <Choice on={label === 'none'} label={t('marks.noLabel')} onPress={() => setMapping(d, { label: 'none' })}>
                        <Ban className="size-3.5" aria-hidden />
                      </Choice>
                      {LABELS.map((l) => (
                        <Choice key={l} on={label === l} label={t(`marks.labels.${l}`)} onPress={() => setMapping(d, { label: l })}>
                          <span className="size-3 rounded-[3px]" style={{ background: LABEL_COLOR[l] }} />
                        </Choice>
                      ))}
                    </div>
                  </div>
                )
              })}
              <p className="col-span-3 pt-1 text-[11px] leading-snug text-default-500">{t('export.xmpMap.userMarksWin')}</p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
