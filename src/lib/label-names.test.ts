import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { LANGUAGES } from '@/lib/i18n'
import { LABEL_NAMES, labelName, labelNamesSample, resolveLabelLanguage } from './label-names'

const COLORS = ['red', 'yellow', 'green', 'blue', 'purple'] as const

describe('label names', () => {
  it('has the five names of every app language, as the engine writes them', () => {
    // The engine's table (`LabelLanguage::names`) must say exactly the same.
    const rust = readFileSync(new URL('../../crates/cull-types/src/lib.rs', import.meta.url), 'utf8')
    const engine = Object.fromEntries(
      [...rust.matchAll(/LabelLanguage::(\w\w) => \[([^\]]+)\]/g)].map(([, code, list]) => [code.toLowerCase(), [...list.matchAll(/"([^"]+)"/g)].map((m) => m[1])]),
    )
    expect(Object.keys(engine).sort()).toEqual([...LANGUAGES].sort())
    for (const language of LANGUAGES) {
      expect(COLORS.map((c) => LABEL_NAMES[language][c]), language).toEqual(engine[language])
      expect(new Set(Object.values(LABEL_NAMES[language])).size, language).toBe(5)
    }
    // No name means two colours, whatever the language.
    const colorOf = new Map<string, string>()
    for (const language of LANGUAGES) {
      for (const color of COLORS) {
        const name = LABEL_NAMES[language][color].toLowerCase()
        expect(colorOf.get(name) ?? color, `${language} ${name}`).toBe(color)
        colorOf.set(name, color)
      }
    }
  })

  it('shows a short sample of each set and the name a label gets', () => {
    expect(labelNamesSample('fr')).toBe('Rouge, Jaune, Vert…')
    expect(labelNamesSample('de')).toBe('Rot, Gelb, Grün…')
    expect(labelName('purple', 'it')).toBe('Viola')
    expect(labelName('none', 'en')).toBeNull()
  })

  it('follows the app language until the photographer picks one', () => {
    expect(resolveLabelLanguage(null, 'fr')).toBe('fr')
    expect(resolveLabelLanguage(undefined, 'sv')).toBe('sv')
    expect(resolveLabelLanguage('en', 'fr')).toBe('en')
  })
})
