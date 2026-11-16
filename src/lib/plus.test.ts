import { describe, expect, it } from 'vitest'
import { fallbackCurrency, formatKeyInput, formatPlusPrice, maskKey, NO_PLUS, normalizeKey, plusErrorCode, plusPriceLabel, shortKey, viewAfterStatus } from './plus'

describe('Plus helpers', () => {
  it('reads a licence key the way people type it', () => {
    expect(normalizeKey('K7QM-4ZTP-9XRA-7K2Q')).toBe('K7QM-4ZTP-9XRA-7K2Q')
    expect(normalizeKey(' k7qm 4ztp 9xra 7k2q ')).toBe('K7QM-4ZTP-9XRA-7K2Q')
    // Crockford: O is a zero, I and L are ones (keys never hold them).
    expect(normalizeKey('K7QM-4ZTP-9XRA-7KOI')).toBe('K7QM-4ZTP-9XRA-7K01')
    expect(normalizeKey('K7QM-4ZTP-9XRA')).toBeNull()
    expect(normalizeKey('K7QM-4ZTP-9XRA-7K2QX')).toBeNull()
    expect(normalizeKey('K7QM-4ZTP-9XRA-7K2U')).toBeNull()
  })

  it('formats the key field as it is typed', () => {
    expect(formatKeyInput('k7qm4z')).toBe('K7QM-4Z')
    expect(formatKeyInput('K7QM-')).toBe('K7QM')
    expect(formatKeyInput('k7qm 4ztp 9xra 7k2q 99')).toBe('K7QM-4ZTP-9XRA-7K2Q')
    expect(formatKeyInput('u!?')).toBe('')
  })

  it('shows only the last group of a key', () => {
    expect(maskKey('K7QM-4ZTP-9XRA-7K2Q')).toBe('••••-••••-••••-7K2Q')
    expect(maskKey(null)).toBeNull()
    expect(shortKey('K7QM-4ZTP-9XRA-7K2Q')).toBe('…\u20117K2Q')
  })

  it('falls back to euros in Europe and dollars elsewhere', () => {
    expect(fallbackCurrency(['fr-FR'])).toBe('eur')
    expect(fallbackCurrency(['en-GB', 'en'])).toBe('eur')
    expect(fallbackCurrency(['de-CH'])).toBe('eur')
    expect(fallbackCurrency(['en-US'])).toBe('usd')
    expect(fallbackCurrency(['pt-BR'])).toBe('usd')
    expect(fallbackCurrency(['sv'])).toBe('eur')
    expect(fallbackCurrency(['en'])).toBe('usd')
    expect(fallbackCurrency([])).toBe('usd')
  })

  it('prints the price like Pelli does, Pelli’s label first', () => {
    expect(formatPlusPrice('eur', 'fr')).toBe('25 €')
    expect(formatPlusPrice('usd', 'en')).toBe('$25')
    expect(plusPriceLabel({ amount: 2500, currency: 'usd', label: 'US$25' }, 'fr', ['fr-FR'])).toBe('US$25')
    expect(plusPriceLabel(null, 'fr', ['fr-FR'])).toBe('25 €')
    expect(plusPriceLabel(null, 'en', ['en-US'])).toBe('$25')
  })

  it('maps engine codes to the ones the dialog explains', () => {
    expect(plusErrorCode('otherDevice')).toBe('otherDevice')
    expect(plusErrorCode('offline')).toBe('offline')
    expect(plusErrorCode('http500')).toBe('internal')
    expect(plusErrorCode(undefined)).toBe('internal')
  })

  it('moves the dialog when Plus changes under it', () => {
    const pending = { ...NO_PLUS, pending: true }
    const active = { active: true, source: 'license' as const, key: 'K7QM-4ZTP-9XRA-7K2Q', pending: false }
    expect(viewAfterStatus('waiting', pending, active)).toEqual({ view: 'success' })
    expect(viewAfterStatus('offer', NO_PLUS, active)).toEqual({ view: 'success' })
    expect(viewAfterStatus('waiting', pending, NO_PLUS)).toEqual({ view: 'error', error: 'expired' })
    expect(viewAfterStatus('waiting', pending, pending)).toEqual({ view: 'waiting' })
    expect(viewAfterStatus('offer', pending, NO_PLUS)).toEqual({ view: 'offer' })
  })
})
