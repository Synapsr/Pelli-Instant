import { create } from 'zustand'
import type { Language } from '@/lib/contract'
import { de } from './locales/de'
import { en } from './locales/en'
import { es } from './locales/es'
import { fr } from './locales/fr'
import { it } from './locales/it'
import { nl } from './locales/nl'
import { pl } from './locales/pl'
import { pt } from './locales/pt'
import { sv } from './locales/sv'
import type { Catalogue, MessageKey, MessageTree, MessageValue, Plural, Vars } from './types'

export type Messages = Catalogue<typeof fr>
export type TKey = MessageKey<typeof fr>
export type Translate = (key: TKey, vars?: Vars) => string

export const CATALOGUES: Record<Language, Messages> = { en, fr, es, de, it, pt, nl, pl, sv }
/** The languages of Pelli, in Pelli's order. */
export const LANGUAGES: Language[] = ['en', 'fr', 'es', 'de', 'it', 'pt', 'nl', 'pl', 'sv']

/** Each language in its own words, for the language menu. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: 'English',
  fr: 'Français',
  es: 'Español',
  de: 'Deutsch',
  it: 'Italiano',
  pt: 'Português',
  nl: 'Nederlands',
  pl: 'Polski',
  sv: 'Svenska',
}

const LOCALES: Record<Language, string> = {
  en: 'en-GB',
  fr: 'fr-FR',
  es: 'es-ES',
  de: 'de-DE',
  it: 'it-IT',
  pt: 'pt-BR',
  nl: 'nl-NL',
  pl: 'pl-PL',
  sv: 'sv-SE',
}

export function isLanguage(value: unknown): value is Language {
  return typeof value === 'string' && (LANGUAGES as string[]).includes(value)
}

/** The first of the computer's preferred languages the app speaks, else English. */
export function detectLanguage(preferred: readonly string[] = typeof navigator === 'undefined' ? [] : (navigator.languages ?? [navigator.language])): Language {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split(/[-_]/)[0]
    if (isLanguage(base)) return base
  }
  return 'en'
}

export function localeOf(language: Language): string {
  return LOCALES[language]
}

function lookup(tree: MessageTree, key: string): MessageValue | undefined {
  let node: MessageValue | MessageTree | undefined = tree
  for (const part of key.split('.')) {
    if (!node || typeof node === 'string') return undefined
    node = (node as MessageTree)[part]
  }
  return node as MessageValue | undefined
}

function isPlural(value: unknown): value is Plural {
  return typeof value === 'object' && value !== null && 'other' in value
}

const pluralRules = new Map<Language, Intl.PluralRules>()
const numberFormats = new Map<Language, Intl.NumberFormat>()

function numberFormat(language: Language): Intl.NumberFormat {
  let nf = numberFormats.get(language)
  if (!nf) {
    nf = new Intl.NumberFormat(localeOf(language))
    numberFormats.set(language, nf)
  }
  return nf
}

/** Translates `key` in `language`; numbers in `vars` are formatted for the locale. */
export function translate(language: Language, key: TKey, vars?: Vars): string {
  let value = lookup(CATALOGUES[language] as unknown as MessageTree, key) ?? lookup(fr as unknown as MessageTree, key)
  if (value === undefined) return key
  if (isPlural(value)) {
    const count = Number(vars?.count ?? 0)
    let rules = pluralRules.get(language)
    if (!rules) {
      rules = new Intl.PluralRules(localeOf(language))
      pluralRules.set(language, rules)
    }
    const form = rules.select(count)
    value = count === 0 && value.zero ? value.zero : form === 'one' ? value.one : ((form === 'few' || form === 'many' ? value[form] : undefined) ?? value.other)
  }
  if (!vars) return value
  const nf = numberFormat(language)
  return value.replace(/\{(\w+)\}/g, (match, name: string) => {
    const v = vars[name]
    if (v === undefined) return match
    return typeof v === 'number' ? nf.format(v).replace(/\u202f/g, '\u00a0') : v
  })
}

interface I18nState {
  language: Language
  setLanguage: (language: Language) => void
}

export const useI18nStore = create<I18nState>((set) => ({
  language: detectLanguage(),
  setLanguage: (language) => {
    if (typeof document !== 'undefined') document.documentElement.lang = language
    set({ language })
  },
}))

/** Non-React access (stores, toasts). */
export function t(key: TKey, vars?: Vars): string {
  return translate(useI18nStore.getState().language, key, vars)
}

const translators = new Map<Language, Translate>()

export function translatorFor(language: Language): Translate {
  let fn = translators.get(language)
  if (!fn) {
    fn = (key, vars) => translate(language, key, vars)
    translators.set(language, fn)
  }
  return fn
}

/** React access: re-renders when the language changes (stable per language). */
export function useT(): Translate {
  return translatorFor(useI18nStore((s) => s.language))
}

export function useLanguage(): Language {
  return useI18nStore((s) => s.language)
}
