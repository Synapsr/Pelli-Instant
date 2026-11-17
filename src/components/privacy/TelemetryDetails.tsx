import { AnimatePresence, motion } from 'framer-motion'
import { ChevronDown } from 'lucide-react'
import { useEffect, useId, useState } from 'react'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import type { TelemetryPreview } from '@/lib/contract'
import { useLanguage, useT } from '@/lib/i18n'
import { eventLabelKey, previewEvents, telemetryExample } from '@/lib/telemetry'
import { useAppStore } from '@/store/app-store'

/** What telemetry sends, from the engine's `telemetry_preview` (reloaded when the switch changes). */
export function useTelemetryPreview(): TelemetryPreview | null {
  const telemetry = useAppStore((s) => s.settings?.telemetry)
  const [preview, setPreview] = useState<TelemetryPreview | null>(null)
  useEffect(() => {
    let alive = true
    api()
      .telemetryPreview()
      .then((p) => alive && setPreview(p))
      .catch(() => {
        /* an engine without telemetry: the interface lists the known events */
      })
    return () => {
      alive = false
    }
  }, [telemetry])
  return preview
}

/** The event list, the common fields and an example payload. */
export function TelemetryContent({ preview, compact = false, className }: { preview: TelemetryPreview | null; compact?: boolean; className?: string }) {
  const t = useT()
  const language = useLanguage()
  const info = useAppStore((s) => s.appInfo)
  return (
    <div className={cn('text-left', className)}>
      <ul className="space-y-1.5">
        {previewEvents(preview).map((event) => {
          const key = eventLabelKey(event)
          return (
            <li key={event} className="flex items-baseline gap-2.5 text-[12px] leading-snug">
              <code className="shrink-0 rounded-md bg-white/[0.06] px-1.5 py-0.5 font-mono text-[10.5px] text-[#c9c1fb]">{event}</code>
              {key && <span className="text-default-500">{t(key)}</span>}
            </li>
          )
        })}
      </ul>
      <p className="mt-3 text-pretty text-[12px] leading-snug text-default-500">{t('settings.privacy.common')}</p>
      <p className="mt-2 text-pretty text-[12px] leading-snug text-default-500">{t('settings.privacy.linked')}</p>
      {!compact && (
        <>
          <p className="mt-3 text-[11px] font-medium text-default-400">{t('settings.privacy.example')}</p>
          <pre className="selectable mt-1.5 max-h-56 overflow-auto rounded-xl border border-white/[0.06] bg-black/30 p-3 font-mono text-[11px] leading-relaxed text-default-600">
            {telemetryExample(preview, info, language)}
          </pre>
        </>
      )}
    </div>
  )
}

/** A disclosure ("What exactly?") that unfolds the event list in place (the example payload lives in Settings → Privacy). */
export function TelemetryDisclosure({ label, className, panelClassName }: { label: string; className?: string; panelClassName?: string }) {
  const [open, setOpen] = useState(false)
  const preview = useTelemetryPreview()
  const id = useId()
  return (
    <div className={className}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        className="focus-ring -ml-2 inline-flex h-10 items-center gap-1 rounded-full px-2 text-[12px] font-medium text-[#b8adf8] transition-colors duration-150 hover:text-[#d4ceff]"
      >
        {label}
        <ChevronDown className={cn('size-3.5 transition-transform duration-200 ease-out', open && 'rotate-180')} aria-hidden />
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            id={id}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <TelemetryContent preview={preview} compact className={cn('pb-2 pt-1', panelClassName)} />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
