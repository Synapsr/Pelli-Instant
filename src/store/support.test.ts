import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { MOCK_SETTINGS } from '@/lib/api/mock/fixtures'
import type { AnalysisSummary, CullApi } from '@/lib/contract'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { setNotifier } from '@/lib/notify'
import { NO_PLUS } from '@/lib/plus'
import type { KeyTarget } from '@/lib/review/keyboard'
import { useAnalysisStore } from './analysis-store'
import { useAppStore } from './app-store'
import { usePlusStore } from './plus-store'
import { closeSupportDialog, followFromSupportDialog, getPlusFromSupportDialog, goToReview, resultKeyDown, supportDialogVisible } from './support'

const PROJECT = 'prj-lea-thomas'
const SUMMARY: AnalysisSummary = {
  projectId: PROJECT,
  analyzed: 10,
  failed: 0,
  durationMs: 4000,
  counts: { selected: 4, toReview: 3, rejected: 3 },
  reasons: {},
  groups: 1,
  precision: 'standard',
}
const LICENCE = { ...NO_PLUS, active: true, source: 'license' as const, key: 'ABCD-EFGH-JKLM-NPQR' }

/** Where the focus is when a key is pressed on the result screen. */
const SURFACE: KeyTarget = { closest: () => null }
const A_BUTTON: KeyTarget = { closest: (selector: string) => (selector.includes('button') ? {} : null) }
const press = (key: string, target: KeyTarget = SURFACE) => resultKeyDown({ key, target, defaultPrevented: false })

let api: CullApi

/** A free version whose Plus status is known, on the analysis screen, nothing sorted yet. */
beforeEach(() => {
  vi.restoreAllMocks()
  setNotifier(() => {})
  api = createMockApi({ thumbnails: false, storage: null, latency: 0, params: new URLSearchParams('mockNoPopups=1') })
  setApi(api)
  useAppStore.setState({ settings: { ...MOCK_SETTINGS }, auth: { status: 'signedOut' }, screen: 'analysis', dialog: null })
  usePlusStore.setState({ status: NO_PLUS, loaded: true, answered: true, dialogOpen: false, view: 'offer', reason: null })
  useAnalysisStore.setState({ projectId: null, running: false, lastSummary: null, error: null, supportOffered: false, supportOpen: false })
  vi.spyOn(api, 'startAnalysis').mockResolvedValue()
  vi.spyOn(api, 'cancelAnalysis').mockResolvedValue()
})

/** A sort that runs to its end, as the engine reports it; back on its result screen. */
async function sort(summary: AnalysisSummary = SUMMARY) {
  useAppStore.setState({ screen: 'analysis' })
  await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
  useAnalysisStore.getState().onDone(summary)
}

const screen = () => useAppStore.getState().screen

