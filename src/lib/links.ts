/** Links the app opens in the browser. */

export const GITHUB_URL = 'https://github.com/Synapsr/Pelli-Instant'
export const PELLI_URL = 'https://pelli.io'
/** Pelli's Instagram, which the one-time support card invites to follow. */
export const INSTAGRAM_URL = 'https://www.instagram.com/pelli.io/'

/** Where the Pelli promotion of the free version leads (`home`, `export`). */
export function pelliPromoUrl(campaign: 'home' | 'export'): string {
  return `${PELLI_URL}/?utm_source=pelli-instant&utm_medium=app&utm_campaign=${campaign}`
}

/** A page of the Pelli app, tagged with where in Pelli Instant it was opened. */
function pelliAppUrl(path: string, campaign: 'plus' | 'gallery'): string {
  return `https://app.pelli.io${path}?utm_source=pelli-instant&utm_medium=app&utm_campaign=${campaign}`
}

/** Pelli's plans, for an account whose plan doesn't include Plus. */
export const PELLI_PRICING_URL = pelliAppUrl('/dashboard/billing', 'plus')

/** Pelli's plans, when sending to a gallery needs a subscription (the trial is over). */
export const PELLI_GALLERY_PLANS_URL = pelliAppUrl('/dashboard/billing', 'gallery')

/** A new Pelli account, for the gallery export: it starts with Pelli's free trial. */
export const PELLI_REGISTER_URL = pelliAppUrl('/register', 'gallery')

/** Length of the free trial a new Pelli account starts with (`TRIAL_DAYS` in Pelli). */
export const PELLI_TRIAL_DAYS = 15
