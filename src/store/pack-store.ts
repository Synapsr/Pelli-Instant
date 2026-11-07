import { create } from 'zustand'
import { api, toCullError } from '@/lib/api'
import type { ModelPack, Precision } from '@/lib/contract'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { useAppStore } from './app-store'

/**
 * The optional Maximum pack: its download, and the precision the
 * photographer picked. Choosing 'maximum' starts (or resumes) the download;
 * going back to 'standard' pauses a download in progress.
 */
interface PackState {
  pack: ModelPack | null
  load: () => Promise<void>
  /** engine event */
  onPack: (pack: ModelPack) => void
  choose: (precision: Precision) => Promise<void>
  download: () => Promise<void>
  pause: () => Promise<void>
  remove: () => Promise<void>
}

const busy = (pack: ModelPack | null) => pack?.state.status === 'downloading' || pack?.state.status === 'preparing'

export const usePackStore = create<PackState>((set, get) => ({
  pack: null,

  load: async () => {
    try {
      set({ pack: await api().modelPack() })
    } catch {
      /* an engine without the pack: the setting stays hidden */
    }
  },

  onPack: (pack) => {
    const before = get().pack?.state.status
    set({ pack })
    if (pack.state.status === 'ready' && before !== 'ready') {
      // The capabilities say whether the Maximum models load.
      void api()
        .appInfo()
        .then((appInfo) => useAppStore.setState({ appInfo }))
      if (before === 'downloading' || before === 'preparing') notify({ title: t('settings.analysis.precision.readyToast'), tone: 'success' })
    }
  },

  choose: async (precision) => {
    await useAppStore.getState().updateSettings({ precision })
    const { pack } = get()
    if (!pack) return
    if (precision === 'maximum' && (pack.state.status === 'absent' || pack.state.status === 'failed')) await get().download()
    if (precision === 'standard' && busy(pack)) await get().pause()
  },

  download: async () => {
    try {
      await api().downloadModelPack()
    } catch (error) {
      const e = toCullError(error)
      const pack = get().pack
      if (pack) {
        const received = pack.state.status === 'failed' || pack.state.status === 'downloading' ? pack.state.receivedBytes : 0
        set({ pack: { ...pack, state: { status: 'failed', code: e.code, message: e.message, receivedBytes: received, totalBytes: pack.sizeBytes } } })
      }
    }
  },

  pause: async () => {
    try {
      set({ pack: await api().cancelModelPack() })
    } catch (error) {
      notify({ title: t('errors.generic'), description: toCullError(error).message, tone: 'danger' })
    }
  },

  remove: async () => {
    try {
      set({ pack: await api().removeModelPack() })
      useAppStore.setState({ appInfo: await api().appInfo() })
    } catch (error) {
      notify({ title: t('errors.generic'), description: toCullError(error).message, tone: 'danger' })
    }
  },
}))
