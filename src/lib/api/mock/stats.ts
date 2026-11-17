import type { AiChanges, CameraStats, DecisionCounts, PhotoView, Precision, RejectionKey, ShootStats, SimilarGroup, SpeedStats, StatsOverview } from '@/lib/contract'

/** A shoot of the simulated engine, as `statsOverview` reads it. */
export interface MockStatsShoot {
  id: string
  name: string
  createdAt: number
  precision: Precision | null
  photos: PhotoView[]
  groups: SimilarGroup[]
}

/** One analysis that measured photos (the engine's `analysis_run`). */
export interface MockRun {
  precision: Precision
  photos: number
  durationMs: number
}

const TOP_CAMERAS = 5

/**
 * The statistics of `crates/cull-engine/src/stats.rs`, from the simulated
 * shoots: the totals for everyone, the details with Plus.
 */
export function mockStats(shoots: MockStatsShoot[], runs: MockRun[], plus: boolean): StatsOverview {
  const counts: DecisionCounts = { selected: 0, toReview: 0, rejected: 0 }
  const rejections: Partial<Record<RejectionKey, number>> = {}
  const changes: AiChanges = { calls: 0, rescued: 0, dropped: 0, toReview: 0, decided: 0 }
  const groups = { count: 0, photos: 0 }
  const months = new Map<string, { photos: number; selected: number }>()
  const cameras = new Map<string, CameraStats>()
  const rows: ShootStats[] = []

  for (const shoot of shoots) {
    const row: ShootStats = { id: shoot.id, name: shoot.name, shotAt: Infinity, photos: 0, counts: { selected: 0, toReview: 0, rejected: 0 }, precision: shoot.precision }
    for (const photo of shoot.photos) {
      if (photo.status !== 'analyzed') continue
      const verdict = photo.assessment?.verdict
      const flag = photo.mark.flag
      const selected = photo.decision === 'selected' ? 1 : 0
      row.photos += 1
      row.counts[photo.decision] += 1
      if (verdict === 'keep' || verdict === 'reject') {
        changes.calls += 1
        if (verdict === 'keep' && flag === 'reject') changes.dropped += 1
        if (verdict === 'reject' && flag === 'pick') changes.rescued += 1
      } else if (verdict === 'maybe') {
        changes.toReview += 1
        if (flag !== 'none') changes.decided += 1
      }
      if (photo.decision === 'rejected') {
        const key: RejectionKey = verdict === 'reject' ? (photo.assessment?.reasons[0] ?? 'other') : 'byYou'
        rejections[key] = (rejections[key] ?? 0) + 1
      }
      if (photo.capturedAt !== null) {
        row.shotAt = Math.min(row.shotAt, photo.capturedAt)
        const key = new Date(photo.capturedAt).toISOString().slice(0, 7)
        const month = months.get(key) ?? { photos: 0, selected: 0 }
        month.photos += 1
        month.selected += selected
        months.set(key, month)
      }
      const name = photo.exif.camera
      if (name) {
        const camera = cameras.get(name) ?? { name, photos: 0, selected: 0 }
        camera.photos += 1
        camera.selected += selected
        cameras.set(name, camera)
      }
    }
    if (row.photos === 0) continue
    if (row.shotAt === Infinity) row.shotAt = shoot.createdAt
    groups.count += shoot.groups.length
    groups.photos += shoot.groups.reduce((n, g) => n + g.photoIds.length, 0)
    counts.selected += row.counts.selected
    counts.toReview += row.counts.toReview
    counts.rejected += row.counts.rejected
    rows.push(row)
  }

  const photos = rows.reduce((n, r) => n + r.photos, 0)
  const since = rows.length ? Math.min(...rows.map((r) => r.shotAt)) : null
  if (!plus) return { photos, shoots: rows.length, since, detail: null }

  const speed: SpeedStats[] = (['standard', 'maximum'] as const).flatMap((precision) => {
    const of = runs.filter((r) => r.precision === precision)
    return of.length ? [{ precision, photos: of.reduce((n, r) => n + r.photos, 0), durationMs: of.reduce((n, r) => n + r.durationMs, 0) }] : []
  })
  return {
    photos,
    shoots: rows.length,
    since,
    detail: {
      counts,
      rejections,
      changes,
      groups,
      months: [...months.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([month, m]) => ({ month, ...m })),
      cameras: [...cameras.values()].sort((a, b) => b.photos - a.photos || a.name.localeCompare(b.name)).slice(0, TOP_CAMERAS),
      speed,
      shoots: rows.sort((a, b) => b.shotAt - a.shotAt || a.name.localeCompare(b.name)),
    },
  }
}
