import { describe, expect, it } from 'vitest'
import { busiestMonth, changedShare, monthWindow, photosPerSecond, rejectionRows, share } from './stats'

describe('monthWindow', () => {
  it('is empty without dated photos', () => {
    expect(monthWindow([])).toEqual([])
  })

  it('shows at least six months, empty ones included, up to the latest', () => {
    const bars = monthWindow([
      { month: '2026-10', photos: 40, selected: 10 },
      { month: '2026-08', photos: 12, selected: 3 },
    ])
    expect(bars.map((b) => `${b.year}-${b.month}`)).toEqual(['2026-5', '2026-6', '2026-7', '2026-8', '2026-9', '2026-10'])
    expect(bars.map((b) => b.photos)).toEqual([0, 0, 0, 12, 0, 40])
    expect(bars[5].selected).toBe(10)
  })

  it('crosses the year and keeps the last twelve months', () => {
    const bars = monthWindow([
      { month: '2024-03', photos: 5, selected: 1 },
      { month: '2025-01', photos: 7, selected: 2 },
      { month: '2025-12', photos: 9, selected: 4 },
    ])
    expect(bars).toHaveLength(12)
    expect(bars[0]).toEqual({ year: 2025, month: 1, photos: 7, selected: 2 })
    expect(bars[11]).toEqual({ year: 2025, month: 12, photos: 9, selected: 4 })
  })
})

describe('the figures', () => {
  it('finds the busiest month', () => {
    const bars = monthWindow([
      { month: '2026-06', photos: 3, selected: 1 },
      { month: '2026-07', photos: 9, selected: 1 },
    ])
    expect(busiestMonth(bars)).toBe(5)
    expect(busiestMonth(bars.map((b) => ({ ...b, photos: 0 })))).toBe(-1)
  })

  it('divides only by something', () => {
    expect(share(1, 4)).toBe(0.25)
    expect(share(0, 0)).toBeNull()
    expect(photosPerSecond({ precision: 'standard', photos: 380, durationMs: 20_000 })).toBe(19)
    expect(photosPerSecond({ precision: 'maximum', photos: 10, durationMs: 0 })).toBeNull()
    expect(changedShare({ calls: 200, rescued: 6, dropped: 4, toReview: 50, decided: 20 })).toBe(0.05)
    expect(changedShare({ calls: 0, rescued: 0, dropped: 0, toReview: 3, decided: 1 })).toBeNull()
  })

  it('lists the AI reasons first, then its score alone, then the photographer', () => {
    const rows = rejectionRows({ byYou: 30, other: 2, blurry: 12, duplicate: 40, eyesClosed: 0 })
    expect(rows).toEqual([
      { key: 'duplicate', count: 40 },
      { key: 'blurry', count: 12 },
      { key: 'other', count: 2 },
      { key: 'byYou', count: 30 },
    ])
  })
})
