import type { SearchHit } from '@/lib/contract'
import type { MockRecord } from './shoot'

/**
 * Fake CLIP text search: keyword matching on per-photo tags with synonyms in
 * the five app languages, scored like a cosine similarity (≈ 0.15 … 0.35).
 */

const SYNONYMS: Record<string, string[]> = {
  mariee: ['bride', 'novia', 'braut', 'sposa', 'mariees'],
  marie: ['groom', 'novio', 'brautigam', 'sposo'],
  maries: ['couple', 'newlyweds', 'novios', 'brautpaar', 'sposi', 'pareja', 'coppia', 'paar'],
  bouquet: ['ramo', 'brautstrauss', 'strauss', 'mazzo'],
  fleurs: ['flowers', 'flores', 'blumen', 'fiori', 'fleur', 'flower'],
  alliances: ['rings', 'ring', 'anillos', 'ringe', 'anelli', 'bagues', 'alliance'],
  gateau: ['cake', 'tarta', 'pastel', 'torte', 'kuchen', 'torta'],
  danse: ['dance', 'dancing', 'baile', 'tanz', 'ballo', 'danser'],
  soiree: ['party', 'fiesta', 'feier', 'festa', 'fete'],
  ceremonie: ['ceremony', 'ceremonia', 'zeremonie', 'trauung', 'cerimonia'],
  'coucher de soleil': ['sunset', 'atardecer', 'sonnenuntergang', 'tramonto', 'golden hour'],
  champagne: ['champan', 'sekt', 'prosecco', 'toast', 'brindis'],
  enfant: ['child', 'kid', 'kids', 'children', 'nino', 'kind', 'bambino', 'enfants'],
  groupe: ['group', 'grupo', 'gruppe', 'gruppo', 'famille', 'family', 'familia', 'familie', 'famiglia'],
  diner: ['dinner', 'cena', 'abendessen', 'repas', 'meal'],
  robe: ['dress', 'gown', 'vestido', 'kleid', 'abito'],
  costume: ['suit', 'traje', 'anzug', 'abito'],
  sourire: ['smile', 'smiling', 'sonrisa', 'lacheln', 'sorriso', 'rires', 'laugh', 'laughing'],
  portrait: ['retrato', 'portrat', 'ritratto'],
  preparatifs: ['getting ready', 'preparativos', 'vorbereitung', 'preparativi', 'preparation'],
  cocktail: ['vin d honneur', 'aperitif', 'aperitivo', 'empfang', 'reception'],
  voile: ['veil', 'velo', 'schleier'],
  chateau: ['castle', 'castillo', 'schloss', 'castello', 'domaine'],
  arche: ['arch', 'arco', 'bogen'],
  table: ['mesa', 'tisch', 'tavola', 'decoration', 'deco'],
}

export function normalise(text: string): string {
  return text
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9 ]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

const CANONICAL = new Map<string, string>()
for (const [canonical, words] of Object.entries(SYNONYMS)) {
  CANONICAL.set(canonical, canonical)
  words.forEach((w) => CANONICAL.set(normalise(w), canonical))
}

/** Query → list of canonical concepts (multi-word synonyms first). */
export function concepts(query: string): string[] {
  let q = ` ${normalise(query)} `
  const found: string[] = []
  const phrases = [...CANONICAL.keys()].filter((k) => k.includes(' ')).sort((a, b) => b.length - a.length)
  for (const phrase of phrases) {
    if (q.includes(` ${phrase} `)) {
      found.push(CANONICAL.get(phrase) as string)
      q = q.replace(` ${phrase} `, ' ')
    }
  }
  for (const word of q.split(' ').filter((w) => w.length >= 3)) {
    found.push(CANONICAL.get(word) ?? word)
  }
  return [...new Set(found)]
}

export function searchRecords(records: MockRecord[], query: string): SearchHit[] {
  const wanted = concepts(query)
  if (!wanted.length) return []
  const hits: SearchHit[] = []
  for (const r of records) {
    const tags = r.tags.map(normalise)
    let matched = 0
    for (const c of wanted) {
      const hit = c.includes(' ')
        ? tags.some((t) => t === c || t.includes(c))
        : tags.some((t) => t === c || t.split(' ').includes(c))
      if (hit) matched += 1
    }
    if (!matched) continue
    const coverage = matched / wanted.length
    const jitter = ((r.index * 2654435761) % 1000) / 1000
    const score = 0.12 + coverage * 0.17 + r.m.aesthetic * 0.04 + jitter * 0.015
    hits.push({ photoId: r.id, score: Math.round(score * 1000) / 1000 })
  }
  const best = Math.max(0, ...hits.map((h) => h.score))
  return hits.filter((h) => h.score >= best - 0.065).sort((a, b) => b.score - a.score)
}
