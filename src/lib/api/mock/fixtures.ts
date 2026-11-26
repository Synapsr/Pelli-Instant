import { DEFAULT_SETTINGS, type Account, type AppInfo, type AppSettings, type Language, type Plan } from '@/lib/contract'
import type { SceneKind } from '@/lib/procedural/types'
import { WEDDING_SCENES } from './shoot'

const DAY = 86_400_000

export const SCENE_LABELS: Partial<Record<Language, Record<SceneKind, string>>> & { en: Record<SceneKind, string> } = {
  fr: {
    prep: 'Préparatifs',
    ceremony: 'Cérémonie',
    groups: 'Photos de groupe',
    couple: 'Séance couple',
    cocktail: "Vin d'honneur",
    dinner: 'Dîner',
    party: 'Soirée dansante',
  },
  en: {
    prep: 'Getting ready',
    ceremony: 'Ceremony',
    groups: 'Group photos',
    couple: 'Couple session',
    cocktail: 'Cocktail hour',
    dinner: 'Dinner',
    party: 'Dance party',
  },
  es: {
    prep: 'Preparativos',
    ceremony: 'Ceremonia',
    groups: 'Fotos de grupo',
    couple: 'Sesión de pareja',
    cocktail: 'Cóctel',
    dinner: 'Cena',
    party: 'Fiesta',
  },
  de: {
    prep: 'Vorbereitungen',
    ceremony: 'Trauung',
    groups: 'Gruppenfotos',
    couple: 'Paarshooting',
    cocktail: 'Sektempfang',
    dinner: 'Abendessen',
    party: 'Party',
  },
  it: {
    prep: 'Preparativi',
    ceremony: 'Cerimonia',
    groups: 'Foto di gruppo',
    couple: 'Servizio di coppia',
    cocktail: 'Aperitivo',
    dinner: 'Cena',
    party: 'Festa',
  },
}

/** Plans the mock can sign in with (`?mockPlan=`): the four Pelli plans and a running trial. */
export type MockPlan = Plan | 'trial'
export const MOCK_PLANS: MockPlan[] = ['free', 'lite', 'pro', 'ultra', 'trial']

const PLAN_LABELS: Record<MockPlan, string> = { free: 'Gratuit', lite: 'Lite', pro: 'Pro', ultra: 'Ultra', trial: 'Essai Ultra' }

/**
 * A signed-in Pelli account; Pro, Ultra and their trials include Plus. A free
 * account has no subscription and no running trial: Pelli's paywall, so no
 * gallery (`sendToGallery: false`), as Pelli answers.
 */
export function mockAccount(plan: MockPlan): Account {
  return {
    name: 'Camille Dubois',
    email: 'camille@studio-dubois.fr',
    slug: 'camille-dubois',
    avatarUrl: null,
    plan: plan === 'trial' ? 'ultra' : plan,
    planLabel: PLAN_LABELS[plan],
    trialEndsAt: plan === 'trial' ? Date.now() + 11 * DAY : null,
    lifetime: false,
    plus: plan === 'pro' || plan === 'ultra' || plan === 'trial',
    entitlements: { analyze: true, monthlyQuota: null, usedThisMonth: 0, resetsAt: 0, sendToGallery: plan !== 'free' },
  }
}

/** Licence key the mock hands out after a purchase (`?mockPlus=active`). */
export const MOCK_LICENSE_KEY = 'K7QM-4ZTP-9XRA-7K2Q'

/**
 * Price of Plus as Pelli's `/plus/offer` answers it: euros in Europe,
 * dollars elsewhere, formatted in the requested language.
 */
export function mockPlusOffer(lang: Language, currency: 'eur' | 'usd' = 'eur'): { amount: number; currency: 'eur' | 'usd'; label: string } {
  const label = new Intl.NumberFormat(lang, { style: 'currency', currency, maximumFractionDigits: 0, currencyDisplay: 'narrowSymbol' })
    .format(25)
    .replace(/\u202f/g, '\u00a0')
  return { amount: 2500, currency, label }
}

/** The events anonymous telemetry knows (contract §telemetry). */
export const TELEMETRY_EVENTS = ['onboarding_completed', 'app_opened', 'analysis_completed', 'export_completed', 'plus_activated', 'telemetry_disabled']

