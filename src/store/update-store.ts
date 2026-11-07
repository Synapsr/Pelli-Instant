import { create } from 'zustand'
import { api, toCullError } from '@/lib/api'
import type { UpdateStatus } from '@/lib/contract'

/**
 * The app's own updates. The desktop shell checks on its own and reports
 * every change (`update-status`); the window offers what it finds and starts
 * the install.
 */
interface UpdateState {
  status: UpdateStatus
  load: () => Promise<void>
  /** shell event */
  onStatus: (status: UpdateStatus) => void
  /** "Check for updates" in the settings */
  check: () => Promise<void>
  /** download, wait for the work under way, restart: progress and failures arrive as events */
  install: () => Promise<void>
}

export const useUpdateStore = create<UpdateState>((set) => ({
  status: { status: 'idle' },

  load: async () => {
    try {
      set({ status: await api().updateStatus() })
    } catch {
      /* a host without updates: nothing to offer */
      set({ status: { status: 'disabled' } })
    }
  },

  onStatus: (status) => set({ status }),

  check: async () => {
    try {
      set({ status: await api().checkForUpdate() })
    } catch (error) {
      set({ status: { status: 'failed', code: 'internal', message: toCullError(error).message, version: null } })
    }
  },

  install: async () => {
    try {
      await api().installUpdate()
    } catch {
      /* reported by the status */
    }
  },
}))
