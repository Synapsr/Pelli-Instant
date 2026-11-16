import { describe, expect, it } from 'vitest'
import { DEFAULT_SETTINGS, type ProjectSummary } from '@/lib/contract'
import { filterProjects, foldText, hasMoreProjects, RECENT_COUNT, sortProjects } from './projects'

function project(id: string, name: string, lastOpenedAt: number, rootPath = `/Photos/${name}`): ProjectSummary {
  return {
    id,
    name,
    rootPath,
    photoCount: 100,
    rawCount: 100,
    createdAt: 0,
    lastOpenedAt,
    analyzedAt: null,
    coverThumbUrl: null,
    counts: { selected: 0, toReview: 0, rejected: 0 },
    settings: DEFAULT_SETTINGS,
    precision: null,
  }
}

describe('projects view', () => {
  const list = [
    project('a', 'Mariage Léa & Thomas', 10),
    project('b', 'Baptême de Jules', 30),
    project('c', 'Séance couple Manon & Hugo', 20, '/Photos/2026-10-05 Manon Hugo – Kerlann'),
  ]

  it('sorts by last opened', () => {
    expect(sortProjects(list).map((p) => p.id)).toEqual(['b', 'c', 'a'])
    expect(list.map((p) => p.id)).toEqual(['a', 'b', 'c'])
  })

  it('searches names and folders, accents and case aside, every word', () => {
    expect(foldText('Léa & Thomas')).toBe('lea & thomas')
    expect(filterProjects(list, 'lea').map((p) => p.id)).toEqual(['a'])
    expect(filterProjects(list, 'BAPTEME').map((p) => p.id)).toEqual(['b'])
    expect(filterProjects(list, 'hugo couple').map((p) => p.id)).toEqual(['c'])
    expect(filterProjects(list, 'kerlann').map((p) => p.id)).toEqual(['c'])
    expect(filterProjects(list, 'mariage jules')).toEqual([])
    expect(filterProjects(list, '   ')).toHaveLength(3)
  })

  it('offers the full list only when some shoots are not shown', () => {
    const many = Array.from({ length: RECENT_COUNT + 1 }, (_, i) => project(String(i), `Shoot ${i}`, i))
    expect(hasMoreProjects(many.slice(0, RECENT_COUNT))).toBe(false)
    expect(hasMoreProjects(many)).toBe(true)
  })
})