export const MOCK_APP_INFO: AppInfo = {
  version: '1.1.1-rc.3',
  platform: 'macos',
  arch: 'aarch64',
  accelerator: 'CPU',
  modelsReady: true,
  capabilities: { faces: true, eyes: true, embeddings: true, aesthetic: true, textSearch: true, maximum: false },
  cpuThreads: 10,
  memoryBytes: 16 * 1024 ** 3,
  cpuCores: 10,
}

/** The newer version `?mockUpdate=available` publishes. */
export const MOCK_UPDATE_VERSION = '1.1.0'

/** Its release notes, written like a CHANGELOG.md section (English, then French). */
export const MOCK_UPDATE_NOTES = `- **Send to a Pelli gallery** straight from the export.
- Faster grid on shoots of 5,000 photos.
- Fixes for Fujifilm RAF previews.

<details><summary>Français</summary>

- **Envoi vers une galerie Pelli**, directement depuis l’export.
- Grille plus rapide sur les séances de 5 000 photos.
- Corrections des aperçus RAF de Fujifilm.

</details>`

export const MOCK_SETTINGS: AppSettings = {
  theme: 'dark',
  language: 'fr',
  acceleration: 'auto',
  background: false,
  precision: 'standard',
  cull: { ...DEFAULT_SETTINGS },
  telemetry: true,
  onboarded: true,
  cacheDir: '/Users/camille/Library/Caches/io.pelli.instant',
  cacheSizeBytes: 1_934_000_000,
}

export interface MockProjectDef {
  id: string
  name: string
  rootPath: string
  count: number
  seed: number
  scenes: SceneKind[]
  twins: boolean
  /** day of the shoot (days before now) */
  shotDaysAgo: number
  openedDaysAgo: number
  analyzed: { daysAgo: number } | null
}

const ROOT = '/Users/camille/Photos/2026'

export const MOCK_PROJECTS: MockProjectDef[] = [
  {
    id: 'prj-lea-thomas',
    name: 'Mariage Léa & Thomas',
    rootPath: `${ROOT}/2026-10-24 Mariage Léa & Thomas`,
    count: 1284,
    seed: 7,
    scenes: WEDDING_SCENES,
    twins: true,
    shotDaysAgo: 10,
    openedDaysAgo: 0.08,
    analyzed: { daysAgo: 1.2 },
  },
  {
    id: 'prj-ines-karim',
    name: 'Mariage Inès & Karim',
    rootPath: `${ROOT}/2026-09-12 Mariage Inès & Karim – Domaine de Kerlann`,
    count: 2136,
    seed: 21,
    scenes: WEDDING_SCENES,
    twins: false,
    shotDaysAgo: 52,
    openedDaysAgo: 6,
    analyzed: { daysAgo: 20 },
  },
  {
    id: 'prj-manon-hugo',
    name: 'Séance couple Manon & Hugo',
    rootPath: `${ROOT}/2026-10-05 Séance couple Manon & Hugo`,
    count: 214,
    seed: 33,
    scenes: ['couple', 'cocktail'],
    twins: false,
    shotDaysAgo: 29,
    openedDaysAgo: 12,
    analyzed: { daysAgo: 27 },
  },
  {
    id: 'prj-bapteme-jules',
    name: 'Baptême de Jules',
    rootPath: `${ROOT}/2026-10-19 Baptême de Jules`,
    count: 356,
    seed: 48,
    scenes: ['ceremony', 'groups', 'cocktail', 'dinner'],
    twins: true,
    shotDaysAgo: 15,
    openedDaysAgo: 14,
    analyzed: null,
  },
  {
    id: 'prj-emma-grossesse',
    name: 'Séance grossesse Emma',
    rootPath: `${ROOT}/2026-09-28 Séance grossesse Emma`,
    count: 168,
    seed: 57,
    scenes: ['couple'],
    twins: false,
    shotDaysAgo: 49,
    openedDaysAgo: 19,
    analyzed: { daysAgo: 45 },
  },
  {
    id: 'prj-sarah-paul',
    name: 'Mariage Sarah & Paul',
    rootPath: `${ROOT}/2026-09-05 Mariage Sarah & Paul`,
    count: 1642,
    seed: 55,
    scenes: WEDDING_SCENES,
    twins: true,
    shotDaysAgo: 72,
    openedDaysAgo: 24,
    analyzed: { daysAgo: 63 },
  },
  {
    id: 'prj-le-gall',
    name: 'Famille Le Gall',
    rootPath: `${ROOT}/2026-08-30 Famille Le Gall`,
    count: 236,
    seed: 64,
    scenes: ['groups', 'couple'],
    twins: false,
    shotDaysAgo: 78,
    openedDaysAgo: 31,
    analyzed: { daysAgo: 70 },
  },
  {
    id: 'prj-clara-evjf',
    name: 'EVJF Clara',
    rootPath: `${ROOT}/2026-08-16 EVJF Clara`,
    count: 412,
    seed: 72,
    scenes: ['cocktail', 'party'],
    twins: true,
    shotDaysAgo: 92,
    openedDaysAgo: 38,
    analyzed: { daysAgo: 88 },
  },
  {
    id: 'prj-camille-antoine',
    name: 'Mariage Camille & Antoine',
    rootPath: `${ROOT}/2026-07-25 Mariage Camille & Antoine – Manoir de Kerbiquet`,
    count: 1928,
    seed: 81,
    scenes: WEDDING_SCENES,
    twins: true,
    shotDaysAgo: 114,
    openedDaysAgo: 52,
    analyzed: { daysAgo: 108 },
  },
  {
    id: 'prj-lou-naissance',
    name: 'Naissance de Lou',
    rootPath: `${ROOT}/2026-07-11 Naissance de Lou`,
    count: 142,
    seed: 86,
    scenes: ['prep', 'couple'],
    twins: false,
    shotDaysAgo: 128,
    openedDaysAgo: 66,
    analyzed: { daysAgo: 125 },
  },
  {
    id: 'prj-helix-corporate',
    name: 'Portraits corporate Helix',
    rootPath: `${ROOT}/2026-06-24 Portraits corporate Helix`,
    count: 318,
    seed: 93,
    scenes: ['groups'],
    twins: true,
    shotDaysAgo: 145,
    openedDaysAgo: 80,
    analyzed: { daysAgo: 140 },
  },
  {
    id: 'prj-julie-mathis',
    name: 'Mariage Julie & Mathis',
    rootPath: `${ROOT}/2026-06-13 Mariage Julie & Mathis`,
    count: 1374,
    seed: 104,
    scenes: WEDDING_SCENES,
    twins: true,
    shotDaysAgo: 156,
    openedDaysAgo: 95,
    analyzed: { daysAgo: 150 },
  },
  {
    id: 'prj-alba-bapteme',
    name: 'Baptême d’Alba',
    rootPath: `${ROOT}/2026-05-31 Baptême d’Alba`,
    count: 274,
    seed: 117,
    scenes: ['ceremony', 'groups', 'dinner'],
    twins: false,
    shotDaysAgo: 169,
    openedDaysAgo: 120,
    analyzed: { daysAgo: 165 },
  },
]

