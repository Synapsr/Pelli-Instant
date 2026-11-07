import { clamp, type Rng } from './random'
import type { DetailKind, HairStyle, Person, Role, SceneKind, Shot } from './types'

/** Base composition of a moment; each frame of a burst jitters it. */
export interface MomentPlan {
  kind: SceneKind
  backdrop: number
  aspect: number
  people: Person[]
  detail: DetailKind | null
}

export interface FrameDefects {
  defocus: number
  subjectDefocus: number
  motion: number
  exposure: number
  /** probability that each face blinks on this frame */
  blink: number
}

const SKINS = ['#f3cfb1', '#eab894', '#dca27c', '#c98a64', '#ad6f4c', '#8d5538', '#6e412b', '#f6d9c2']
const HAIRS = ['#2a1d16', '#3b2a1e', '#5a3d27', '#8a6239', '#b88a50', '#d8bb84', '#1a1514', '#7a7a7a']
const GUEST_OUTFITS = [
  '#7d8f7a',
  '#c48a8a',
  '#2f3a56',
  '#6b2d3a',
  '#c9a24a',
  '#8fb0c9',
  '#d8cfc4',
  '#3d3d3d',
  '#a0657a',
  '#46624f',
  '#b86f4b',
  '#5d6b8a',
]
const SUITS = ['#232c3a', '#2a2a2d', '#38485a', '#55595f', '#4b3f33']
const BRIDE_DRESS = '#f4efe6'
const ROBE = '#efd9cf'

function person(rng: Rng, role: Role, x: number, y: number, size: number, extra: Partial<Person> = {}): Person {
  const longHair = role === 'bride' || (role === 'guest' && rng.chance(0.55)) || (role === 'child' && rng.chance(0.4))
  const hairStyle: HairStyle =
    role === 'bride' ? rng.pick<HairStyle>(['bun', 'long', 'bun']) : longHair ? rng.pick<HairStyle>(['long', 'bun', 'long']) : rng.chance(0.08) ? 'bald' : 'short'
  const outfit =
    role === 'bride'
      ? BRIDE_DRESS
      : role === 'groom'
        ? rng.pick(SUITS)
        : role === 'officiant'
          ? '#3c3a40'
          : longHair
            ? rng.pick(GUEST_OUTFITS)
            : rng.pick([...SUITS, ...GUEST_OUTFITS.slice(0, 4)])
  return {
    x,
    y,
    size,
    role,
    skin: rng.pick(SKINS),
    hair: role === 'bride' ? rng.pick(HAIRS.slice(0, 6)) : rng.pick(HAIRS),
    hairStyle,
    outfit,
    veil: role === 'bride' && rng.chance(0.45),
    eyes: [1, 1],
    turn: rng.range(-0.35, 0.35),
    smile: rng.range(0.3, 1),
    ...extra,
  }
}

function spread(count: number, from: number, to: number, rng: Rng, jitter = 0.015): number[] {
  if (count === 1) return [(from + to) / 2]
  return Array.from({ length: count }, (_, i) => from + ((to - from) * i) / (count - 1) + rng.range(-jitter, jitter))
}

