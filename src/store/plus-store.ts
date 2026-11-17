import { create } from 'zustand'
import { api, toCullError } from '@/lib/api'
import type { PlusOffer, PlusStatus } from '@/lib/contract'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { NO_PLUS, normalizeKey, plusErrorCode, viewAfterStatus, type PlusErrorCode, type PlusView } from '@/lib/plus'
import { useAppStore } from './app-store'

/** What opened the Plus dialog: its success view offers the next step that makes sense. */
export type PlusReason = 'precision' | 'search' | 'stats' | 'settings' | 'pill' | 'onboarding'

export interface PlusState {
  status: PlusStatus
  /** the engine answered once */
  loaded: boolean
  /** Pelli's price for this visitor; null offline (the dialog falls back to the locale's) */
  offer: PlusOffer | null
  dialogOpen: boolean
  view: PlusView
  error: PlusErrorCode | null
  /** a request of the dialog is under way (checkout, activation, release, sign-in) */
  busy: boolean
  /** the Stripe page opened in the browser, to open it again */
  checkoutUrl: string | null
  reason: PlusReason | null

  load: () => Promise<void>
  /** engine event `plus`, and every answer that carries the status */
  onStatus: (status: PlusStatus) => void
  openDialog: (reason?: PlusReason, view?: PlusView) => void
  closeDialog: () => void
  setView: (view: PlusView) => void
  loadOffer: () => Promise<void>
  checkout: () => Promise<void>
  /** one check of the pending checkout (the waiting view calls it every 3 s, the window on focus) */
  poll: () => Promise<void>
  cancelCheckout: () => Promise<void>
  /** true once Plus is active */
  activate: (input: string) => Promise<boolean>
  /** true once the licence is released */
  release: () => Promise<boolean>
  /** Pelli account sign-in, the secondary way to Plus (Pro and Ultra include it) */
  signIn: () => Promise<void>
  pollSignIn: () => Promise<void>
  cancelSignIn: () => Promise<void>
}

function language() {
  return useAppStore.getState().settings?.language ?? 'en'
}

export const usePlusStore = create<PlusState>((set, get) => ({
  status: NO_PLUS,
  loaded: false,
  offer: null,
  dialogOpen: false,
  view: 'offer',
  error: null,
  busy: false,
  checkoutUrl: null,
  reason: null,

  load: async () => {
    try {
      const status = await api().plusStatus()
      set({ status, loaded: true })
    } catch {
      set({ loaded: true })
    }
  },

  onStatus: (status) => {
    const before = get().status
    set({ status, loaded: true })
    if (get().dialogOpen) {
      const next = viewAfterStatus(get().view, before, status)
      if (next.view !== get().view) set({ view: next.view, error: next.error ?? null, busy: false })
    } else if (status.active && !before.active) {
      // Paid while the dialog was closed (the window came back into focus).
      notify({ title: t('plus.success.title'), tone: 'success' })
    }
  },

  openDialog: (reason = 'pill', view) => {
    const { status } = get()
    set({
      dialogOpen: true,
      reason,
      error: null,
      busy: false,
      view: view ?? (status.active ? 'success' : status.pending ? 'waiting' : 'offer'),
    })
    void get().loadOffer()
  },

  closeDialog: () => set({ dialogOpen: false, busy: false }),

  setView: (view) => set({ view, error: null }),

  loadOffer: async () => {
    try {
      set({ offer: await api().plusOffer(language()) })
    } catch {
      /* offline: the dialog shows the locale's price */
    }
  },

  checkout: async () => {
    if (get().busy) return
    set({ busy: true, error: null })
    try {
      const { url } = await api().plusCheckout(language())
      set({ checkoutUrl: url, view: 'waiting', busy: false, status: { ...get().status, pending: true } })
    } catch (error) {
      set({ view: 'error', error: plusErrorCode(toCullError(error).code), busy: false })
    }
  },

  poll: async () => {
    try {
      get().onStatus(await api().plusPoll())
    } catch (error) {
      const code = toCullError(error).code
      // Offline for a moment: the next poll tries again.
      if (code === 'expired' && get().dialogOpen && get().view === 'waiting') set({ view: 'error', error: 'expired' })
    }
  },

  cancelCheckout: async () => {
    // The offer first, so the end of the wait doesn't read as an expired checkout.
    set({ view: 'offer', error: null })
    try {
      get().onStatus(await api().plusCancelCheckout())
    } catch {
      /* nothing to cancel */
    }
  },

  activate: async (input) => {
    const key = normalizeKey(input)
    if (!key) {
      set({ error: 'invalidKey' })
      return false
    }
    set({ busy: true, error: null })
    try {
      const status = await api().plusActivate(key)
      get().onStatus(status)
      set({ busy: false, view: status.active ? 'success' : get().view })
      return status.active
    } catch (error) {
      set({ busy: false, error: plusErrorCode(toCullError(error).code) })
      return false
    }
  },

  release: async () => {
    try {
      get().onStatus(await api().plusRelease())
      notify({ title: t('plus.settings.released'), tone: 'success' })
      return true
    } catch (error) {
      const e = toCullError(error)
      notify({ title: t('plus.settings.releaseFailed'), description: e.code === 'offline' ? t('plus.errors.offline') : e.message, tone: 'danger' })
      return false
    }
  },

  signIn: async () => {
    if (get().busy) return
    set({ busy: true, error: null })
    try {
      useAppStore.getState().setAuth(await api().authStart())
      set({ view: 'account', busy: false })
    } catch (error) {
      set({ view: 'error', error: plusErrorCode(toCullError(error).code), busy: false })
    }
  },

  pollSignIn: async () => {
    const auth = useAppStore.getState().auth
    if (auth.status !== 'pending') return
    if (Date.now() > auth.expiresAt) {
      useAppStore.getState().setAuth(await api().authLogout())
      set({ view: 'error', error: 'expired' })
      return
    }
    try {
      const next = await api().authPoll()
      useAppStore.getState().setAuth(next)
      if (next.status !== 'signedIn') return
      if (next.account.plus) get().onStatus(await api().plusStatus())
      else set({ view: 'error', error: 'notIncluded' })
    } catch {
      /* offline for a moment: the next poll tries again */
    }
  },

  cancelSignIn: async () => {
    set({ view: 'offer' })
    try {
      useAppStore.getState().setAuth(await api().authLogout())
    } catch {
      /* already signed out */
    }
  },
}))
