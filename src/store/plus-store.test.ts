import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { MOCK_SETTINGS } from '@/lib/api/mock/fixtures'
import type { CullApi } from '@/lib/contract'
import { NO_PLUS } from '@/lib/plus'
import { useAppStore } from './app-store'
import { usePackStore } from './pack-store'
import { usePlusStore } from './plus-store'
import { useReviewStore } from './review-store'

/** The mock engine, its `plus` events routed to the store as the app does. */
function setup(params: string): CullApi {
  const api = createMockApi({ thumbnails: false, storage: null, latency: 0, params: new URLSearchParams(`mockNoPopups=1&${params}`) })
  setApi(api)
  api.on('plus', (status) => usePlusStore.getState().onStatus(status))
  usePlusStore.setState({ status: NO_PLUS, loaded: false, offer: null, dialogOpen: false, view: 'offer', error: null, busy: false, checkoutUrl: null, reason: null })
  useAppStore.setState({ settings: { ...MOCK_SETTINGS }, auth: { status: 'signedOut' } })
  return api
}

describe('Plus store', () => {
  beforeEach(() => vi.restoreAllMocks())

  it('opens on the offer with Pelli’s price, or on the wait of a pending checkout', async () => {
    setup('mockPlus=none')
    await usePlusStore.getState().load()
    usePlusStore.getState().openDialog('pill')
    expect(usePlusStore.getState()).toMatchObject({ dialogOpen: true, view: 'offer', reason: 'pill' })
    await usePlusStore.getState().loadOffer()
    expect(usePlusStore.getState().offer).toMatchObject({ amount: 2500, currency: 'eur' })

    setup('mockPlus=pending')
    await usePlusStore.getState().load()
    usePlusStore.getState().openDialog('settings')
    expect(usePlusStore.getState().view).toBe('waiting')
  })

  it('buys Plus: checkout, wait, then success when the payment arrives', async () => {
    setup('mockPlus=none&mockPolls=2')
    await usePlusStore.getState().load()
    usePlusStore.getState().openDialog('precision')
    await usePlusStore.getState().checkout()
    expect(usePlusStore.getState()).toMatchObject({ view: 'waiting', checkoutUrl: expect.stringMatching(/^https:/) })
    expect(usePlusStore.getState().status.pending).toBe(true)
    await usePlusStore.getState().poll()
    expect(usePlusStore.getState().view).toBe('waiting')
    await usePlusStore.getState().poll()
    expect(usePlusStore.getState().view).toBe('success')
    expect(usePlusStore.getState().status).toMatchObject({ active: true, source: 'license', pending: false })
  })

  it('says the checkout expired when it stops waiting without Plus', async () => {
    setup('mockPlus=pending&mockCheckout=expired&mockPolls=1')
    await usePlusStore.getState().load()
    usePlusStore.getState().openDialog('pill')
    await usePlusStore.getState().poll()
    expect(usePlusStore.getState()).toMatchObject({ view: 'error', error: 'expired' })
  })

  it('cancelling the wait goes back to the offer, not to an error', async () => {
    setup('mockPlus=pending&mockCheckout=never')
    await usePlusStore.getState().load()
    usePlusStore.getState().openDialog('pill')
    await usePlusStore.getState().cancelCheckout()
    expect(usePlusStore.getState()).toMatchObject({ view: 'offer', error: null })
    expect(usePlusStore.getState().status.pending).toBe(false)
  })

  it('activates a key, refuses a malformed one without asking the engine, explains refusals', async () => {
    const api = setup('mockPlus=none')
    const spy = vi.spyOn(api, 'plusActivate')
    usePlusStore.getState().openDialog('pill', 'key')
    expect(await usePlusStore.getState().activate('K7QM-4ZTP')).toBe(false)
    expect(usePlusStore.getState().error).toBe('invalidKey')
    expect(spy).not.toHaveBeenCalled()
    expect(await usePlusStore.getState().activate('1AAA-BBBB-CCCC-DDDD')).toBe(false)
    expect(usePlusStore.getState().error).toBe('otherDevice')
    expect(await usePlusStore.getState().activate('2AAA-BBBB-CCCC-DDDD')).toBe(false)
    expect(usePlusStore.getState().error).toBe('revoked')
    expect(await usePlusStore.getState().activate('k7qm 4ztp 9xra 7k2q')).toBe(true)
    expect(usePlusStore.getState()).toMatchObject({ view: 'success', error: null })
    expect(spy).toHaveBeenLastCalledWith('K7QM-4ZTP-9XRA-7K2Q')
  })

  it('releases the licence, and keeps it when offline', async () => {
    setup('mockPlus=active&mockOffline=1')
    await usePlusStore.getState().load()
    expect(await usePlusStore.getState().release()).toBe(false)
    expect(usePlusStore.getState().status.active).toBe(true)

    setup('mockPlus=active')
    await usePlusStore.getState().load()
    expect(await usePlusStore.getState().release()).toBe(true)
    expect(usePlusStore.getState().status).toEqual(NO_PLUS)
  })

  it('a Pelli account without Plus says so; Pro brings it', async () => {
    setup('mockPlus=none&mockPlan=free')
    usePlusStore.getState().openDialog('pill')
    await usePlusStore.getState().signIn()
    expect(usePlusStore.getState().view).toBe('account')
    await usePlusStore.getState().pollSignIn()
    await usePlusStore.getState().pollSignIn()
    expect(usePlusStore.getState()).toMatchObject({ view: 'error', error: 'notIncluded' })

    setup('mockPlus=none&mockPlan=pro&mockAuth=signedOut')
    usePlusStore.getState().openDialog('pill')
    await usePlusStore.getState().signIn()
    await usePlusStore.getState().pollSignIn()
    await usePlusStore.getState().pollSignIn()
    expect(usePlusStore.getState().view).toBe('success')
    expect(usePlusStore.getState().status).toMatchObject({ active: true, source: 'account' })
  })

  it('Maximum and AI search open the Plus dialog for the free version', async () => {
    setup('mockPlus=none')
    await usePackStore.getState().load()
    await usePackStore.getState().choose('maximum')
    expect(usePlusStore.getState()).toMatchObject({ dialogOpen: true, reason: 'precision' })
    expect(useAppStore.getState().settings?.precision).toBe('standard')

    usePlusStore.getState().closeDialog()
    const api = setup('mockPlus=none')
    useReviewStore.getState().reset()
    useReviewStore.getState().load(await api.getProject('prj-lea-thomas'))
    await useReviewStore.getState().runSearch('mariée')
    expect(usePlusStore.getState()).toMatchObject({ dialogOpen: true, reason: 'search' })
    expect(useReviewStore.getState().search.status).toBe('idle')
  })
})

describe('telemetry and the welcome', () => {
  it('finishing the welcome saves onboarded and the statistics choice in one change', async () => {
    const api = setup('mockOnboarded=0')
    useAppStore.setState({ settings: await api.getSettings() })
    const spy = vi.spyOn(api, 'setSettings')
    useAppStore.getState().finishOnboarding(false)
    await vi.waitFor(() => expect(useAppStore.getState().settings).toMatchObject({ onboarded: true, telemetry: false }))
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith({ onboarded: true, telemetry: false })
    expect(useAppStore.getState().screen).toBe('home')
  })

  it('a replay of the welcome changes nothing when the choice stays the same', async () => {
    const api = setup('')
    useAppStore.setState({ settings: await api.getSettings() })
    const spy = vi.spyOn(api, 'setSettings')
    useAppStore.getState().finishOnboarding(true)
    expect(spy).not.toHaveBeenCalled()
  })
})