export function composeMoment(kind: SceneKind, rng: Rng): MomentPlan {
  const backdrop = rng.int(1, 2 ** 30)
  const portrait = (kind === 'prep' || kind === 'couple') && rng.chance(0.28)
  const aspect = portrait ? 2 / 3 : kind !== 'groups' && rng.chance(0.06) ? 2 / 3 : 3 / 2
  const people: Person[] = []
  let detail: DetailKind | null = null
  const p = portrait ? 1.35 : 1

  switch (kind) {
    case 'prep': {
      if (rng.chance(0.38)) {
        detail = rng.pick<DetailKind>(['rings', 'bouquet', 'rings', 'bouquet', 'table'])
        break
      }
      const size = rng.range(0.15, 0.27) * (portrait ? 0.85 : 1)
      const x = portrait ? rng.range(0.42, 0.58) : rng.range(0.34, 0.62)
      people.push(person(rng, 'bride', x, rng.range(0.3, 0.4), size, { outfit: rng.chance(0.5) ? ROBE : BRIDE_DRESS, veil: rng.chance(0.25) }))
      if (!portrait && rng.chance(0.55)) {
        const side = x > 0.5 ? -1 : 1
        people.push(person(rng, 'guest', x + side * rng.range(0.2, 0.28), rng.range(0.32, 0.44), size * rng.range(0.82, 0.96), { turn: side * -0.45 }))
      }
      break
    }
    case 'ceremony': {
      if (rng.chance(0.62)) {
        const size = rng.range(0.075, 0.12)
        const cx = rng.range(0.44, 0.56)
        people.push(person(rng, 'groom', cx - size * 0.95, rng.range(0.36, 0.41), size, { turn: 0.55 }))
        people.push(person(rng, 'bride', cx + size * 0.95, rng.range(0.37, 0.42), size * 0.95, { turn: -0.55 }))
        if (rng.chance(0.5)) people.unshift(person(rng, 'officiant', cx + rng.range(-0.02, 0.02), 0.31, size * 0.82, { turn: 0 }))
      } else {
        const n = rng.int(4, 7)
        spread(n, 0.12, 0.88, rng, 0.03).forEach((x) => people.push(person(rng, 'guest', x, rng.range(0.6, 0.7), rng.range(0.07, 0.1))))
      }
      break
    }
    case 'groups': {
      const back = rng.int(4, 7)
      const front = rng.int(4, 6)
      const size = rng.range(0.05, 0.065)
      spread(back, 0.16, 0.84, rng).forEach((x) => people.push(person(rng, 'guest', x, 0.4 + rng.range(-0.01, 0.01), size * 0.94, { turn: rng.range(-0.15, 0.15) })))
      spread(front, 0.2, 0.8, rng).forEach((x, i) => {
        const middle = Math.floor(front / 2)
        const role: Role = i === middle - 1 ? 'groom' : i === middle ? 'bride' : rng.chance(0.15) ? 'child' : 'guest'
        const s = role === 'child' ? size * 0.82 : size
        people.push(person(rng, role, x, (role === 'child' ? 0.56 : 0.5) + rng.range(-0.008, 0.008), s, { turn: rng.range(-0.12, 0.12) }))
      })
      break
    }
    case 'couple': {
      const layout = rng.int(0, 2)
      if (layout === 2 || portrait) {
        people.push(person(rng, 'bride', rng.range(0.42, 0.58), rng.range(0.3, 0.38), rng.range(0.18, 0.26) / p, { turn: rng.range(-0.3, 0.3) }))
        if (portrait && rng.chance(0.4)) people.unshift(person(rng, 'groom', 0.62, 0.3, 0.16, { turn: -0.4 }))
      } else {
        const size = rng.range(0.13, 0.21)
        const cx = rng.range(0.4, 0.6)
        const gap = layout === 0 ? size * 0.85 : size * 0.6
        people.push(person(rng, 'groom', cx - gap, rng.range(0.3, 0.36), size, { turn: 0.5 }))
        people.push(person(rng, 'bride', cx + gap, rng.range(0.34, 0.4), size * 0.94, { turn: layout === 0 ? -0.5 : -0.2 }))
      }
      break
    }
    case 'cocktail': {
      if (rng.chance(0.18)) {
        detail = 'glasses'
        break
      }
      const n = rng.int(2, 4)
      spread(n, 0.22, 0.78, rng, 0.05).forEach((x) =>
        people.push(person(rng, rng.chance(0.08) ? 'child' : 'guest', x, rng.range(0.3, 0.42), rng.range(0.09, 0.15))),
      )
      if (rng.chance(0.3)) {
        const i = rng.int(0, people.length - 1)
        people[i] = { ...people[i], role: rng.chance(0.5) ? 'bride' : 'groom', outfit: rng.chance(0.5) ? BRIDE_DRESS : rng.pick(SUITS) }
      }
      break
    }
    case 'dinner': {
      const roll = rng.next()
      if (roll < 0.2) {
        detail = rng.pick<DetailKind>(['cake', 'table'])
      } else if (roll < 0.38) {
        people.push(person(rng, 'guest', rng.range(0.35, 0.65), rng.range(0.28, 0.34), rng.range(0.1, 0.14), { turn: rng.range(-0.2, 0.2) }))
      } else {
        const n = rng.int(3, 5)
        spread(n, 0.15, 0.85, rng, 0.04).forEach((x) => people.push(person(rng, 'guest', x, rng.range(0.42, 0.52), rng.range(0.07, 0.1))))
      }
      break
    }
    case 'party': {
      const n = rng.int(3, 6)
      spread(n, 0.14, 0.86, rng, 0.06).forEach((x) =>
        people.push(person(rng, 'guest', x, rng.range(0.32, 0.5), rng.range(0.07, 0.13), { turn: rng.range(-0.6, 0.6) })),
      )
      if (rng.chance(0.4)) {
        const i = Math.floor(people.length / 2)
        people[i] = { ...people[i], role: 'bride', outfit: BRIDE_DRESS, hairStyle: 'bun' }
      }
      break
    }
  }
  people.sort((a, b) => a.size - b.size)
  return { kind, backdrop, aspect, people, detail }
}

/** One frame of a moment: small framing/pose variations + defects. */
export function frameFromMoment(plan: MomentPlan, rng: Rng, defects: FrameDefects): Shot {
  const dx = rng.range(-0.012, 0.012)
  const dy = rng.range(-0.008, 0.008)
  const zoom = 1 + rng.range(-0.025, 0.025)
  const people = plan.people.map((p) => {
    const blink = rng.chance(defects.blink)
    const open = blink ? rng.range(0.02, 0.18) : rng.range(0.82, 1)
    return {
      ...p,
      x: clamp(0.5 + (p.x - 0.5) * zoom + dx + rng.range(-0.004, 0.004), 0.04, 0.96),
      y: clamp(0.5 + (p.y - 0.5) * zoom + dy + rng.range(-0.003, 0.003), 0.08, 0.9),
      size: p.size * zoom,
      eyes: [open, blink ? open * rng.range(0.8, 1.2) : rng.range(0.82, 1)] as [number, number],
      turn: clamp(p.turn + rng.range(-0.08, 0.08), -1, 1),
      smile: clamp(p.smile + rng.range(-0.15, 0.15)),
    }
  })
  return {
    seed: rng.int(1, 2 ** 30),
    kind: plan.kind,
    backdrop: plan.backdrop,
    aspect: plan.aspect,
    people,
    detail: plan.detail,
    defocus: defects.defocus,
    subjectDefocus: defects.subjectDefocus,
    motion: defects.motion,
    motionAngle: rng.range(-0.5, 0.5),
    exposure: defects.exposure,
  }
}
