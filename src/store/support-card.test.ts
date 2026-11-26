import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { MOCK_SETTINGS } from '@/lib/api/mock/fixtures'
import type { AnalysisSummary, CullApi } from '@/lib/contract'
import { GITHUB_URL, INSTAGRAM_URL } from '@/lib/links'
import { setNotifier } from '@/lib/notify'
import { NO_PLUS } from '@/lib/plus'
import { useAnalysisStore } from './analysis-store'
import { useAppStore } from './app-store'
import { usePlusStore } from './plus-store'
import { closeSupportCard, followFromSupportCard, getPlusFromSupportCard, supportCardVisible } from './support-card'

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

let api: CullApi

/** A free version whose Plus status is known, nothing sorted yet. */
beforeEach(() => {
  vi.restoreAllMocks()
  setNotifier(() => {})
  api = createMockApi({ thumbnails: false, storage: null, latency: 0, params: new URLSearchParams('mockNoPopups=1') })
  setApi(api)
  useAppStore.setState({ settings: { ...MOCK_SETTINGS }, auth: { status: 'signedOut' } })
  usePlusStore.setState({ status: NO_PLUS, loaded: true, dialogOpen: false, view: 'offer', reason: null })
  useAnalysisStore.setState({ projectId: null, running: false, lastSummary: null, error: null, supportCard: false })
  vi.spyOn(api, 'startAnalysis').mockResolvedValue()
  vi.spyOn(api, 'cancelAnalysis').mockResolvedValue()
})

/** A sort that runs to its end, as the engine reports it. */
async function sort() {
  await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
  useAnalysisStore.getState().onDone(SUMMARY)
}

describe('support card after a sort', () => {
  it('shows after every completed sort, in the free version', async () => {
    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    // Never while the sort is still running.
    expect(supportCardVisible()).toBe(false)
    useAnalysisStore.getState().onDone(SUMMARY)
    expect(supportCardVisible()).toBe(true)
    await sort()
    expect(supportCardVisible()).toBe(true)
  })

  it('does not show after a cancelled sort, even when its summary arrives', async () => {
    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    await useAnalysisStore.getState().cancel()
    // A cancelled run also ends with a summary.
    useAnalysisStore.getState().onDone(SUMMARY)
    expect(supportCardVisible()).toBe(false)
  })

  it('does not show after a failed sort', async () => {
    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    useAnalysisStore.getState().onError({ projectId: PROJECT, code: 'diskFull', message: 'No space left' })
    expect(supportCardVisible()).toBe(false)
    vi.spyOn(api, 'startAnalysis').mockRejectedValue({ code: 'io', message: 'Folder gone' })
    await useAnalysisStore.getState().start(PROJECT, MOCK_SETTINGS.cull)
    expect(useAnalysisStore.getState().error).not.toBeNull()
    expect(supportCardVisible()).toBe(false)
  })

  it('never shows with Plus, nor before the Plus status is known', async () => {
    usePlusStore.setState({ status: LICENCE })
    await sort()
    expect(supportCardVisible()).toBe(false)
    // Plus from a Pelli account hides it the same way.
    usePlusStore.setState({ status: { ...NO_PLUS, active: true, source: 'account' } })
    expect(supportCardVisible()).toBe(false)
    usePlusStore.setState({ status: NO_PLUS, loaded: false })
    expect(supportCardVisible()).toBe(false)
  })

  it('disappears as soon as Plus activates while it is visible', async () => {
    await sort()
    expect(supportCardVisible()).toBe(true)
    usePlusStore.getState().onStatus(LICENCE)
    expect(supportCardVisible()).toBe(false)
  })

  it('closes for this sort only, and comes back after the next one', async () => {
    await sort()
    closeSupportCard()
    expect(supportCardVisible()).toBe(false)
    await sort()
    expect(supportCardVisible()).toBe(true)
  })

  it('closes when Instagram or GitHub opens, until the next sort', async () => {
    const open = vi.spyOn(api, 'openExternal').mockResolvedValue()
    await sort()
    followFromSupportCard(INSTAGRAM_URL)
    expect(open).toHaveBeenLastCalledWith(INSTAGRAM_URL)
    expect(supportCardVisible()).toBe(false)
    await sort()
    followFromSupportCard(GITHUB_URL)
    expect(open).toHaveBeenLastCalledWith(GITHUB_URL)
    expect(supportCardVisible()).toBe(false)
    await sort()
    expect(supportCardVisible()).toBe(true)
  })

  it('opens the Plus dialog on its offer from its discreet line', async () => {
    await sort()
    getPlusFromSupportCard()
    expect(usePlusStore.getState()).toMatchObject({ dialogOpen: true, view: 'offer', reason: 'support' })
    // The card stays behind the dialog until Plus is active.
    expect(supportCardVisible()).toBe(true)
  })
})
