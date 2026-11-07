import { DEFAULT_SETTINGS, type Account, type AppInfo, type AppSettings, type Language, type Offers, type Plan } from '@/lib/contract'
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
const MONTHLY_QUOTA: Record<MockPlan, number | null> = { free: 200, lite: 1000, pro: null, ultra: null, trial: null }

/** First instant of next month (UTC), when the monthly count resets. */
export function nextMonthStart(at = Date.now()): number {
  const d = new Date(at)
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1)
}

export function mockAccount(plan: MockPlan, usedThisMonth: number, trialAvailable = false): Account {
  return {
    name: 'Camille Dubois',
    email: 'camille@studio-dubois.fr',
    slug: 'camille-dubois',
    avatarUrl: null,
    plan: plan === 'trial' ? 'ultra' : plan,
    planLabel: PLAN_LABELS[plan],
    trialEndsAt: plan === 'trial' ? Date.now() + 11 * DAY : null,
    trialAvailable: plan === 'free' && trialAvailable,
    lifetime: false,
    entitlements: {
      analyze: true,
      monthlyQuota: MONTHLY_QUOTA[plan],
      usedThisMonth,
      resetsAt: nextMonthStart(),
      sendToGallery: true,
    },
  }
}

// As Pelli's API returns them (2026 catalogue, euros); the app shows what Pelli sends.
const HIGHLIGHTS: Record<'fr' | 'en', Record<Plan, string[]>> = {
  fr: {
    free: ['Tout Pelli à l’essai pendant 15 jours', 'Sans carte bancaire'],
    lite: ['15 galeries', '10 Go de stockage', '1 heure de vidéo'],
    pro: ['Galeries illimitées', '100 Go de stockage', 'Lea, l’assistante IA pour vos devis et factures'],
    ultra: ['1 To de stockage', 'Boutique en ligne pour vendre vos photos', '15 heures de vidéo'],
  },
  en: {
    free: ['All of Pelli free for 15 days', 'No card needed'],
    lite: ['15 galleries', '10 GB of storage', '1 hour of video'],
    pro: ['Unlimited galleries', '100 GB of storage', 'Lea, the AI assistant for your quotes and invoices'],
    ultra: ['1 TB of storage', 'Online store to sell your photos', '15 hours of video'],
  },
}

/** Pelli's offers as the API returns them. */
export function mockOffers(lang: Language): Offers {
  const h = HIGHLIGHTS[lang === 'fr' ? 'fr' : 'en']
  const free = lang === 'fr' ? 'Gratuit' : 'Free'
  const app = 'https://app.pelli.io'
  const utm = 'utm_source=pelli-instant'
  return {
    currency: 'EUR',
    trialDays: 15,
    plans: [
      { slug: 'free', name: free, monthlyPrice: 0, yearlyPrice: 0, yearlyPricePerMonth: 0, photosPerMonth: 200, highlights: h.free },
      { slug: 'lite', name: 'Lite', monthlyPrice: 5, yearlyPrice: 60, yearlyPricePerMonth: 5, photosPerMonth: 1000, highlights: h.lite },
      { slug: 'pro', name: 'Pro', monthlyPrice: 25, yearlyPrice: 180, yearlyPricePerMonth: 15, photosPerMonth: null, highlights: h.pro },
      { slug: 'ultra', name: 'Ultra', monthlyPrice: 50, yearlyPrice: 420, yearlyPricePerMonth: 35, photosPerMonth: null, highlights: h.ultra },
    ],
    links: {
      register: `${app}/register?${utm}&utm_content=register`,
      pricing: `${app}/dashboard/billing?${utm}&utm_content=pricing#plans-section`,
      trial: `${app}/dashboard/billing?${utm}&utm_content=trial`,
      billing: `${app}/dashboard/billing?${utm}&utm_content=billing`,
    },
  }
}

export const MOCK_APP_INFO: AppInfo = {
  version: '0.1.0',
  platform: 'macos',
  arch: 'aarch64',
  accelerator: 'CPU',
  modelsReady: true,
  capabilities: { faces: true, eyes: true, embeddings: true, aesthetic: true, textSearch: true, maximum: false },
  cpuThreads: 10,
  memoryBytes: 16 * 1024 ** 3,
  cpuCores: 10,
  noAccount: false,
}

/** The newer version `?mockUpdate=available` publishes. */
export const MOCK_UPDATE_VERSION = '0.2.0'

export const MOCK_SETTINGS: AppSettings = {
  theme: 'dark',
  language: 'fr',
  acceleration: 'auto',
  background: false,
  precision: 'standard',
  cull: { ...DEFAULT_SETTINGS },
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
