import { ArrowDownRight, ArrowUpRight, Camera, ChevronDown, CircleDashed, Sparkles, TrendingDown, UserRound, type LucideIcon } from 'lucide-react'
import { useReducedMotion } from 'framer-motion'
import { useMemo, useState } from 'react'
import { REASON_ICON } from '@/components/ui/ReasonIcon'
import { AnimatedNumber } from '@/components/ui/AnimatedNumber'
import type { AiChanges, CameraStats, RejectionKey, ShootStats, SpeedStats, StatsDetail, StatsOverview } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatMonthYear, formatNumber, formatShootDay } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { changedShare, monthWindow, photosPerSecond, rejectionRows, share, SHOOTS_SHOWN } from '@/lib/stats'
import { Card, DecisionBar, DecisionCountsRow, Meter, MonthChart, Muted, percent, Reveal, Tile } from './parts'

/** Every statistic, with Plus. `onOpen` opens a shoot of the table. */
export function StatsDetailView({ stats, detail, onOpen }: { stats: StatsOverview; detail: StatsDetail; onOpen: (projectId: string) => void }) {
  const t = useT()
  const bars = useMemo(() => monthWindow(detail.months), [detail.months])
  const changes = detail.changes

  return (
    <div className="flex flex-col gap-3">
      <Reveal index={0} className="grid grid-cols-4 gap-3">
        <TotalTiles stats={stats} />
        <Tile
          label={t('stats.keepRate')}
          value={percent(share(detail.counts.selected, stats.photos))}
          hint={
            <>
              <DecisionBar counts={detail.counts} className="mt-1" />
              <div className="mt-2">
                <DecisionCountsRow counts={detail.counts} />
              </div>
            </>
          }
        />
        <Tile label={t('stats.changed')} value={percent(changedShare(changes))} hint={t('stats.of', { count: changes.rescued + changes.dropped, total: changes.calls })} />
      </Reveal>

      <Reveal index={1} className="grid grid-cols-[minmax(0,1.55fr)_minmax(0,1fr)] gap-3">
        <Card title={t('stats.months.title')} aside={t('stats.months.hint')}>
          {bars.length ? <MonthChart bars={bars} /> : <Muted>{t('stats.months.none')}</Muted>}
        </Card>
        <Card title={t('stats.rejections.title')}>
          <Rejections detail={detail} />
        </Card>
      </Reveal>

      <Reveal index={2} className="grid grid-cols-3 gap-3">
        <Card title={t('stats.ai.title')}>
          <YouAndTheAi changes={changes} />
        </Card>
        <Card title={t('stats.cameras.title')}>
          <Cameras cameras={detail.cameras} />
        </Card>
        <div className="flex min-w-0 flex-col gap-3">
          <Card title={t('stats.bursts.title')}>
            {detail.groups.count > 0 ? (
              <>
                <Figure value={formatNumber(detail.groups.count)} unit={t('stats.bursts.count', { count: detail.groups.count })} />
                <p className="mt-2 text-[12px] text-white/45">
                  {t('stats.bursts.frames', { count: formatNumber(detail.groups.photos / detail.groups.count, undefined, { maximumFractionDigits: 1 }) })}
                </p>
              </>
            ) : (
              <Muted>{t('stats.bursts.none')}</Muted>
            )}
          </Card>
          <Card title={t('stats.speed.title')}>
            <Speed speed={detail.speed} />
          </Card>
        </div>
      </Reveal>

      <Reveal index={3}>
        <ShootsTable shoots={detail.shoots} onOpen={onOpen} />
      </Reveal>
    </div>
  )
}

/** Photos sorted and shoots: the two figures everyone gets. */
export function TotalTiles({ stats }: { stats: StatsOverview }) {
  const t = useT()
  const reduce = useReducedMotion()
  return (
    <>
      <Tile
        hero
        label={t('stats.photos')}
        value={<AnimatedNumber value={stats.photos} from={reduce ? stats.photos : 0} duration={0.9} />}
        hint={stats.since !== null && <span className="first-letter:uppercase">{t('stats.since', { date: formatMonthYear(stats.since) })}</span>}
      />
      <Tile label={t('stats.shoots')} value={formatNumber(stats.shoots)} hint={stats.shoots > 0 && t('stats.perShoot', { count: Math.round(stats.photos / stats.shoots) })} />
    </>
  )
}

function Figure({ value, unit, small = false }: { value: string; unit: string; small?: boolean }) {
  return (
    <p className="flex items-baseline gap-2">
      <span className={cn('tabular font-semibold leading-none tracking-[-0.02em] text-white', small ? 'text-[20px]' : 'text-[26px]')}>{value}</span>
      <span className="text-[12.5px] text-white/55">{unit}</span>
    </p>
  )
}

