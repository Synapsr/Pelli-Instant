/** Links the app opens in the browser. */

export const GITHUB_URL = 'https://github.com/Synapsr/Pelli-Instant'
export const PELLI_URL = 'https://pelli.io'

/** Where the Pelli promotion of the free version leads (`home`, `export`). */
export function pelliPromoUrl(campaign: 'home' | 'export'): string {
  return `${PELLI_URL}/?utm_source=pelli-instant&utm_medium=app&utm_campaign=${campaign}`
}

/** Pelli's plans, for an account whose plan doesn't include Plus. */
export const PELLI_PRICING_URL = 'https://app.pelli.io/dashboard/billing?utm_source=pelli-instant&utm_medium=app&utm_campaign=plus'
