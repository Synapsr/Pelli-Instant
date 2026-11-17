import { Button } from '@heroui/react'
import { ArrowRight, ChartColumn, Lock } from 'lucide-react'
import type { StatsOverview } from '@/lib/contract'
import { useT } from '@/lib/i18n'
import { usePlusStore } from '@/store/plus-store'
import { Reveal } from './parts'
import { TotalTiles } from './StatsDetail'

/* Shapes of the blurred preview: decoration, not data. */
const PREVIEW_BARS = [34, 48, 41, 62, 88, 72, 95, 58, 66, 44, 52, 30]
const PREVIEW_ROWS = [92, 70, 54, 38, 26]

/**
 * The free version: its two real totals, then what Plus adds, as a blurred
 * preview with no number in it and the way to Plus.
 */
export function StatsLocked({ stats }: { stats: StatsOverview }) {
  const t = useT()
  const openDialog = usePlusStore((s) => s.openDialog)
  const unlock = () => openDialog('stats')
  return (
    <div className="flex flex-col gap-3">
      <Reveal index={0} className="grid grid-cols-4 gap-3">
        <TotalTiles stats={stats} />
        {(['stats.keepRate', 'stats.changed'] as const).map((key) => (
          <button key={key} type="button" onClick={unlock} className="glass glass-hover focus-ring min-w-0 rounded-[20px] px-5 pb-4 pt-[18px] text-left">
            <span className="block truncate text-[11px] font-semibold uppercase tracking-[0.12em] text-white/45">{t(key)}</span>
            <span aria-hidden className="mt-2.5 flex h-8 items-center gap-1.5 short:h-7">
              <span className="h-6 w-[72px] rounded-lg bg-white/[0.08]" />
              <span className="h-6 w-5 rounded-lg bg-white/[0.05]" />
            </span>
            <span className="mt-3 inline-flex items-center gap-1.5 text-[12px] font-medium text-[#b8adf8]">
              <Lock className="size-3" strokeWidth={2.2} aria-hidden />
              {t('stats.locked.tile')}
            </span>
          </button>
        ))}
      </Reveal>

      <Reveal index={1}>
        <section className="glass relative isolate overflow-hidden rounded-[24px]">
          <Preview />
          <div aria-hidden className="absolute inset-0 -z-10 bg-[radial-gradient(50%_60%_at_50%_50%,rgba(13,11,22,0.86),rgba(13,11,22,0.3))]" />
          <span aria-hidden className="pointer-events-none absolute left-1/2 top-0 -z-10 h-64 w-[560px] -translate-x-1/2 -translate-y-1/3 rounded-full bg-[radial-gradient(closest-side,rgba(88,64,240,0.35),transparent)]" />
          <div className="flex min-h-[clamp(340px,calc(100vh-300px),560px)] flex-col items-center justify-center px-8 py-12 text-center short:py-9">
            <span className="brand-fill flex size-12 items-center justify-center rounded-2xl shadow-[0_12px_32px_-10px_rgba(88,64,240,0.9)]" aria-hidden>
              <ChartColumn className="size-[22px]" strokeWidth={1.9} />
            </span>
            <p className="mt-4 text-[11px] font-semibold uppercase tracking-[0.16em] text-[#c9c1ff]">{t('plus.name')}</p>
            <h3 className="mt-1.5 max-w-[480px] text-balance text-[24px] font-semibold leading-tight tracking-[-0.02em] text-white">{t('stats.locked.title')}</h3>
            <p className="mt-2.5 max-w-[480px] text-pretty text-[13.5px] leading-relaxed text-white/60">{t('stats.locked.body')}</p>
            <Button radius="full" className="btn-brand mt-6 h-12 px-7 text-[15px] font-semibold" endContent={<ArrowRight className="size-4" />} onPress={unlock}>
              {t('stats.locked.cta')}
            </Button>
          </div>
        </section>
      </Reveal>
    </div>
  )
}

/** The look of the statistics behind frosted glass: bars and rows, never a figure. */
function Preview() {
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 -z-20 grid grid-rows-[1.5fr_1fr] gap-3 p-5 blur-[6px]">
      <div className="grid min-h-0 grid-cols-[1.55fr_1fr] gap-3">
        <div className="flex min-h-0 flex-col rounded-[18px] bg-white/[0.03] p-5">
          <span className="h-3 w-28 shrink-0 rounded-full bg-white/15" />
          <div className="mt-5 flex min-h-0 flex-1 items-end gap-[6px]">
            {PREVIEW_BARS.map((h, i) => (
              <span key={i} className="relative flex-1 overflow-hidden rounded-t-[7px] bg-[#5840f0]/35" style={{ height: `${h}%` }}>
                <span className="absolute inset-x-0 bottom-0 bg-[#7c6af0]" style={{ height: '38%' }} />
              </span>
            ))}
          </div>
        </div>
        <div className="flex min-h-0 flex-col gap-4 overflow-hidden rounded-[18px] bg-white/[0.03] p-5">
          <span className="h-3 w-36 shrink-0 rounded-full bg-white/15" />
          {PREVIEW_ROWS.map((w, i) => (
            <span key={i} className="flex shrink-0 items-center gap-3">
              <span className="size-6 rounded-md bg-danger/25" />
              <span className="h-2 rounded-full bg-danger/40" style={{ width: `${w}%` }} />
            </span>
          ))}
        </div>
      </div>
      <div className="grid min-h-0 grid-cols-3 gap-3">
        {['bg-success/40', 'bg-white/20', 'bg-[#7c6af0]/50'].map((bar, i) => (
          <div key={i} className="flex flex-col gap-4 rounded-[18px] bg-white/[0.03] p-5">
            <span className="h-3 w-24 rounded-full bg-white/15" />
            <span className="h-7 w-20 rounded-lg bg-white/[0.12]" />
            <span className={`h-2 w-3/4 rounded-full ${bar}`} />
          </div>
        ))}
      </div>
    </div>
  )
}
