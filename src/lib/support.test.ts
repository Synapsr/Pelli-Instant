import { describe, expect, it } from 'vitest'
import { isSupportVisible } from './support'

describe('support dialog', () => {
  const free = { offered: true, plusKnown: true, plusActive: false }

  it('shows when the sort offers it, in the free version', () => {
    expect(isSupportVisible(free)).toBe(true)
    expect(isSupportVisible({ ...free, offered: false })).toBe(false)
  })

  it('is hidden with Plus, and until a Plus status really arrived', () => {
    expect(isSupportVisible({ ...free, plusActive: true })).toBe(false)
    expect(isSupportVisible({ ...free, plusKnown: false })).toBe(false)
  })
})