/** Rows of rejections the card shows: the AI's top reasons, then the photographer's own. */
const REASON_ROWS = 5

function Rejections({ detail }: { detail: StatsDetail }) {
  const t = useT()
  const rows = rejectionRows(detail.rejections)
  if (!rows.length) return <Muted>{t('stats.rejections.none')}</Muted>
  const ai = rows.filter((r) => r.key !== 'byYou').slice(0, REASON_ROWS)
  const byYou = rows.find((r) => r.key === 'byYou')
  const max = Math.max(...rows.map((r) => r.count))
  const total = detail.counts.rejected
  const label = (key: RejectionKey) => (key === 'byYou' ? t('stats.rejections.byYou') : key === 'other' ? t('stats.rejections.other') : t(`reasonPlural.${key}`))
  const icon = (key: RejectionKey): LucideIcon => (key === 'byYou' ? UserRound : key === 'other' ? TrendingDown : REASON_ICON[key])
  return (
    <ul className="flex flex-col gap-2.5">
      {[...ai, ...(byYou ? [byYou] : [])].map((r, i) => {
        const Icon = icon(r.key)
        const mine = r.key === 'byYou'
        return (
          <li key={r.key} className={cn(mine && i > 0 && 'mt-1 border-t border-white/[0.06] pt-3')}>
            <div className="flex items-center gap-2.5">
              <span className={cn('flex size-6 shrink-0 items-center justify-center rounded-md', mine ? 'bg-white/[0.07] text-white/60' : 'bg-danger/[0.12] text-danger')}>
                <Icon className="size-3.5" strokeWidth={1.9} aria-hidden />
              </span>
              <span className="min-w-0 flex-1 truncate text-[12.5px] text-white/80">{label(r.key)}</span>
              <span className="tabular text-[12.5px] font-medium text-white">{formatNumber(r.count)}</span>
              <span className="tabular w-10 text-right text-[11.5px] text-white/40">{percent(share(r.count, total))}</span>
            </div>
            <Meter value={r.count / max} tone={mine ? 'muted' : 'danger'} index={i} className="ml-[34px] mt-1.5" />
          </li>
        )
      })}
    </ul>
  )
}

function YouAndTheAi({ changes }: { changes: AiChanges }) {
  const t = useT()
  const rows: { icon: LucideIcon; tone: string; label: string; value: number }[] = [
    { icon: ArrowUpRight, tone: 'bg-success/[0.12] text-success', label: t('stats.ai.rescued'), value: changes.rescued },
    { icon: ArrowDownRight, tone: 'bg-danger/[0.12] text-danger', label: t('stats.ai.dropped'), value: changes.dropped },
    { icon: CircleDashed, tone: 'bg-warning/[0.12] text-warning', label: t('stats.ai.decided'), value: changes.decided },
  ]
  return (
    <>
      <ul className="flex flex-col gap-3.5">
        {rows.map(({ icon: Icon, tone, label, value }, i) => (
          <li key={label}>
            <div className="flex items-center gap-3">
              <span className={cn('flex size-8 shrink-0 items-center justify-center rounded-xl', tone)}>
                <Icon className="size-4" strokeWidth={2} aria-hidden />
              </span>
              <span className="min-w-0 flex-1 text-pretty text-[12.5px] leading-snug text-white/70">{label}</span>
              <span className="tabular text-[18px] font-semibold tracking-[-0.01em] text-white">{formatNumber(value)}</span>
            </div>
            {i === 2 && changes.toReview > 0 && (
              <div className="ml-11 mt-1.5 flex items-center gap-2.5">
                <Meter value={changes.decided / changes.toReview} tone="warning" />
                <span className="tabular shrink-0 text-[11px] text-white/40">{t('stats.of', { count: changes.decided, total: changes.toReview })}</span>
              </div>
            )}
          </li>
        ))}
      </ul>
      <p className="mt-auto pt-4 text-pretty text-[11.5px] leading-snug text-white/40">{t('stats.ai.note')}</p>
    </>
  )
}

function Cameras({ cameras }: { cameras: CameraStats[] }) {
  const t = useT()
  if (!cameras.length) return <Muted>{t('stats.cameras.none')}</Muted>
  return (
    <ul className="flex flex-col gap-3.5">
      {cameras.map((c, i) => (
        <li key={c.name}>
          <div className="flex items-baseline justify-between gap-3">
            <span className="flex min-w-0 items-center gap-2 text-[13px] text-white/85">
              <Camera className="size-3.5 shrink-0 translate-y-px text-white/35" strokeWidth={1.9} aria-hidden />
              <span className="truncate">{c.name}</span>
            </span>
            <span className="tabular shrink-0 text-[11.5px] text-white/45">{t('common.photos', { count: c.photos })}</span>
          </div>
          <div className="mt-1.5 flex items-center gap-2.5 pl-[22px]">
            <Meter value={c.selected / c.photos} tone="success" index={i} />
            <span className="tabular shrink-0 text-[11px] text-white/45">{t('stats.cameras.kept', { rate: percent(share(c.selected, c.photos)) })}</span>
          </div>
        </li>
      ))}
    </ul>
  )
}

