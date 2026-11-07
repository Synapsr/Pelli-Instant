import { describe, expect, it } from 'vitest'
import { CATALOGUES, detectLanguage, LANGUAGES, translate } from './index'
import type { MessageTree } from './types'

function leaves(tree: MessageTree, prefix = ''): Map<string, string> {
  const out = new Map<string, string>()
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (typeof value === 'string') out.set(path, value)
    else if ('other' in value && typeof value.other === 'string') {
      for (const [form, text] of Object.entries(value)) out.set(`${path}#${form}`, String(text))
    } else for (const [k, v] of leaves(value as MessageTree, path)) out.set(k, v)
  }
  return out
}

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

describe('i18n', () => {
  const reference = leaves(CATALOGUES.fr as unknown as MessageTree)

  it.each(LANGUAGES)('%s has every message, none empty, with the same placeholders', (language) => {
    const messages = leaves(CATALOGUES[language] as unknown as MessageTree)
    for (const [key, text] of reference) {
      const base = key.split('#')[0]
      const translated = messages.get(key) ?? messages.get(`${base}#other`)
      expect(translated, `${language}: ${key}`).toBeTruthy()
      if (!key.includes('#zero') && !key.includes('#one')) expect(placeholders(translated as string), `${language}: ${key}`).toEqual(placeholders(text))
    }
  })

  it('is a real translation, not a copy of English', () => {
    for (const language of ['es', 'de', 'it', 'pt', 'nl', 'pl', 'sv'] as const) {
      expect(translate(language, 'review.tabs.selected')).not.toBe(translate('en', 'review.tabs.selected'))
      expect(translate(language, 'signIn.cta')).not.toBe(translate('en', 'signIn.cta'))
    }
  })

  it('interpolates and pluralises with the locale rules', () => {
    expect(translate('fr', 'common.photos', { count: 1 })).toBe('1 photo')
    expect(translate('fr', 'common.photos', { count: 0 })).toBe('0 photo')
    expect(translate('fr', 'common.photos', { count: 1284 })).toBe('1 284 photos')
    expect(translate('en', 'common.photos', { count: 0 })).toBe('0 photos')
    expect(translate('de', 'common.photos', { count: 2 })).toMatch(/^2 /)
    expect(translate('en', 'marks.stars', { count: 0 })).toBe('No stars')
    expect(translate('fr', 'home.greeting', { name: 'Camille' })).toBe('Bonjour Camille')
  })

  it('uses the Polish few and many forms', () => {
    const photos = (count: number) => translate('pl', 'common.photos', { count })
    expect(photos(1)).toBe('1 zdjęcie')
    expect(photos(3)).toBe('3 zdjęcia')
    expect(photos(5)).toBe('5 zdjęć')
    expect(photos(22)).toBe('22 zdjęcia')
  })

  it('picks the computer language the app speaks, else English', () => {
    expect(detectLanguage(['pl-PL', 'en-US'])).toBe('pl')
    expect(detectLanguage(['pt-BR'])).toBe('pt')
    expect(detectLanguage(['ja-JP', 'sv-SE'])).toBe('sv')
    expect(detectLanguage(['ja-JP'])).toBe('en')
    expect(detectLanguage([])).toBe('en')
  })
})
