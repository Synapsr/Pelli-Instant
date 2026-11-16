import type { Language, PlusOffer, PlusStatus } from '@/lib/contract'
import { localeOf } from '@/lib/i18n'

/**
 * Pelli Instant Plus, the paid tier: 25 € in Europe, $25 elsewhere, paid
 * once per computer (or included in a Pelli Pro or Ultra plan). Pure helpers,
 * tested in plus.test.ts.
 */

export const PLUS_PRICE = 25

/** Plus as the interface assumes it before the engine answered: nothing. */
export const NO_PLUS: PlusStatus = { active: false, source: null, key: null, pending: false }

/** Crockford base32 alphabet of licence keys: no I, L, O, U. */
const KEY_CHARS = /[^0-9A-HJKMNP-TV-Z]/g
const KEY_VALID = /^[0-9A-HJKMNP-TV-Z]{16}$/
const KEY_LENGTH = 16

/**
 * What a human typed, as the key's 16 characters: case ignored, spaces and
 * dashes dropped; the letters people confuse are read as Crockford says
 * (O → 0, I and L → 1). Null when it can't be a key.
 */
export function normalizeKey(input: string): string | null {
  const raw = input
    .toUpperCase()
    .replace(/[\s-]/g, '')
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
  if (!KEY_VALID.test(raw)) return null
  return raw.match(/.{4}/g)!.join('-')
}

/** Formats the field as it is typed: groups of four, at most 16 characters. */
export function formatKeyInput(input: string): string {
  const raw = input
    .toUpperCase()
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
    .replace(KEY_CHARS, '')
    .slice(0, KEY_LENGTH)
  return raw.replace(/(.{4})(?=.)/g, '$1-')
}

/** "…‑7K2Q" (non-breaking hyphen): the end of the key, inside a sentence. */
export function shortKey(key: string | null): string | null {
  const last = key?.replace(/[\s-]/g, '').slice(-4)
  return last ? `…\u2011${last}` : null
}

/** "••••-••••-••••-7K2Q": only the last group of the key is shown. */
export function maskKey(key: string | null): string | null {
  if (!key) return null
  const last = key.replace(/[\s-]/g, '').slice(-4)
  return last ? `••••-••••-••••-${last}` : null
}

/** Regions that pay in euros (EU/EEA, Switzerland, the UK and the rest of Europe), as Pelli decides from the country. */
const EURO_REGIONS = new Set(
  'AD AL AT BA BE BG BY CH CY CZ DE DK EE ES FI FO FR GB GG GI GR HR HU IE IM IS IT JE LI LT LU LV MC MD ME MK MT NL NO PL PT RO RS SE SI SK SM UA VA XK'.split(' '),
)

/** Languages spoken mostly in Europe, for a locale tag without region. */
const EURO_LANGUAGES = new Set(['fr', 'es', 'de', 'it', 'nl', 'pl', 'sv'])

/** Currency of the offline price, from the computer's locales (region first, then language). */
export function fallbackCurrency(locales: readonly string[] = typeof navigator === 'undefined' ? [] : (navigator.languages ?? [navigator.language])): 'eur' | 'usd' {
  for (const tag of locales) {
    const region = tag.split(/[-_]/)[1]?.toUpperCase()
    if (region && /^[A-Z]{2}$/.test(region)) return EURO_REGIONS.has(region) ? 'eur' : 'usd'
  }
  const base = locales[0]?.toLowerCase().split(/[-_]/)[0]
  return base && EURO_LANGUAGES.has(base) ? 'eur' : 'usd'
}

/** "25 €", "$25": the price in the interface language. */
export function formatPlusPrice(currency: 'eur' | 'usd', language: Language, amount = PLUS_PRICE): string {
  return new Intl.NumberFormat(localeOf(language), { style: 'currency', currency, currencyDisplay: 'narrowSymbol', maximumFractionDigits: 0 })
    .format(amount)
    .replace(/ /g, ' ')
}

/** The price shown: Pelli's label when online, else the locale's fallback. */
export function plusPriceLabel(offer: PlusOffer | null, language: Language, locales?: readonly string[]): string {
  if (offer?.label) return offer.label
  return formatPlusPrice(fallbackCurrency(locales), language)
}

/** The views of the Plus dialog. */
export type PlusView = 'offer' | 'key' | 'waiting' | 'success' | 'error' | 'account'

/** Error codes the Plus dialog explains in its own words. */
export type PlusErrorCode = 'invalidKey' | 'otherDevice' | 'revoked' | 'offline' | 'expired' | 'notIncluded' | 'internal'

const KNOWN: PlusErrorCode[] = ['invalidKey', 'otherDevice', 'revoked', 'offline', 'expired', 'notIncluded']

/** Any engine code, as one the dialog knows (the rest reads "something went wrong"). */
export function plusErrorCode(code: string | null | undefined): PlusErrorCode {
  return KNOWN.includes(code as PlusErrorCode) ? (code as PlusErrorCode) : 'internal'
}

/**
 * Where the dialog goes when Plus changes under it: activated → success
 * (whatever it showed); a checkout that stops waiting without Plus → the
 * checkout expired (or was cancelled in the browser).
 */
export function viewAfterStatus(view: PlusView, before: PlusStatus, after: PlusStatus): { view: PlusView; error?: PlusErrorCode } {
  if (after.active && !before.active) return { view: 'success' }
  if (view === 'waiting' && before.pending && !after.pending && !after.active) return { view: 'error', error: 'expired' }
  return { view }
}
