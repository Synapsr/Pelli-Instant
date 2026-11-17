import type { AiChanges, MonthStats, RejectionKey, SpeedStats, StatsDetail } from '@/lib/contract'

/**
 * Pure helpers of the Statistics view (`src/components/stats`), tested in
 * stats.test.ts. The numbers themselves come from the engine.
 */

/** Shoots the table lists before "Show all". */
export const SHOOTS_SHOWN = 8

export interface MonthBar {
  year: number
  /** 1 … 12 */
  month: number
  photos: number
  selected: number
}

/**
 * The months the chart shows, empty ones included: from the first month with
 * photos to the latest, at least 6 and at most the last 12.
 */
export function monthWindow(months: readonly MonthStats[]): MonthBar[] {
  if (!months.length) return []
  const index = (key: string) => {
    const [y, m] = key.split('-').map(Number)
    return y * 12 + (m - 1)
  }
  const keys = months.map((m) => index(m.month))
  const last = Math.max(...keys)
  const span = last - Math.min(...keys) + 1
  const count = Math.min(12, Math.max(6, span))
  const byIndex = new Map(months.map((m) => [index(m.month), m]))
  return Array.from({ length: count }, (_, i) => {
    const at = last - count + 1 + i
    const m = byIndex.get(at)
    return { year: Math.floor(at / 12), month: (at % 12) + 1, photos: m?.photos ?? 0, selected: m?.selected ?? 0 }
  })
}

/** Index of the month with the most photos, -1 when none has any. */
export function busiestMonth(bars: readonly MonthBar[]): number {
  let best = -1
  bars.forEach((b, i) => {
    if (b.photos > 0 && (best < 0 || b.photos > bars[best].photos)) best = i
  })
  return best
}

/** `part / total`, null without a total. */
export function share(part: number, total: number): number | null {
  return total > 0 ? part / total : null
}

/** Measured photos per second, null without a duration. */
export function photosPerSecond(speed: SpeedStats): number | null {
  return speed.durationMs > 0 && speed.photos > 0 ? speed.photos / (speed.durationMs / 1000) : null
}

/** Share of the AI's keep and reject calls the photographer turned around. */
export function changedShare(changes: AiChanges): number | null {
  return share(changes.rescued + changes.dropped, changes.calls)
}

/**
 * Rejected photos by reason: the AI's reasons, most frequent first, then the
 * ones it rejected on its score alone, then the photographer's own.
 */
export function rejectionRows(rejections: StatsDetail['rejections']): { key: RejectionKey; count: number }[] {
  const rank = (key: RejectionKey) => (key === 'byYou' ? 2 : key === 'other' ? 1 : 0)
  return (Object.entries(rejections) as [RejectionKey, number][])
    .filter(([, count]) => count > 0)
    .map(([key, count]) => ({ key, count }))
    .sort((a, b) => rank(a.key) - rank(b.key) || b.count - a.count)
}
