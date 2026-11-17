import { describe, expect, it } from 'vitest'
import { MOCK_UPDATE_NOTES } from '@/lib/api/mock/fixtures'
import { releaseHighlights } from './release-notes'

describe('releaseHighlights', () => {
  it('reads the English list, then the French one, as plain text', () => {
    expect(releaseHighlights(MOCK_UPDATE_NOTES, 'en')).toEqual([
      'Send to a Pelli gallery straight from the export.',
      'Faster grid on shoots of 5,000 photos.',
      'Fixes for Fujifilm RAF previews.',
    ])
    expect(releaseHighlights(MOCK_UPDATE_NOTES, 'fr')[0]).toBe('Envoi vers une galerie Pelli, directement depuis l’export.')
  })

  it('gives nothing for other languages, empty notes or notes without a list', () => {
    expect(releaseHighlights(MOCK_UPDATE_NOTES, 'de')).toEqual([])
    expect(releaseHighlights(null, 'en')).toEqual([])
    expect(releaseHighlights("What's new: https://github.com/Synapsr/Pelli-Instant/releases/tag/v1.0.0", 'en')).toEqual([])
    expect(releaseHighlights('- [Notes](https://x.test) and `code`\n  - nested, ignored', 'en')).toEqual(['Notes and code'])
  })

  it('keeps at most five points', () => {
    expect(releaseHighlights(Array.from({ length: 8 }, (_, i) => `- point ${i}`).join('\n'), 'en')).toHaveLength(5)
  })
})
