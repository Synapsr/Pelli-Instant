import { describe, expect, it } from 'vitest'
import { computeTransform } from './ZoomableImage'

const FIT = { active: false, x: 0.5, y: 0.5 }
const ACTUAL = (y: number) => ({ active: true, x: 0.5, y })

describe('loupe transform', () => {
  it('fits the photo above the band kept for the floating bar', () => {
    // A 3:2 photo in a 1100 × 760 loupe, 20 px padding, a 60 px band at the bottom.
    const view = computeTransform(1100, 760, 2048, 2048 / 1.5, FIT, 20, 60)
    expect(view.ty).toBeGreaterThanOrEqual(20)
    expect(view.ty + view.height).toBeLessThanOrEqual(760 - 60 - 20 + 1e-6)
    // Centred in the space above the band.
    expect(view.ty).toBeCloseTo((760 - 60 - view.height) / 2)
    // Without a band the photo uses the whole height, as in the compare panes.
    const plain = computeTransform(1100, 760, 2048, 2048 / 1.5, FIT, 20)
    expect(plain.height).toBeGreaterThan(view.height)
  })

  it('lets the bottom edge be panned above the band at 100 %', () => {
    const bottom = computeTransform(1100, 760, 2048, 2048 / 1.5, ACTUAL(1), 20, 60)
    expect(bottom.ty + bottom.height).toBeCloseTo(760 - 60)
    const top = computeTransform(1100, 760, 2048, 2048 / 1.5, ACTUAL(0), 20, 60)
    expect(top.ty).toBe(0)
  })

  it('uses the whole screen in full screen (no band, no padding)', () => {
    const view = computeTransform(1512, 982, 2048, 2048 / 1.5, FIT, 0, 0)
    expect(Math.min(view.tx, view.ty)).toBeCloseTo(0)
    expect(view.width).toBeCloseTo(1473, 0)
  })
})