describe('support dialog on the way to the review', () => {
  it('opens on the first way to the review of a free sort that ended well, not on the second', async () => {
    await sort()
    expect(supportDialogVisible()).toBe(false)
    goToReview()
    expect(supportDialogVisible()).toBe(true)
    // The review waits behind the dialog.
    expect(screen()).toBe('analysis')
    closeSupportDialog()
    expect(screen()).toBe('workspace')
    // Back to the review of the same sort: straight in.
    useAppStore.setState({ screen: 'analysis' })
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')
  })

  it('comes back after the next sort', async () => {
    await sort()
    goToReview()
    closeSupportDialog()
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(true)
  })

  it('goes straight to the review with Plus, from a licence or an account', async () => {
    usePlusStore.setState({ status: LICENCE })
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')
    usePlusStore.setState({ status: { ...NO_PLUS, active: true, source: 'account' } })
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')
  })

  it('goes straight to the review while the Plus status is unknown, or after a failed status request', async () => {
    usePlusStore.setState({ status: NO_PLUS, loaded: false, answered: false })
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')

    vi.spyOn(api, 'plusStatus').mockRejectedValue({ code: 'internal', message: 'Engine busy' })
    await usePlusStore.getState().load()
    // The app goes on (loaded), but nothing says the rights are free.
    expect(usePlusStore.getState()).toMatchObject({ loaded: true, answered: false })
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')
  })

  it('opens once a Plus status really arrived (an answer or a plus event)', async () => {
    usePlusStore.setState({ status: NO_PLUS, loaded: false, answered: false })
    vi.spyOn(api, 'plusStatus').mockResolvedValue(NO_PLUS)
    await usePlusStore.getState().load()
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(true)

    usePlusStore.setState({ status: NO_PLUS, loaded: true, answered: false })
    usePlusStore.getState().onStatus(NO_PLUS)
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(true)
  })

  it('closes when Plus activates while it is open, and opens the review once', async () => {
    const review = vi.spyOn(useAppStore.getState(), 'reviewResults')
    await sort()
    goToReview()
    expect(supportDialogVisible()).toBe(true)
    usePlusStore.getState().onStatus(LICENCE)
    expect(supportDialogVisible()).toBe(false)
    expect(useAnalysisStore.getState()).toMatchObject({ supportOpen: false, supportOffered: false })
    expect(screen()).toBe('workspace')
    expect(review).toHaveBeenCalledTimes(1)
    // Another Plus answer changes nothing more.
    usePlusStore.getState().onStatus(LICENCE)
    expect(review).toHaveBeenCalledTimes(1)
  })

  it('never comes back for the same sort when Plus is released', async () => {
    await sort()
    goToReview()
    usePlusStore.getState().onStatus(LICENCE)
    usePlusStore.getState().onStatus(NO_PLUS)
    expect(supportDialogVisible()).toBe(false)
    // Even on the result screen again, straight to the review.
    useAppStore.setState({ screen: 'analysis' })
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')
  })

  it('is not offered after a cancelled sort, a failed one, or one where every photo failed', async () => {
    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    await useAnalysisStore.getState().cancel()
    // A cancelled run also ends with a summary.
    useAnalysisStore.getState().onDone(SUMMARY)
    goToReview()
    expect(supportDialogVisible()).toBe(false)

    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    useAnalysisStore.getState().onError({ projectId: PROJECT, code: 'diskFull', message: 'No space left' })
    goToReview()
    expect(supportDialogVisible()).toBe(false)

    await sort({ ...SUMMARY, analyzed: 0, failed: 10, counts: { selected: 0, toReview: 10, rejected: 0 } })
    expect(useAnalysisStore.getState()).toMatchObject({ running: false, lastSummary: { analyzed: 0, failed: 10 } })
    goToReview()
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('workspace')

    // Some photos failed, others were sorted: offered.
    await sort({ ...SUMMARY, analyzed: 7, failed: 3 })
    goToReview()
    expect(supportDialogVisible()).toBe(true)
  })

  it('goes on to the review however it closes: ×, Escape, a click outside, "See the selection"', async () => {
    // The ×, Escape and a click outside all call the dialog's onClose; "Not now" too.
    for (let i = 0; i < 2; i++) {
      await sort()
      goToReview()
      expect(supportDialogVisible()).toBe(true)
      closeSupportDialog()
      expect(supportDialogVisible()).toBe(false)
      expect(screen()).toBe('workspace')
    }
  })

  it('opens Instagram or GitHub in the browser, and goes on to the review', async () => {
    const open = vi.spyOn(api, 'openExternal').mockResolvedValue()
    for (const url of [INSTAGRAM_URL, GITHUB_URL]) {
      await sort()
      goToReview()
      followFromSupportDialog(url)
      expect(open).toHaveBeenLastCalledWith(url)
      expect(supportDialogVisible()).toBe(false)
      expect(screen()).toBe('workspace')
    }
  })

  it('closes for the Plus dialog on its offer, the review behind it', async () => {
    await sort()
    goToReview()
    getPlusFromSupportDialog()
    expect(supportDialogVisible()).toBe(false)
    expect(usePlusStore.getState()).toMatchObject({ dialogOpen: true, view: 'offer', reason: 'support' })
    expect(screen()).toBe('workspace')
  })
})

describe('keys on the result screen', () => {
  it('Enter on the screen opens the dialog first, and never acts twice while it is open', async () => {
    await sort()
    expect(press('Enter')).toBe(true)
    expect(supportDialogVisible()).toBe(true)
    // Enter inside the dialog belongs to its focused button, not to the result screen.
    expect(press('Enter', A_BUTTON)).toBe(false)
    expect(press('Enter')).toBe(false)
    expect(screen()).toBe('analysis')
    // Escape closes it (onClose): on to the review.
    closeSupportDialog()
    expect(screen()).toBe('workspace')
  })

  it('leaves Enter to a focused control and to the other dialogs', async () => {
    await sort()
    expect(press('Enter', A_BUTTON)).toBe(false)
    usePlusStore.setState({ dialogOpen: true })
    expect(press('Enter')).toBe(false)
    usePlusStore.setState({ dialogOpen: false })
    useAppStore.setState({ dialog: 'settings' })
    expect(press('Enter')).toBe(false)
    expect(press('Escape')).toBe(false)
    expect(supportDialogVisible()).toBe(false)
    expect(screen()).toBe('analysis')
  })
})
