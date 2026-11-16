import { create } from 'zustand'
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware'
import type { ColorLabel, FinalDecision, Language, XmpExportOptions } from '@/lib/contract'
import { isLanguage } from '@/lib/i18n'
import { resolveLabelLanguage } from '@/lib/label-names'

/** Stars (0..5, -1 = rejected) and colour label the XMP sidecars get for each decision. */
export interface XmpMapping {
  rating: Record<FinalDecision, number>
  label: Record<FinalDecision, ColorLabel>
  /** language of the colour names written in the sidecars, Lightroom's; null: the app language */
  labelLanguage: Language | null
}

/** "To review" gets its colour only: two stars would collide with the photographer's own star system. */
export const DEFAULT_XMP_MAPPING: XmpMapping = {
  rating: { selected: 3, toReview: 0, rejected: -1 },
  label: { selected: 'green', toReview: 'yellow', rejected: 'red' },
  labelLanguage: null,
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
  setVoice: (value: boolean) => void
  setXmpMapping: (decision: FinalDecision, patch: { rating?: number; label?: ColorLabel }) => void
  setLabelLanguage: (language: Language | null) => void
}

export const THUMB_MIN = 120
export const THUMB_MAX = 360

/** What the export sends for the XMP sidecars: the mapping, with the label names' language resolved. */
export function xmpExportMapping(mapping: XmpMapping, appLanguage: Language): Pick<XmpExportOptions, 'rating' | 'label' | 'labelLanguage'> {
  return { rating: mapping.rating, label: mapping.label, labelLanguage: resolveLabelLanguage(mapping.labelLanguage, appLanguage) }
}

/**
 * Preferences saved by an earlier version, brought up to date. Version 1
 * had no label language: the sidecars follow the app language, as before.
 */
export function migratePrefs(persisted: unknown): Partial<PrefsState> {
  const state = (persisted && typeof persisted === 'object' ? persisted : {}) as Partial<PrefsState>
  const mapping = state.xmpMapping as Partial<XmpMapping> | undefined
  if (!mapping || typeof mapping !== 'object') return { ...state, xmpMapping: DEFAULT_XMP_MAPPING }
  return {
    ...state,
    xmpMapping: {
      rating: { ...DEFAULT_XMP_MAPPING.rating, ...mapping.rating },
      label: { ...DEFAULT_XMP_MAPPING.label, ...mapping.label },
      labelLanguage: isLanguage(mapping.labelLanguage) ? mapping.labelLanguage : null,
    },
  }
}

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
      voice: true,
      xmpMapping: DEFAULT_XMP_MAPPING,
      setThumbSize: (size) => set({ thumbSize: Math.round(Math.min(THUMB_MAX, Math.max(THUMB_MIN, size))) }),
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
      setAutoAdvance: (autoAdvance) => set({ autoAdvance }),
      setIncludeSubfolders: (includeSubfolders) => set({ includeSubfolders }),
      setSounds: (sounds) => set({ sounds }),
      setVoice: (voice) => set({ voice }),
      setXmpMapping: (decision, patch) =>
        set(({ xmpMapping }) => ({
          xmpMapping: {
            rating: patch.rating === undefined ? xmpMapping.rating : { ...xmpMapping.rating, [decision]: patch.rating },
            label: patch.label === undefined ? xmpMapping.label : { ...xmpMapping.label, [decision]: patch.label },
            labelLanguage: xmpMapping.labelLanguage,
          },
        })),
      setLabelLanguage: (labelLanguage) => set(({ xmpMapping }) => ({ xmpMapping: { ...xmpMapping, labelLanguage } })),
    }),
    {
      name: 'pelli-cull:prefs',
      version: 2,
      storage: createJSONStorage(() => safeStorage),
      migrate: (persisted) => migratePrefs(persisted) as PrefsState,
      // A partial or older saved mapping never leaves a field undefined.
      merge: (persisted, current) => ({ ...current, ...migratePrefs(persisted) }),
      partialize: (s) => ({
        thumbSize: s.thumbSize,
        sidebarOpen: s.sidebarOpen,
        inspectorOpen: s.inspectorOpen,
        autoAdvance: s.autoAdvance,
        includeSubfolders: s.includeSubfolders,
        sounds: s.sounds,
        voice: s.voice,
        xmpMapping: s.xmpMapping,
      }),
    },
  ),
)
