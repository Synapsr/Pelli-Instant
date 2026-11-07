import { create } from 'zustand'
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware'
import type { ColorLabel, FinalDecision } from '@/lib/contract'

/** Stars (0..5, -1 = rejected) and colour label the XMP sidecars get for each decision. */
export interface XmpMapping {
  rating: Record<FinalDecision, number>
  label: Record<FinalDecision, ColorLabel>
}

/** "To review" gets its colour only: two stars would collide with the photographer's own star system. */
export const DEFAULT_XMP_MAPPING: XmpMapping = {
  rating: { selected: 3, toReview: 0, rejected: -1 },
  label: { selected: 'green', toReview: 'yellow', rejected: 'red' },
}

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
  /** what the export writes in the XMP sidecars (the photographer's own marks always win) */
  xmpMapping: XmpMapping
  setThumbSize: (size: number) => void
  toggleSidebar: () => void
  toggleInspector: () => void
  setAutoAdvance: (value: boolean) => void
  setIncludeSubfolders: (value: boolean) => void
  setSounds: (value: boolean) => void
  setOnboarded: (value: boolean) => void
  setVoice: (value: boolean) => void
  setXmpMapping: (decision: FinalDecision, patch: { rating?: number; label?: ColorLabel }) => void
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
      xmpMapping: DEFAULT_XMP_MAPPING,
      setThumbSize: (size) => set({ thumbSize: Math.round(Math.min(THUMB_MAX, Math.max(THUMB_MIN, size))) }),
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
      setAutoAdvance: (autoAdvance) => set({ autoAdvance }),
      setIncludeSubfolders: (includeSubfolders) => set({ includeSubfolders }),
      setSounds: (sounds) => set({ sounds }),
      setOnboarded: (onboarded) => set({ onboarded }),
      setVoice: (voice) => set({ voice }),
      setXmpMapping: (decision, patch) =>
        set(({ xmpMapping }) => ({
          xmpMapping: {
            rating: patch.rating === undefined ? xmpMapping.rating : { ...xmpMapping.rating, [decision]: patch.rating },
            label: patch.label === undefined ? xmpMapping.label : { ...xmpMapping.label, [decision]: patch.label },
          },
        })),
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
        xmpMapping: s.xmpMapping,
      }),
    },
  ),
)
