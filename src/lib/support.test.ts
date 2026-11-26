import { describe, expect, it } from 'vitest'
import { isSupportCardVisible } from './support'

describe('support card', () => {
  const free = { offered: true, plusLoaded: true, plusActive: false }

  it('shows when the sort that just ended offers it, in the free version', () => {
    expect(isSupportCardVisible(free)).toBe(true)
    expect(isSupportCardVisible({ ...free, offered: false })).toBe(false)
  })

  it('is hidden with Plus, and until the engine said whether Plus is active', () => {
    expect(isSupportCardVisible({ ...free, plusActive: true })).toBe(false)
    expect(isSupportCardVisible({ ...free, plusLoaded: false })).toBe(false)
  })
})
