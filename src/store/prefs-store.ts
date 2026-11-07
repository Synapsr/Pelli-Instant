import { create } from 'zustand'
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware'

/** Per-user UI conveniences, kept in localStorage (never required to work). */
export interface PrefsState {
  /** target thumbnail width in px */
  thumbSize: number
  sidebarOpen: boolean
  inspectorOpen: boolean
  autoAdvance: boolean
  includeSubfolders: boolean
  /** small chimes when an analysis or an export ends */
  sounds: boolean
  /** the welcome tour was seen (shown once, after the first sign-in) */
  onboarded: boolean
  /** the welcome tour's voice-over */
  voice: boolean
  setThumbSize: (size: number) => void
  toggleSidebar: () => void
  toggleInspector: () => void
  setAutoAdvance: (value: boolean) => void
  setIncludeSubfolders: (value: boolean) => void
  setSounds: (value: boolean) => void
  setOnboarded: (value: boolean) => void
  setVoice: (value: boolean) => void
}

export const THUMB_MIN = 120
export const THUMB_MAX = 360

const memory = new Map<string, string>()
const safeStorage: StateStorage = {
  getItem: (name) => {
    try {
      return localStorage.getItem(name)
    } catch {
      return memory.get(name) ?? null
    }
  },
  setItem: (name, value) => {
    try {
      localStorage.setItem(name, value)
    } catch {
      memory.set(name, value)
    }
  },
  removeItem: (name) => {
    try {
      localStorage.removeItem(name)
    } catch {
      memory.delete(name)
    }
  },
}

export const usePrefsStore = create<PrefsState>()(
  persist(
    (set) => ({
      thumbSize: 208,
      sidebarOpen: true,
      inspectorOpen: true,
      autoAdvance: true,
      includeSubfolders: true,
      sounds: true,
      onboarded: false,
      voice: true,
      setThumbSize: (size) => set({ thumbSize: Math.round(Math.min(THUMB_MAX, Math.max(THUMB_MIN, size))) }),
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
      setAutoAdvance: (autoAdvance) => set({ autoAdvance }),
      setIncludeSubfolders: (includeSubfolders) => set({ includeSubfolders }),
      setSounds: (sounds) => set({ sounds }),
      setOnboarded: (onboarded) => set({ onboarded }),
      setVoice: (voice) => set({ voice }),
    }),
    {
      name: 'pelli-cull:prefs',
      version: 1,
      storage: createJSONStorage(() => safeStorage),
      partialize: (s) => ({
        thumbSize: s.thumbSize,
        sidebarOpen: s.sidebarOpen,
        inspectorOpen: s.inspectorOpen,
        autoAdvance: s.autoAdvance,
        includeSubfolders: s.includeSubfolders,
        sounds: s.sounds,
        onboarded: s.onboarded,
        voice: s.voice,
      }),
    },
  ),
)
