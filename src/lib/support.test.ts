import { describe, expect, it } from 'vitest'
import { isSupportCardVisible } from './support'

describe('support card', () => {
  const free = { offered: true, plusKnown: true, plusActive: false }

  it('shows when the sort that just ended offers it, in the free version', () => {
    expect(isSupportCardVisible(free)).toBe(true)
    expect(isSupportCardVisible({ ...free, offered: false })).toBe(false)
  })

  it('is hidden with Plus, and until a Plus status really arrived', () => {
    expect(isSupportCardVisible({ ...free, plusActive: true })).toBe(false)
    expect(isSupportCardVisible({ ...free, plusKnown: false })).toBe(false)
  })
})