export const NEW_FOLDER = {
  name: '2026-11-01 Mariage Chloé & Maxime',
  rootPath: `${ROOT}/2026-11-01 Mariage Chloé & Maxime`,
  count: 1186,
  seed: 91,
}

export interface MockGalleryDef {
  id: string
  title: string
  slug: string
  status: string
  imageCount: number
  daysAgo: number
  seed: number
}

export const MOCK_GALLERIES: MockGalleryDef[] = [
  { id: 'gal-1', title: 'Mariage Inès & Karim', slug: 'mariage-ines-karim', status: 'published', imageCount: 642, daysAgo: 18, seed: 21 },
  { id: 'gal-2', title: 'Séance couple Manon & Hugo', slug: 'seance-manon-hugo', status: 'published', imageCount: 86, daysAgo: 25, seed: 33 },
  { id: 'gal-3', title: 'Mariage Sarah & Paul', slug: 'mariage-sarah-paul', status: 'published', imageCount: 518, daysAgo: 61, seed: 55 },
  { id: 'gal-4', title: 'Famille Le Gall', slug: 'famille-le-gall', status: 'draft', imageCount: 0, daysAgo: 2, seed: 64 },
  { id: 'gal-5', title: 'EVJF Clara', slug: 'evjf-clara', status: 'published', imageCount: 134, daysAgo: 90, seed: 72 },
]

export function daysAgo(days: number, now = Date.now()): number {
  return Math.round(now - days * DAY)
}

/**
 * Midnight `days` ago, in UTC: EXIF capture times carry no time zone and the
 * engine reads them as UTC, so the UI formats them in UTC too.
 */
export function shootDay(days: number, now = Date.now()): number {
  const d = new Date(now - days * DAY)
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())
}
