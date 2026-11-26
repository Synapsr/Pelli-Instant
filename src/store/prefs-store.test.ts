import { afterEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_XMP_MAPPING, migratePrefs, usePrefsStore, xmpExportMapping } from './prefs-store'

const v1Mapping = { rating: { selected: 4, toReview: 0, rejected: -1 }, label: { selected: 'blue', toReview: 'none', rejected: 'red' } }

describe('preferences', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.resetModules()
    usePrefsStore.setState({ xmpMapping: DEFAULT_XMP_MAPPING })
  })

  it('brings the export mapping of earlier versions up to date', () => {
    const migrated = migratePrefs({ thumbSize: 240, xmpMapping: v1Mapping })
    expect(migrated.thumbSize).toBe(240)
    // Their stars and colours stay; the names follow the app language, as before.
    expect(migrated.xmpMapping).toEqual({ ...v1Mapping, labelLanguage: null })
    expect(migratePrefs({ xmpMapping: { ...v1Mapping, labelLanguage: 'de' } }).xmpMapping?.labelLanguage).toBe('de')
    expect(migratePrefs({ xmpMapping: { ...v1Mapping, labelLanguage: 'ja' } }).xmpMapping?.labelLanguage).toBeNull()
    expect(migratePrefs({ xmpMapping: { rating: { selected: 5 } } }).xmpMapping).toEqual({
      ...DEFAULT_XMP_MAPPING,
      rating: { ...DEFAULT_XMP_MAPPING.rating, selected: 5 },
    })
    expect(migratePrefs(null).xmpMapping).toEqual(DEFAULT_XMP_MAPPING)
  })

  it('loads the preferences an earlier version saved', async () => {
    const saved = new Map([['pelli-cull:prefs', JSON.stringify({ state: { thumbSize: 240, xmpMapping: v1Mapping }, version: 1 })]])
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => saved.set(key, value),
      removeItem: (key: string) => saved.delete(key),
    })
    vi.resetModules()
    const { usePrefsStore: fresh } = await import('./prefs-store')
    expect(fresh.getState().thumbSize).toBe(240)
    expect(fresh.getState().xmpMapping).toEqual({ ...v1Mapping, labelLanguage: null })
    fresh.getState().setLabelLanguage('nl')
    const stored = JSON.parse(saved.get('pelli-cull:prefs') ?? '{}')
    expect(stored.version).toBe(2)
    expect(stored.state.xmpMapping).toEqual({ ...v1Mapping, labelLanguage: 'nl' })
  })

  it('ignores the support card keys a test build saved', async () => {
    const legacy = { thumbSize: 240, xmpMapping: DEFAULT_XMP_MAPPING, exportsDone: 7, supportCardShown: true }
    const saved = new Map([['pelli-cull:prefs', JSON.stringify({ state: legacy, version: 2 })]])
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => saved.set(key, value),
      removeItem: (key: string) => saved.delete(key),
    })
    vi.resetModules()
    const { usePrefsStore: fresh } = await import('./prefs-store')
    expect(fresh.getState().thumbSize).toBe(240)
    fresh.getState().setSounds(false)
    const stored = JSON.parse(saved.get('pelli-cull:prefs') ?? '{}')
    expect(stored.state.sounds).toBe(false)
    expect(stored.state).not.toHaveProperty('exportsDone')
    expect(stored.state).not.toHaveProperty('supportCardShown')
  })

  it('sends the label language picked in the export, else the app one', () => {
    const prefs = usePrefsStore.getState()
    expect(xmpExportMapping(usePrefsStore.getState().xmpMapping, 'fr')).toEqual({ ...DEFAULT_XMP_MAPPING, labelLanguage: 'fr' })
    prefs.setLabelLanguage('en')
    // Editing a decision's colour keeps the language.
    prefs.setXmpMapping('toReview', { label: 'blue' })
    const mapping = usePrefsStore.getState().xmpMapping
    expect(mapping.labelLanguage).toBe('en')
    expect(xmpExportMapping(mapping, 'fr')).toMatchObject({ labelLanguage: 'en', label: { toReview: 'blue' } })
  })
})
