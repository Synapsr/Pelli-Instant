import { describe, expect, it } from 'vitest'
import { SUPPORT_AFTER_EXPORTS, shouldAskSupport } from './support'

const base = { exportsDone: SUPPORT_AFTER_EXPORTS, closed: false, plusLoaded: true, plusActive: false }

describe('support card', () => {
  it('shows after a few successful exports', () => {
    expect(shouldAskSupport(base)).toBe(true)
    expect(shouldAskSupport({ ...base, exportsDone: SUPPORT_AFTER_EXPORTS + 5 })).toBe(true)
    expect(shouldAskSupport({ ...base, exportsDone: SUPPORT_AFTER_EXPORTS - 1 })).toBe(false)
    expect(shouldAskSupport({ ...base, exportsDone: 0 })).toBe(false)
  })

  it('never comes back once closed or used', () => {
    expect(shouldAskSupport({ ...base, closed: true })).toBe(false)
  })

  it('is hidden with Plus, and until the engine said whether Plus is active', () => {
    expect(shouldAskSupport({ ...base, plusActive: true })).toBe(false)
    expect(shouldAskSupport({ ...base, plusLoaded: false })).toBe(false)
  })
})