function Speed({ speed }: { speed: SpeedStats[] }) {
  const t = useT()
  const rows = speed.flatMap((s) => {
    const rate = photosPerSecond(s)
    return rate === null ? [] : [{ ...s, rate }]
  })
  if (!rows.length) return <Muted>{t('stats.speed.none')}</Muted>
  return (
    <div className="flex flex-col gap-3">
      {rows.map((s, i) => (
        <div key={s.precision}>
          <Figure small={i > 0} value={formatNumber(s.rate, undefined, { maximumFractionDigits: 1 })} unit={t('stats.speed.unit')} />
          <p className="mt-1.5 text-[12px] text-white/45">
            {t('stats.speed.on', { precision: t(`settings.analysis.precision.${s.precision}.title`), photos: t('common.photos', { count: s.photos }) })}
          </p>
        </div>
      ))}
    </div>
  )
}

function ShootsTable({ shoots, onOpen }: { shoots: ShootStats[]; onOpen: (projectId: string) => void }) {
  const t = useT()
  const [all, setAll] = useState(false)
  const shown = all ? shoots : shoots.slice(0, SHOOTS_SHOWN)
  const cols = 'grid grid-cols-[minmax(0,1fr)_96px_84px_minmax(150px,210px)_92px] items-center gap-4'
  return (
    <section className="glass rounded-[22px] p-2">
      <header className="flex items-baseline justify-between gap-3 px-3 pb-1 pt-3">
        <h3 className="text-[14px] font-semibold tracking-[-0.01em] text-white">{t('stats.table.title')}</h3>
        <span className="tabular text-[11.5px] text-white/40">{formatNumber(shoots.length)}</span>
      </header>
      <div aria-hidden className={cn(cols, 'px-3 py-2 text-[10.5px] font-semibold uppercase tracking-[0.1em] text-white/35')}>
        <span>{t('stats.table.name')}</span>
        <span>{t('stats.table.date')}</span>
        <span className="text-right">{t('stats.table.photos')}</span>
        <span>{t('stats.table.kept')}</span>
        <span>{t('stats.table.precision')}</span>
      </div>
      <ul>
        {shown.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => onOpen(s.id)}
              aria-label={`${t('home.open')} ${s.name}`}
              className={cn(cols, 'focus-ring group w-full rounded-xl px-3 py-2.5 text-left transition-colors duration-150 hover:bg-white/[0.05]')}
            >
              <span className="truncate text-[13px] font-medium text-white">{s.name}</span>
              <span className="tabular truncate text-[12px] text-white/50">{formatShootDay(s.shotAt)}</span>
              <span className="tabular text-right text-[12px] text-white/70">{formatNumber(s.photos)}</span>
              <span className="flex min-w-0 items-center gap-2.5">
                <DecisionBar counts={s.counts} className="flex-1" />
                <span className="tabular w-10 shrink-0 text-right text-[12px] text-white/70">{percent(share(s.counts.selected, s.photos))}</span>
              </span>
              <span className="min-w-0">
                {s.precision === 'maximum' ? (
                  <span className="inline-flex h-5 items-center gap-1 rounded-full bg-[#5840f0]/25 px-2 text-[10.5px] font-semibold text-[#d4ceff] ring-1 ring-inset ring-[#a599f5]/30">
                    <Sparkles className="size-2.5" strokeWidth={2.4} aria-hidden />
                    {t('settings.analysis.precision.maximum.title')}
                  </span>
                ) : (
                  <span className="text-[11.5px] text-white/40">{t('settings.analysis.precision.standard.title')}</span>
                )}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {shoots.length > SHOOTS_SHOWN && (
        <div className="flex justify-center px-3 pb-2 pt-1">
          <button
            type="button"
            onClick={() => setAll((v) => !v)}
            className="focus-ring group inline-flex h-9 items-center gap-1.5 rounded-full px-3.5 text-[12px] font-medium text-white/55 transition-[color,background-color] duration-150 hover:bg-white/[0.05] hover:text-white"
          >
            <span className="tabular">{all ? t('stats.table.showLess') : t('home.seeAll', { count: shoots.length })}</span>
            <ChevronDown className={cn('size-3.5 transition-transform duration-200', all && 'rotate-180')} aria-hidden />
          </button>
        </div>
      )}
    </section>
  )
}
