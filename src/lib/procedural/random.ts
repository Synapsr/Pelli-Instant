/** Small deterministic PRNG (mulberry32) so mock shoots are reproducible. */
export interface Rng {
  /** float in [0, 1) */
  next(): number
  range(min: number, max: number): number
  int(min: number, maxInclusive: number): number
  pick<T>(items: readonly T[]): T
  chance(probability: number): boolean
  /** roughly normal, mean 0, sd 1 (sum of uniforms) */
  gauss(): number
}

export function createRng(seed: number): Rng {
  let a = seed >>> 0 || 0x9e3779b9
  const next = (): number => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
  return {
    next,
    range: (min, max) => min + (max - min) * next(),
    int: (min, maxInclusive) => min + Math.floor(next() * (maxInclusive - min + 1)),
    pick: (items) => items[Math.floor(next() * items.length)],
    chance: (probability) => next() < probability,
    gauss: () => (next() + next() + next() + next() - 2) * 1.7320508,
  }
}

/** Stable 32-bit hash of a string (FNV-1a). */
export function hashString(value: string): number {
  let h = 0x811c9dc5
  for (let i = 0; i < value.length; i++) {
    h ^= value.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return h >>> 0
}

export function clamp(value: number, min = 0, max = 1): number {
  return Math.min(max, Math.max(min, value))
}
