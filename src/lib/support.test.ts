import { describe, expect, it } from 'vitest'
import { isSuccessfulRun, isSupportCardDue, type RunOutcome, SUPPORT_AFTER_EXPORTS, shouldShowSupportCard } from './support'

const clean: RunOutcome = { phase: 'done', error: null, report: { errors: [] }, withUpload: false, uploadReport: null }
const sent: RunOutcome = { ...clean, withUpload: true, uploadReport: { failed: 0 } }

describe('a successful run', () => {
  it('writes every file, and sends every photo when there is a gallery', () => {
    expect(isSuccessfulRun(clean)).toBe(true)
    expect(isSuccessfulRun(sent)).toBe(true)
  })

  it('is not a partial export, a failed or cancelled upload, an error or a run still going', () => {
    expect(isSuccessfulRun({ ...clean, report: { errors: [{ photoId: 'p1', message: 'denied' }] } })).toBe(false)
    expect(isSuccessfulRun({ ...sent, report: { errors: [{ photoId: 'p1', message: 'denied' }] } })).toBe(false)
    expect(isSuccessfulRun({ ...sent, uploadReport: { failed: 2 } })).toBe(false)
    // A cancelled upload ends without error and without an upload report.
    expect(isSuccessfulRun({ ...sent, uploadReport: null })).toBe(false)
    expect(isSuccessfulRun({ ...clean, error: 'No network' })).toBe(false)
    expect(isSuccessfulRun({ ...clean, report: null })).toBe(false)
    expect(isSuccessfulRun({ ...sent, phase: 'uploading', uploadReport: null })).toBe(false)
  })
})

describe('support card', () => {
  it('is due after a few successful exports, if it was never shown', () => {
    expect(isSupportCardDue({ exportsDone: SUPPORT_AFTER_EXPORTS, shown: false })).toBe(true)
    expect(isSupportCardDue({ exportsDone: SUPPORT_AFTER_EXPORTS + 5, shown: false })).toBe(true)
    expect(isSupportCardDue({ exportsDone: SUPPORT_AFTER_EXPORTS - 1, shown: false })).toBe(false)
    expect(isSupportCardDue({ exportsDone: 0, shown: false })).toBe(false)
    expect(isSupportCardDue({ exportsDone: SUPPORT_AFTER_EXPORTS + 5, shown: true })).toBe(false)
  })

  const open = { due: true, succeeded: true, plusLoaded: true, plusActive: false }

  it('shows only on the report of a successful run', () => {
    expect(shouldShowSupportCard(open)).toBe(true)
    expect(shouldShowSupportCard({ ...open, succeeded: false })).toBe(false)
    expect(shouldShowSupportCard({ ...open, due: false })).toBe(false)
  })

  it('is hidden with Plus, until the engine said whether Plus is active, and as soon as Plus activates', () => {
    expect(shouldShowSupportCard({ ...open, plusLoaded: false })).toBe(false)
    expect(shouldShowSupportCard({ ...open, plusActive: true })).toBe(false)
  })
})
