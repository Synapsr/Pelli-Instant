import { clamp, createRng, type Rng } from './random'
import type { DetailKind, Person, SceneKind, Shot } from './types'

/**
 * Canvas renderer of procedural shots. Backdrops are painted at low
 * resolution and upscaled (cheap, natural-looking depth of field); people and
 * details are painted sharp at full resolution, then defects (focus miss,
 * motion, exposure) and a light grade (vignette, grain) are applied.
 */

type Ctx = CanvasRenderingContext2D

interface Surface {
  canvas: HTMLCanvasElement
  ctx: Ctx
}

const surfaces = new Map<string, Surface>()

function surface(name: string, w: number, h: number): Surface {
  let s = surfaces.get(name)
  if (!s) {
    const canvas = document.createElement('canvas')
    const ctx = canvas.getContext('2d', { willReadFrequently: false })
    if (!ctx) throw new Error('Canvas 2D unavailable')
    s = { canvas, ctx }
    surfaces.set(name, s)
  }
  if (s.canvas.width !== w || s.canvas.height !== h) {
    s.canvas.width = w
    s.canvas.height = h
  } else {
    s.ctx.setTransform(1, 0, 0, 1, 0, 0)
    s.ctx.globalAlpha = 1
    s.ctx.globalCompositeOperation = 'source-over'
    s.ctx.filter = 'none'
    s.ctx.clearRect(0, 0, w, h)
  }
  s.ctx.imageSmoothingEnabled = true
  s.ctx.imageSmoothingQuality = 'high'
  return s
}

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.slice(1), 16)
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
}

/** Lighten (amount > 0) or darken (amount < 0) a hex colour. */
function shade(hex: string, amount: number): string {
  const [r, g, b] = hexToRgb(hex)
  const t = amount < 0 ? 0 : 255
  const p = Math.abs(amount)
  const mix = (c: number) => Math.round((t - c) * p + c)
  return `rgb(${mix(r)},${mix(g)},${mix(b)})`
}

function rgba(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex)
  return `rgba(${r},${g},${b},${alpha})`
}

function bokeh(ctx: Ctx, x: number, y: number, r: number, color: string, alpha: number): void {
  const g = ctx.createRadialGradient(x, y, 0, x, y, r)
  g.addColorStop(0, rgba(color, alpha))
  g.addColorStop(0.72, rgba(color, alpha * 0.85))
  g.addColorStop(1, rgba(color, 0))
  ctx.fillStyle = g
  ctx.beginPath()
  ctx.arc(x, y, r, 0, Math.PI * 2)
  ctx.fill()
}

function glow(ctx: Ctx, x: number, y: number, r: number, color: string, alpha: number): void {
  const g = ctx.createRadialGradient(x, y, 0, x, y, r)
  g.addColorStop(0, rgba(color, alpha))
  g.addColorStop(1, rgba(color, 0))
  ctx.fillStyle = g
  ctx.fillRect(x - r, y - r, r * 2, r * 2)
}

function vGradient(ctx: Ctx, w: number, y0: number, y1: number, stops: [number, string][]): void {
  const g = ctx.createLinearGradient(0, y0, 0, y1)
  stops.forEach(([o, c]) => g.addColorStop(o, c))
  ctx.fillStyle = g
  ctx.fillRect(0, y0, w, y1 - y0)
}

function catenary(ctx: Ctx, w: number, y: number, sag: number, rng: Rng, r: number, color: string, count: number): void {
  for (let i = 0; i <= count; i++) {
    const t = i / count
    const x = t * w
    const yy = y + sag * 4 * t * (1 - t) + rng.range(-r * 0.3, r * 0.3)
    bokeh(ctx, x, yy, r * rng.range(0.7, 1.25), color, rng.range(0.55, 0.95))
  }
}

// ---------------------------------------------------------------------------
// Backdrops (painted small: soft = 1/5 res, mid = 1/2.5 res)
// ---------------------------------------------------------------------------

interface Backdrop {
  /** blurred far plane */
  soft(ctx: Ctx, w: number, h: number, rng: Rng): void
  /** closer elements, less blurred */
  mid?(ctx: Ctx, w: number, h: number, rng: Rng): void
  /** key light horizontal position, 0 (left) … 1 (right) */
  light(rng: Rng): number
  grade: string
  gradeAlpha: number
}

const BACKDROPS: Record<SceneKind, Backdrop> = {
  prep: {
    light: (rng) => (rng.chance(0.5) ? 0.15 : 0.85),
    soft(ctx, w, h, rng) {
      const warm = rng.range(0, 1)
      vGradient(ctx, w, 0, h, [
        [0, shade('#e6d8c6', warm * 0.05)],
        [1, shade('#b9a286', -warm * 0.1)],
      ])
      const left = rng.chance(0.5)
      const wx = left ? w * 0.02 : w * 0.62
      const ww = w * 0.34
      glow(ctx, wx + ww / 2, h * 0.38, w * 0.75, '#fff6e6', 0.7)
      ctx.fillStyle = '#fffaf2'
      ctx.fillRect(wx, h * 0.06, ww, h * 0.62)
      ctx.fillStyle = 'rgba(205,190,168,0.9)'
      ctx.fillRect(wx + ww / 2 - w * 0.007, h * 0.06, w * 0.014, h * 0.62)
      ctx.fillRect(wx, h * 0.34, ww, h * 0.014)
      ctx.fillStyle = 'rgba(244,236,226,0.95)'
      ctx.fillRect(left ? wx + ww : wx - w * 0.09, h * 0.02, w * 0.09, h * 0.8)
      ctx.fillStyle = '#6f523d'
      ctx.fillRect(left ? w * 0.55 : w * 0.02, h * 0.74, w * 0.42, h * 0.3)
      bokeh(ctx, left ? w * 0.86 : w * 0.14, h * 0.62, h * 0.1, '#ffd9a0', 0.6)
      ctx.fillStyle = '#5c7a4d'
      for (let i = 0; i < 6; i++) bokeh(ctx, (left ? w * 0.9 : w * 0.08) + rng.range(-w * 0.05, w * 0.05), h * rng.range(0.55, 0.72), h * 0.06, '#5c7a4d', 0.9)
    },
    grade: '#ffcf9e',
    gradeAlpha: 0.1,
  },
  ceremony: {
    light: () => 0.3,
    soft(ctx, w, h, rng) {
      vGradient(ctx, w, 0, h * 0.5, [
        [0, '#b8d1e3'],
        [1, '#efe8dc'],
      ])
      for (let i = 0; i < 26; i++) {
        const x = rng.range(-0.05, 1.05) * w
        const r = rng.range(0.07, 0.17) * h
        bokeh(ctx, x, h * 0.48 - r * 0.3, r, rng.pick(['#3e5a3a', '#4f6e45', '#2f4a2f', '#5d7a4b']), 0.95)
      }
      vGradient(ctx, w, h * 0.47, h, [
        [0, '#86a05e'],
        [1, '#4d6a35'],
      ])
      ctx.fillStyle = 'rgba(214,201,178,0.85)'
      ctx.beginPath()
      ctx.moveTo(w * 0.47, h * 0.5)
      ctx.lineTo(w * 0.53, h * 0.5)
      ctx.lineTo(w * 0.7, h)
      ctx.lineTo(w * 0.3, h)
      ctx.fill()
    },
    mid(ctx, w, h, rng) {
      const l = w * 0.32
      const r = w * 0.68
      ctx.strokeStyle = '#e9e2d6'
      ctx.lineWidth = w * 0.018
      ctx.beginPath()
      ctx.moveTo(l, h * 0.78)
      ctx.lineTo(l, h * 0.28)
      ctx.quadraticCurveTo(w * 0.5, h * 0.02, r, h * 0.28)
      ctx.lineTo(r, h * 0.78)
      ctx.stroke()
      const flowers = ['#fbf7f2', '#f2c6c2', '#f5d0b0', '#fffdf9', '#e7a9a6']
      for (let i = 0; i < 70; i++) {
        const t = rng.next()
        let x: number
        let y: number
        if (t < 0.4) {
          const a = rng.range(0.15, 0.85)
          x = l + (r - l) * a
          y = h * 0.28 - Math.sin(a * Math.PI) * h * 0.13
        } else {
          x = rng.chance(0.5) ? l : r
          y = h * rng.range(0.2, 0.5)
        }
        const leaf = rng.chance(0.35)
        bokeh(ctx, x + rng.range(-w * 0.025, w * 0.025), y + rng.range(-h * 0.03, h * 0.03), h * rng.range(0.014, 0.03), leaf ? '#6f8f5c' : rng.pick(flowers), 1)
      }
      for (let row = 0; row < 3; row++) {
        for (let i = 0; i < 4; i++) {
          const y = h * (0.82 + row * 0.07)
          ctx.fillStyle = 'rgba(245,242,236,0.9)'
          ctx.fillRect(w * (0.04 + i * 0.065), y, w * 0.045, h * 0.05)
          ctx.fillRect(w * (0.76 + i * 0.065), y, w * 0.045, h * 0.05)
        }
      }
    },
    grade: '#ffe6c4',
    gradeAlpha: 0.06,
  },
  groups: {
    light: () => 0.35,
    soft(ctx, w, h) {
      vGradient(ctx, w, 0, h * 0.2, [
        [0, '#c7d7e2'],
        [1, '#e7e3da'],
      ])
      ctx.fillStyle = '#525a66'
      ctx.fillRect(0, h * 0.1, w, h * 0.1)
      vGradient(ctx, w, h * 0.18, h * 0.64, [
        [0, '#d3c6b0'],
        [1, '#b9aa92'],
      ])
      for (let i = 0; i < 7; i++) {
        const x = w * (0.04 + i * 0.14)
        ctx.fillStyle = '#5f6870'
        ctx.fillRect(x, h * 0.24, w * 0.07, h * 0.22)
        ctx.beginPath()
        ctx.arc(x + w * 0.035, h * 0.24, w * 0.035, Math.PI, 0)
        ctx.fill()
        ctx.fillStyle = 'rgba(236,230,220,0.8)'
        ctx.fillRect(x + w * 0.033, h * 0.21, w * 0.004, h * 0.25)
      }
      vGradient(ctx, w, h * 0.6, h * 0.72, [
        [0, '#dcd3c4'],
        [1, '#c4b9a7'],
      ])
      vGradient(ctx, w, h * 0.72, h, [
        [0, '#7a9455'],
        [1, '#56703a'],
      ])
    },
    grade: '#fff0dc',
    gradeAlpha: 0.04,
  },
  couple: {
    light: (rng) => rng.range(0.1, 0.9),
    soft(ctx, w, h, rng) {
      if (rng.chance(0.62)) {
        vGradient(ctx, w, 0, h * 0.64, [
          [0, '#f3d6a8'],
          [0.6, '#f1b57a'],
          [1, '#e79a60'],
        ])
        const sx = rng.range(0.12, 0.88) * w
        glow(ctx, sx, h * 0.56, w * 0.6, '#fff1cc', 0.95)
        ctx.fillStyle = 'rgba(140,105,74,0.55)'
        ctx.beginPath()
        ctx.moveTo(0, h * 0.6)
        for (let i = 0; i <= 8; i++) ctx.lineTo((w * i) / 8, h * (0.56 + rng.range(-0.03, 0.03)))
        ctx.lineTo(w, h * 0.66)
        ctx.lineTo(0, h * 0.66)
        ctx.fill()
        vGradient(ctx, w, h * 0.62, h, [
          [0, '#d9b467'],
          [1, '#8f6a33'],
        ])
        for (let i = 0; i < 90; i++) {
          const x = rng.next() * w
          const y = h * rng.range(0.66, 1)
          ctx.strokeStyle = rng.chance(0.5) ? 'rgba(240,205,130,0.6)' : 'rgba(120,88,40,0.5)'
          ctx.lineWidth = Math.max(1, w * 0.003)
          ctx.beginPath()
          ctx.moveTo(x, y)
          ctx.lineTo(x + rng.range(-w * 0.01, w * 0.01), y - h * rng.range(0.04, 0.1))
          ctx.stroke()
        }
      } else {
        vGradient(ctx, w, 0, h, [
          [0, '#3c5a3c'],
          [1, '#22361f'],
        ])
        for (let i = 0; i < 40; i++) {
          bokeh(ctx, rng.next() * w, rng.next() * h * 0.8, h * rng.range(0.03, 0.09), rng.pick(['#e9d38f', '#9fbf6a', '#f6e7b5', '#5f8a4a']), rng.range(0.25, 0.7))
        }
        vGradient(ctx, w, h * 0.7, h, [
          [0, 'rgba(120,96,60,0)'],
          [1, 'rgba(120,96,60,0.8)'],
        ])
      }
    },
    mid(ctx, w, h, rng) {
      for (let i = 0; i < 4; i++) {
        const t = i / 4
        bokeh(ctx, w * (0.2 + t * 0.6), h * (0.2 + t * 0.5), h * rng.range(0.02, 0.05), '#fff1d0', 0.18)
      }
    },
    grade: '#ffb46e',
    gradeAlpha: 0.13,
  },
  cocktail: {
    light: () => 0.6,
    soft(ctx, w, h, rng) {
      vGradient(ctx, w, 0, h, [
        [0, '#86a46a'],
        [1, '#3d5833'],
      ])
      for (let i = 0; i < 46; i++) {
        bokeh(ctx, rng.next() * w, rng.next() * h * 0.85, h * rng.range(0.025, 0.09), rng.pick(['#c9df8f', '#f3efd0', '#6f9a52', '#e8f0c0', '#a3c47a']), rng.range(0.25, 0.65))
      }
      ctx.fillStyle = 'rgba(246,243,236,0.92)'
      ctx.beginPath()
      ctx.moveTo(0, 0)
      ctx.lineTo(w, 0)
      ctx.lineTo(w, h * 0.08)
      ctx.quadraticCurveTo(w * 0.5, h * 0.2, 0, h * 0.08)
      ctx.fill()
      ctx.fillStyle = 'rgba(240,236,228,0.9)'
      ctx.fillRect(0, h * 0.82, w, h * 0.2)
    },
    mid(ctx, w, h, rng) {
      ctx.strokeStyle = 'rgba(60,50,40,0.35)'
      ctx.lineWidth = Math.max(1, w * 0.002)
      ctx.beginPath()
      ctx.moveTo(0, h * 0.12)
      ctx.quadraticCurveTo(w * 0.5, h * 0.3, w, h * 0.12)
      ctx.stroke()
      catenary(ctx, w, h * 0.12, h * 0.09, rng, h * 0.012, '#ffe6a8', 14)
    },
    grade: '#ffe0b0',
    gradeAlpha: 0.07,
  },
  dinner: {
    light: () => 0.5,
    soft(ctx, w, h, rng) {
      vGradient(ctx, w, 0, h, [
        [0, '#3d281b'],
        [1, '#1b110b'],
      ])
      glow(ctx, w * rng.range(0.3, 0.7), h * 0.1, w * 0.5, '#ffb35c', 0.35)
      for (let row = 0; row < 3; row++) {
        catenary(ctx, w, h * (0.04 + row * 0.08), h * 0.1, rng, h * 0.018, '#ffcf7a', 16)
      }
      for (let i = 0; i < 5; i++) {
        const x = w * (0.1 + i * 0.2) + rng.range(-w * 0.04, w * 0.04)
        ctx.fillStyle = 'rgba(222,208,186,0.45)'
        ctx.beginPath()
        ctx.ellipse(x, h * 0.62, w * 0.09, h * 0.035, 0, 0, Math.PI * 2)
        ctx.fill()
        bokeh(ctx, x, h * 0.58, h * 0.025, '#ffc46a', 0.9)
      }
      vGradient(ctx, w, h * 0.72, h, [
        [0, '#e5d9c6'],
        [1, '#b9a88f'],
      ])
    },
    grade: '#ff9f4a',
    gradeAlpha: 0.14,
  },
  party: {
    light: () => 0.5,
    soft(ctx, w, h, rng) {
      vGradient(ctx, w, 0, h, [
        [0, '#110e1b'],
        [1, '#07060b'],
      ])
      ctx.globalCompositeOperation = 'screen'
      const colors = ['#ff3fa4', '#3fd3ff', '#8f5bff', '#ffb347', '#ff5f6d']
      for (let i = 0; i < 5; i++) {
        const x = rng.range(0, 1) * w
        const c = rng.pick(colors)
        const g = ctx.createLinearGradient(x, 0, x, h)
        g.addColorStop(0, rgba(c, 0.55))
        g.addColorStop(1, rgba(c, 0))
        ctx.fillStyle = g
        ctx.beginPath()
        ctx.moveTo(x, -h * 0.05)
        const spreadX = rng.range(0.15, 0.35) * w
        const dir = rng.range(-0.4, 0.4) * w
        ctx.lineTo(x + dir - spreadX, h)
        ctx.lineTo(x + dir + spreadX, h)
        ctx.fill()
      }
      for (let i = 0; i < 26; i++) bokeh(ctx, rng.next() * w, rng.next() * h * 0.7, h * rng.range(0.015, 0.05), rng.pick(colors), rng.range(0.3, 0.8))
      glow(ctx, w * 0.5, h * 0.95, w * 0.6, '#8f5bff', 0.25)
      ctx.globalCompositeOperation = 'source-over'
    },
    grade: '#b38cff',
    gradeAlpha: 0.04,
  },
}

/** Backdrops of detail shots (rings on silk, bouquet on a table…). */
function detailBackdrop(detail: DetailKind, ctx: Ctx, w: number, h: number, rng: Rng): void {
  switch (detail) {
    case 'rings': {
      vGradient(ctx, w, 0, h, [
        [0, '#f0e7db'],
        [1, '#cdbca6'],
      ])
      ctx.lineWidth = h * 0.12
      for (let i = 0; i < 6; i++) {
        ctx.strokeStyle = i % 2 ? 'rgba(255,255,255,0.35)' : 'rgba(120,96,70,0.12)'
        ctx.beginPath()
        const y = h * (i / 5) + rng.range(-h * 0.05, h * 0.05)
        ctx.moveTo(-w * 0.1, y)
        ctx.bezierCurveTo(w * 0.3, y - h * 0.25, w * 0.6, y + h * 0.25, w * 1.1, y - h * 0.1)
        ctx.stroke()
      }
      break
    }
    case 'bouquet':
      BACKDROPS.cocktail.soft(ctx, w, h, rng)
      break
    case 'glasses':
      vGradient(ctx, w, 0, h, [
        [0, '#5a4630'],
        [1, '#2b1f14'],
      ])
      for (let i = 0; i < 30; i++) bokeh(ctx, rng.next() * w, rng.next() * h, h * rng.range(0.03, 0.1), rng.pick(['#ffd27a', '#fff0c4', '#f5b25c']), rng.range(0.3, 0.75))
      break
    case 'cake':
      BACKDROPS.dinner.soft(ctx, w, h, rng)
      break
    case 'table':
      vGradient(ctx, w, 0, h, [
        [0, '#efe9e0'],
        [1, '#ddd3c5'],
      ])
      break
  }
}

const backdropCache = new Map<string, HTMLCanvasElement>()

function renderBackdrop(shot: Shot, w: number, h: number): HTMLCanvasElement {
  const key = `${shot.kind}:${shot.detail ?? ''}:${shot.backdrop}:${w}x${h}`
  const cached = backdropCache.get(key)
  if (cached) return cached

  const out = document.createElement('canvas')
  out.width = w
  out.height = h
  const ctx = out.getContext('2d')
  if (!ctx) return out
  ctx.imageSmoothingQuality = 'high'

  const softW = Math.max(16, Math.round(w / 5))
  const softH = Math.max(16, Math.round(h / 5))
  const soft = surface('soft', softW, softH)
  const rng = createRng(shot.backdrop)
  if (shot.detail) detailBackdrop(shot.detail, soft.ctx, softW, softH, rng)
  else BACKDROPS[shot.kind].soft(soft.ctx, softW, softH, rng)
  ctx.drawImage(soft.canvas, 0, 0, w, h)

  const mid = BACKDROPS[shot.kind].mid
  if (mid && !shot.detail) {
    const midW = Math.max(16, Math.round(w / 2.5))
    const midH = Math.max(16, Math.round(h / 2.5))
    const m = surface('mid', midW, midH)
    mid(m.ctx, midW, midH, createRng(shot.backdrop ^ 0x5bd1e995))
    ctx.drawImage(m.canvas, 0, 0, w, h)
  }

  backdropCache.set(key, out)
  if (backdropCache.size > 48) {
    const first = backdropCache.keys().next().value
    if (first !== undefined) backdropCache.delete(first)
  }
  return out
}

// ---------------------------------------------------------------------------
// People
// ---------------------------------------------------------------------------

function headPath(ctx: Ctx, cx: number, cy: number, rx: number, ry: number): void {
  ctx.beginPath()
  ctx.moveTo(cx, cy - ry)
  ctx.bezierCurveTo(cx + rx * 1.12, cy - ry, cx + rx * 1.06, cy + ry * 0.35, cx + rx * 0.56, cy + ry * 0.84)
  ctx.quadraticCurveTo(cx, cy + ry * 1.1, cx - rx * 0.56, cy + ry * 0.84)
  ctx.bezierCurveTo(cx - rx * 1.06, cy + ry * 0.35, cx - rx * 1.12, cy - ry, cx, cy - ry)
  ctx.closePath()
}

function isSuit(outfit: string): boolean {
  const [r, g, b] = hexToRgb(outfit)
  return r + g + b < 260
}

function drawPerson(ctx: Ctx, p: Person, w: number, h: number, light: number, kind: SceneKind): void {
  const headH = p.size * h
  const headW = headH * 0.76
  const rx = headW / 2
  const ry = headH / 2
  const cx = p.x * w
  const cy = p.y * h
  const lightDir = light < 0.5 ? -1 : 1
  const bottom = cy + headH * 8
  const sw = headH * (p.role === 'child' ? 1.7 : p.role === 'bride' ? 2.05 : 2.35)
  const neckY = cy + ry * 0.7
  const shoulderY = cy + headH * 0.98

  // Veil and long hair sit behind the body.
  if (p.veil) {
    const g = ctx.createLinearGradient(0, cy - ry, 0, bottom)
    g.addColorStop(0, 'rgba(255,255,255,0.5)')
    g.addColorStop(1, 'rgba(255,255,255,0.08)')
    ctx.fillStyle = g
    ctx.beginPath()
    ctx.moveTo(cx - rx * 0.6, cy - ry * 0.9)
    ctx.quadraticCurveTo(cx - sw * 0.9, cy + headH * 2, cx - sw * 0.95, bottom)
    ctx.lineTo(cx + sw * 0.95, bottom)
    ctx.quadraticCurveTo(cx + sw * 0.9, cy + headH * 2, cx + rx * 0.6, cy - ry * 0.9)
    ctx.fill()
  }
  if (p.hairStyle === 'long') {
    ctx.fillStyle = shade(p.hair, -0.12)
    ctx.beginPath()
    ctx.moveTo(cx - rx * 1.1, cy - ry * 0.3)
    ctx.quadraticCurveTo(cx - rx * 1.45, cy + headH * 1.1, cx - rx * 1.2, cy + headH * 1.55)
    ctx.lineTo(cx + rx * 1.2, cy + headH * 1.55)
    ctx.quadraticCurveTo(cx + rx * 1.45, cy + headH * 1.1, cx + rx * 1.1, cy - ry * 0.3)
    ctx.fill()
  }

  // Body: shoulders, waist, then a flared skirt or straight jacket.
  const suit = p.role === 'groom' || p.role === 'officiant' || (p.role !== 'bride' && p.hairStyle !== 'long' && isSuit(p.outfit))
  const dress = !suit
  const waistY = shoulderY + headH * 1.85
  const wx = sw * (dress ? 0.34 : 0.43)
  const bx = sw * (dress ? (p.role === 'bride' ? 0.95 : 0.62) : 0.47)
  const bodyPath = () => {
    ctx.beginPath()
    ctx.moveTo(cx - headW * 0.24, neckY)
    ctx.quadraticCurveTo(cx - sw * 0.42, shoulderY - headH * 0.06, cx - sw * 0.5, shoulderY + headH * 0.28)
    ctx.quadraticCurveTo(cx - sw * 0.48, waistY - headH * 0.55, cx - wx, waistY)
    ctx.quadraticCurveTo(cx - bx * 0.85, waistY + headH * 1.4, cx - bx, bottom)
    ctx.lineTo(cx + bx, bottom)
    ctx.quadraticCurveTo(cx + bx * 0.85, waistY + headH * 1.4, cx + wx, waistY)
    ctx.quadraticCurveTo(cx + sw * 0.48, waistY - headH * 0.55, cx + sw * 0.5, shoulderY + headH * 0.28)
    ctx.quadraticCurveTo(cx + sw * 0.42, shoulderY - headH * 0.06, cx + headW * 0.24, neckY)
    ctx.closePath()
  }
  const sideGradient = (color: string, strength: number) => {
    const g = ctx.createLinearGradient(cx - sw * 0.6, 0, cx + sw * 0.6, 0)
    g.addColorStop(0, shade(color, lightDir < 0 ? strength * 0.6 : -strength))
    g.addColorStop(0.5, color)
    g.addColorStop(1, shade(color, lightDir < 0 ? -strength : strength * 0.6))
    return g
  }

  if (dress) {
    // Skin of shoulders, then a sweetheart/V neckline dress.
    bodyPath()
    ctx.fillStyle = sideGradient(p.skin, 0.12)
    ctx.fill()
    ctx.save()
    bodyPath()
    ctx.clip()
    ctx.fillStyle = sideGradient(p.outfit, p.role === 'bride' ? 0.1 : 0.22)
    ctx.beginPath()
    const top = shoulderY + headH * (p.role === 'bride' ? 0.42 : 0.18)
    ctx.moveTo(cx - sw, top)
    ctx.quadraticCurveTo(cx - sw * 0.2, top - headH * 0.12, cx, top + headH * 0.12)
    ctx.quadraticCurveTo(cx + sw * 0.2, top - headH * 0.12, cx + sw, top)
    ctx.lineTo(cx + sw, bottom)
    ctx.lineTo(cx - sw, bottom)
    ctx.fill()
    if (p.role === 'bride') {
      ctx.strokeStyle = 'rgba(190,180,165,0.35)'
      ctx.lineWidth = Math.max(0.6, headH * 0.02)
      for (let i = -2; i <= 2; i++) {
        ctx.beginPath()
        ctx.moveTo(cx + i * sw * 0.12, top + headH * 0.5)
        ctx.lineTo(cx + i * sw * 0.18, bottom)
        ctx.stroke()
      }
    }
    ctx.restore()
  } else {
    bodyPath()
    ctx.fillStyle = sideGradient(p.outfit, 0.2)
    ctx.fill()
    // Shirt, lapels, tie
    ctx.fillStyle = '#f3f2ee'
    ctx.beginPath()
    ctx.moveTo(cx - headW * 0.26, neckY)
    ctx.lineTo(cx + headW * 0.26, neckY)
    ctx.lineTo(cx, shoulderY + headH * 0.95)
    ctx.closePath()
    ctx.fill()
    ctx.strokeStyle = shade(p.outfit, -0.35)
    ctx.lineWidth = Math.max(0.6, headH * 0.03)
    ctx.beginPath()
    ctx.moveTo(cx - headW * 0.3, neckY + headH * 0.05)
    ctx.lineTo(cx - headW * 0.02, shoulderY + headH * 0.95)
    ctx.moveTo(cx + headW * 0.3, neckY + headH * 0.05)
    ctx.lineTo(cx + headW * 0.02, shoulderY + headH * 0.95)
    ctx.stroke()
    ctx.fillStyle = p.role === 'groom' ? '#1b1b1f' : shade(p.outfit, -0.4)
    ctx.beginPath()
    const ty = neckY + headH * 0.12
    ctx.moveTo(cx, ty)
    ctx.lineTo(cx - headW * 0.17, ty - headH * 0.07)
    ctx.lineTo(cx - headW * 0.17, ty + headH * 0.07)
    ctx.closePath()
    ctx.moveTo(cx, ty)
    ctx.lineTo(cx + headW * 0.17, ty - headH * 0.07)
    ctx.lineTo(cx + headW * 0.17, ty + headH * 0.07)
    ctx.closePath()
    ctx.fill()
    if (p.role === 'groom') {
      bokeh(ctx, cx - sw * 0.28, shoulderY + headH * 0.45, headH * 0.09, '#fbf6ee', 1)
      bokeh(ctx, cx - sw * 0.25, shoulderY + headH * 0.52, headH * 0.06, '#7d9a6a', 1)
    }
  }

  // Arms hang along the torso; the bride holds her bouquet instead.
  const holdsBouquet = p.role === 'bride' && p.outfit !== '#efd9cf' && (kind === 'ceremony' || kind === 'couple' || kind === 'groups')
  if (!holdsBouquet && headH > 6) {
    const armColor = dress ? shade(p.skin, -0.04) : shade(p.outfit, -0.1)
    const aw = headH * (p.role === 'child' ? 0.3 : 0.36)
    for (const dir of [-1, 1]) {
      ctx.fillStyle = armColor
      ctx.beginPath()
      const ax = cx + dir * sw * 0.47
      ctx.moveTo(ax - (dir * aw) / 2, shoulderY + headH * 0.12)
      ctx.quadraticCurveTo(ax + dir * aw * 0.9, shoulderY + headH * 1.2, ax + dir * aw * 0.25, shoulderY + headH * 2.55)
      ctx.lineTo(ax - dir * aw * 0.65, shoulderY + headH * 2.5)
      ctx.quadraticCurveTo(ax - dir * aw * 0.1, shoulderY + headH * 1.2, ax - (dir * aw) / 2, shoulderY + headH * 0.12)
      ctx.fill()
      ctx.strokeStyle = 'rgba(0,0,0,0.12)'
      ctx.lineWidth = Math.max(0.5, headH * 0.02)
      ctx.stroke()
    }
  }
  if (holdsBouquet) {
    const by = shoulderY + headH * 1.9
    const flowers = ['#fbf6ee', '#f0c4bd', '#f3c7a1', '#ffffff', '#e8b4a8']
    for (let i = 0; i < 12; i++) {
      const a = (i / 12) * Math.PI * 2
      bokeh(ctx, cx + Math.cos(a) * headH * 0.42, by + Math.sin(a) * headH * 0.3, headH * 0.22, '#6f8f5c', 1)
    }
    for (let i = 0; i < 9; i++) {
      const a = (i / 9) * Math.PI * 2
      const d = i === 0 ? 0 : headH * 0.24
      bokeh(ctx, cx + Math.cos(a) * d, by + Math.sin(a) * d * 0.8, headH * 0.17, flowers[i % flowers.length], 1)
    }
  }

  // Neck
  ctx.fillStyle = shade(p.skin, -0.1)
  ctx.fillRect(cx - headW * 0.2, cy + ry * 0.4, headW * 0.4, neckY - cy - ry * 0.3 + headH * 0.06)

  // Head
  headPath(ctx, cx, cy, rx, ry)
  const hg = ctx.createRadialGradient(cx + lightDir * rx * 0.35, cy - ry * 0.3, ry * 0.1, cx, cy, ry * 1.2)
  hg.addColorStop(0, shade(p.skin, 0.1))
  hg.addColorStop(0.65, p.skin)
  hg.addColorStop(1, shade(p.skin, -0.16))
  ctx.fillStyle = hg
  ctx.fill()

  // Hair cap
  const tx = p.turn * rx * 0.25
  if (p.hairStyle !== 'bald') {
    ctx.fillStyle = p.hair
    ctx.beginPath()
    const temple = p.hairStyle === 'short' ? 0.05 : 0.3
    ctx.moveTo(cx - rx * 1.06, cy + ry * temple)
    ctx.bezierCurveTo(cx - rx * 1.2, cy - ry * 1.2, cx + rx * 1.2, cy - ry * 1.2, cx + rx * 1.06, cy + ry * temple)
    ctx.quadraticCurveTo(cx + rx * 0.8 + tx, cy - ry * 0.45, cx + tx, cy - ry * 0.6)
    ctx.quadraticCurveTo(cx - rx * 0.8 + tx, cy - ry * 0.45, cx - rx * 1.06, cy + ry * temple)
    ctx.fill()
    if (p.hairStyle === 'bun') {
      ctx.beginPath()
      ctx.arc(cx + tx * 1.4, cy - ry * 1.02, ry * 0.36, 0, Math.PI * 2)
      ctx.fill()
    }
    ctx.strokeStyle = 'rgba(255,255,255,0.12)'
    ctx.lineWidth = Math.max(0.5, headH * 0.03)
    ctx.beginPath()
    ctx.arc(cx + lightDir * rx * 0.2, cy - ry * 0.25, rx * 0.75, Math.PI * 1.15, Math.PI * 1.55)
    ctx.stroke()
  }

  // Face
  if (headH >= 9) {
    const fx = cx + p.turn * rx * 0.3
    const ey = cy - ry * 0.02
    const ex = rx * 0.42
    const browColor = shade(p.hair, -0.25)
    const eyeW = rx * 0.21
    const lw = Math.max(0.6, headH * 0.016)
    for (let side = 0; side < 2; side++) {
      const dir = side === 0 ? -1 : 1
      const squash = 1 - dir * p.turn * 0.3
      const x = fx + dir * ex * squash
      const open = p.eyes[side]
      ctx.strokeStyle = browColor
      ctx.lineWidth = lw * 1.1
      ctx.beginPath()
      ctx.moveTo(x - eyeW * 1.1, ey - ry * 0.2)
      ctx.quadraticCurveTo(x, ey - ry * 0.27, x + eyeW * 1.1, ey - ry * 0.19)
      ctx.stroke()
      if (open > 0.35) {
        const eh = ry * 0.085 * open
        ctx.fillStyle = '#f2ede8'
        ctx.beginPath()
        ctx.ellipse(x, ey, eyeW, eh, 0, 0, Math.PI * 2)
        ctx.fill()
        ctx.fillStyle = '#3a271c'
        ctx.beginPath()
        ctx.arc(x + p.turn * eyeW * 0.3, ey, Math.min(eh, eyeW * 0.55), 0, Math.PI * 2)
        ctx.fill()
        ctx.strokeStyle = 'rgba(40,25,20,0.75)'
        ctx.lineWidth = lw
        ctx.beginPath()
        ctx.ellipse(x, ey, eyeW, eh, 0, Math.PI * 1.05, Math.PI * 1.95)
        ctx.stroke()
      } else {
        ctx.strokeStyle = shade(p.skin, -0.5)
        ctx.lineWidth = lw * 1.2
        ctx.beginPath()
        ctx.moveTo(x - eyeW, ey - ry * 0.005)
        ctx.quadraticCurveTo(x, ey + ry * 0.07, x + eyeW, ey - ry * 0.005)
        ctx.stroke()
      }
    }
    ctx.strokeStyle = rgba('#5a3a2a', 0.28)
    ctx.lineWidth = lw
    ctx.beginPath()
    ctx.moveTo(fx + rx * 0.06, ey + ry * 0.1)
    ctx.lineTo(fx + rx * 0.1, ey + ry * 0.32)
    ctx.lineTo(fx - rx * 0.05, ey + ry * 0.36)
    ctx.stroke()
    const my = cy + ry * 0.52
    const mw = rx * (0.36 + p.smile * 0.12)
    ctx.fillStyle = p.smile > 0.65 ? '#f6f1ea' : rgba('#a7524e', 0.85)
    ctx.beginPath()
    ctx.moveTo(fx - mw, my)
    ctx.quadraticCurveTo(fx, my + ry * (0.08 + p.smile * 0.16), fx + mw, my)
    ctx.quadraticCurveTo(fx, my + ry * 0.04, fx - mw, my)
    ctx.fill()
    ctx.strokeStyle = rgba('#8c3f3c', 0.7)
    ctx.lineWidth = lw
    ctx.stroke()
    bokeh(ctx, fx - ex * 1.05, cy + ry * 0.3, rx * 0.3, '#f08a7a', 0.12)
    bokeh(ctx, fx + ex * 1.05, cy + ry * 0.3, rx * 0.3, '#f08a7a', 0.12)
  }

  if (kind === 'party') {
    ctx.save()
    headPath(ctx, cx, cy, rx, ry)
    ctx.clip()
    ctx.globalCompositeOperation = 'screen'
    glow(ctx, cx + lightDir * rx * 1.1, cy, rx * 1.2, '#ff4fb0', 0.45)
    ctx.restore()
  }
}

// ---------------------------------------------------------------------------
// Details
// ---------------------------------------------------------------------------

function drawDetail(ctx: Ctx, detail: DetailKind, w: number, h: number, rng: Rng): void {
  switch (detail) {
    case 'rings': {
      const s = Math.min(w, h)
      const rings = [
        { x: w * 0.44, y: h * 0.56, r: s * 0.17 },
        { x: w * 0.58, y: h * 0.49, r: s * 0.14 },
      ]
      rings.forEach(({ x, y, r }) => {
        ctx.fillStyle = 'rgba(80,60,40,0.18)'
        ctx.beginPath()
        ctx.ellipse(x + r * 0.15, y + r * 0.75, r * 1.05, r * 0.3, 0, 0, Math.PI * 2)
        ctx.fill()
      })
      rings.forEach(({ x, y, r }, i) => {
        const g = ctx.createLinearGradient(x - r, y - r, x + r, y + r)
        g.addColorStop(0, '#fbe7a6')
        g.addColorStop(0.45, '#d4a64a')
        g.addColorStop(0.7, '#f7dc92')
        g.addColorStop(1, '#9b6f22')
        ctx.strokeStyle = g
        ctx.lineWidth = r * (i === 0 ? 0.24 : 0.2)
        ctx.beginPath()
        ctx.ellipse(x, y, r, r * 0.78, -0.25, 0, Math.PI * 2)
        ctx.stroke()
        ctx.strokeStyle = 'rgba(255,255,255,0.55)'
        ctx.lineWidth = r * 0.04
        ctx.beginPath()
        ctx.ellipse(x, y, r * 1.06, r * 0.84, -0.25, Math.PI * 1.1, Math.PI * 1.6)
        ctx.stroke()
      })
      const d = rings[0]
      glow(ctx, d.x - d.r * 0.2, d.y - d.r * 0.82, d.r * 0.5, '#ffffff', 0.9)
      ctx.fillStyle = '#ffffff'
      ctx.beginPath()
      ctx.arc(d.x - d.r * 0.2, d.y - d.r * 0.82, d.r * 0.12, 0, Math.PI * 2)
      ctx.fill()
      break
    }
    case 'bouquet': {
      const cx = w * rng.range(0.42, 0.58)
      const cy = h * 0.48
      const s = Math.min(w, h)
      ctx.fillStyle = '#efe3d3'
      ctx.beginPath()
      ctx.moveTo(cx - s * 0.06, cy + s * 0.2)
      ctx.lineTo(cx + s * 0.06, cy + s * 0.2)
      ctx.lineTo(cx + s * 0.04, h)
      ctx.lineTo(cx - s * 0.04, h)
      ctx.fill()
      for (let i = 0; i < 16; i++) {
        const a = rng.range(0, Math.PI * 2)
        ctx.save()
        ctx.translate(cx + Math.cos(a) * s * 0.24, cy + Math.sin(a) * s * 0.2)
        ctx.rotate(a)
        ctx.fillStyle = rng.pick(['#5f7d4f', '#7d9a6a', '#4a6640', '#94ad7f'])
        ctx.beginPath()
        ctx.ellipse(0, 0, s * 0.09, s * 0.03, 0, 0, Math.PI * 2)
        ctx.fill()
        ctx.restore()
      }
      const palette = ['#f7f1e8', '#f0c4bd', '#f3c7a1', '#d99aa0', '#fffaf3', '#e8b4a8']
      for (let i = 0; i < 24; i++) {
        const a = rng.range(0, Math.PI * 2)
        const d = Math.sqrt(rng.next()) * s * 0.2
        const x = cx + Math.cos(a) * d
        const y = cy + Math.sin(a) * d * 0.85
        const r = s * rng.range(0.04, 0.065)
        const c = rng.pick(palette)
        const g = ctx.createRadialGradient(x - r * 0.3, y - r * 0.3, r * 0.1, x, y, r)
        g.addColorStop(0, shade(c, 0.15))
        g.addColorStop(1, shade(c, -0.12))
        ctx.fillStyle = g
        ctx.beginPath()
        ctx.arc(x, y, r, 0, Math.PI * 2)
        ctx.fill()
        ctx.strokeStyle = rgba('#8a5a50', 0.25)
        ctx.lineWidth = Math.max(0.6, r * 0.08)
        ctx.beginPath()
        ctx.arc(x, y, r * 0.55, 0.3, 4.2)
        ctx.arc(x, y, r * 0.28, 1.5, 5.5)
        ctx.stroke()
      }
      break
    }
    case 'glasses': {
      const s = Math.min(w, h)
      ;[
        { x: w * 0.42, tilt: 0.14 },
        { x: w * 0.58, tilt: -0.14 },
      ].forEach(({ x, tilt }) => {
        ctx.save()
        ctx.translate(x, h * 0.95)
        ctx.rotate(tilt)
        ctx.strokeStyle = 'rgba(255,255,255,0.55)'
        ctx.lineWidth = s * 0.008
        ctx.beginPath()
        ctx.moveTo(0, 0)
        ctx.lineTo(0, -h * 0.32)
        ctx.stroke()
        const g = ctx.createLinearGradient(-s * 0.06, 0, s * 0.06, 0)
        g.addColorStop(0, 'rgba(240,205,120,0.65)')
        g.addColorStop(0.5, 'rgba(255,232,160,0.85)')
        g.addColorStop(1, 'rgba(220,170,80,0.65)')
        ctx.fillStyle = g
        ctx.beginPath()
        ctx.ellipse(0, -h * 0.55, s * 0.065, h * 0.24, 0, 0, Math.PI * 2)
        ctx.fill()
        ctx.strokeStyle = 'rgba(255,255,255,0.7)'
        ctx.lineWidth = s * 0.005
        ctx.stroke()
        ctx.fillStyle = 'rgba(255,255,255,0.75)'
        for (let i = 0; i < 9; i++) {
          ctx.beginPath()
          ctx.arc(rng.range(-s * 0.03, s * 0.03), -h * rng.range(0.38, 0.72), s * 0.004, 0, Math.PI * 2)
          ctx.fill()
        }
        ctx.restore()
      })
      glow(ctx, w * 0.5, h * 0.28, s * 0.12, '#ffffff', 0.5)
      break
    }
    case 'cake': {
      const s = Math.min(w, h)
      const cx = w * 0.5
      let y = h * 0.85
      ctx.fillStyle = '#c9a75e'
      ctx.beginPath()
      ctx.ellipse(cx, y + s * 0.02, s * 0.3, s * 0.05, 0, 0, Math.PI * 2)
      ctx.fill()
      ;[0.26, 0.2, 0.14].forEach((r) => {
        const tierH = s * 0.17
        const g = ctx.createLinearGradient(cx - s * r, 0, cx + s * r, 0)
        g.addColorStop(0, '#d9d2c6')
        g.addColorStop(0.4, '#faf7f1')
        g.addColorStop(1, '#cfc6b8')
        ctx.fillStyle = g
        ctx.fillRect(cx - s * r, y - tierH, s * r * 2, tierH)
        ctx.beginPath()
        ctx.ellipse(cx, y, s * r, s * r * 0.18, 0, 0, Math.PI)
        ctx.fill()
        ctx.fillStyle = '#fbf9f4'
        ctx.beginPath()
        ctx.ellipse(cx, y - tierH, s * r, s * r * 0.18, 0, 0, Math.PI * 2)
        ctx.fill()
        y -= tierH
      })
      for (let i = 0; i < 9; i++) {
        bokeh(ctx, cx + s * rng.range(0.05, 0.2), h * 0.85 - s * rng.range(0.1, 0.5), s * rng.range(0.02, 0.035), rng.pick(['#f0c4bd', '#d99aa0', '#7d9a6a']), 1)
      }
      break
    }
    case 'table': {
      const s = Math.min(w, h)
      for (let i = 0; i < 40; i++) {
        ctx.save()
        ctx.translate(w * rng.range(0.05, 0.95), h * 0.5 + rng.range(-s * 0.06, s * 0.06))
        ctx.rotate(rng.range(-0.8, 0.8))
        ctx.fillStyle = rng.pick(['#6d8a5a', '#8aa676', '#55704a'])
        ctx.beginPath()
        ctx.ellipse(0, 0, s * 0.05, s * 0.018, 0, 0, Math.PI * 2)
        ctx.fill()
        ctx.restore()
      }
      ;[0.25, 0.75].forEach((px) => {
        ;[0.22, 0.8].forEach((py) => {
          const x = w * px
          const y = h * py
          ctx.fillStyle = 'rgba(120,100,80,0.15)'
          ctx.beginPath()
          ctx.arc(x + s * 0.01, y + s * 0.015, s * 0.15, 0, Math.PI * 2)
          ctx.fill()
          ctx.fillStyle = '#fbfaf7'
          ctx.beginPath()
          ctx.arc(x, y, s * 0.15, 0, Math.PI * 2)
          ctx.fill()
          ctx.strokeStyle = 'rgba(200,170,110,0.7)'
          ctx.lineWidth = s * 0.006
          ctx.beginPath()
          ctx.arc(x, y, s * 0.12, 0, Math.PI * 2)
          ctx.stroke()
          ctx.fillStyle = '#b9b6b0'
          ctx.fillRect(x - s * 0.2, y - s * 0.09, s * 0.012, s * 0.18)
          ctx.fillRect(x + s * 0.19, y - s * 0.09, s * 0.012, s * 0.18)
        })
      })
      for (let i = 0; i < 4; i++) {
        const x = w * (0.15 + i * 0.23)
        glow(ctx, x, h * 0.5, s * 0.08, '#ffcf7a', 0.6)
        ctx.fillStyle = '#f6efe0'
        ctx.beginPath()
        ctx.arc(x, h * 0.5, s * 0.022, 0, Math.PI * 2)
        ctx.fill()
      }
      break
    }
  }
}

// ---------------------------------------------------------------------------
// Grain
// ---------------------------------------------------------------------------

let grainTile: HTMLCanvasElement | null = null

function grain(): HTMLCanvasElement {
  if (grainTile) return grainTile
  const c = document.createElement('canvas')
  c.width = 128
  c.height = 128
  const ctx = c.getContext('2d')
  if (ctx) {
    const img = ctx.createImageData(128, 128)
    const rng = createRng(1234)
    for (let i = 0; i < img.data.length; i += 4) {
      const v = Math.round(128 + rng.gauss() * 40)
      img.data[i] = v
      img.data[i + 1] = v
      img.data[i + 2] = v
      img.data[i + 3] = 255
    }
    ctx.putImageData(img, 0, 0)
  }
  grainTile = c
  return c
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function shotSize(shot: Shot, longEdge: number): { width: number; height: number } {
  return shot.aspect >= 1
    ? { width: longEdge, height: Math.round(longEdge / shot.aspect) }
    : { width: Math.round(longEdge * shot.aspect), height: longEdge }
}

/** Renders `shot` at `longEdge` pixels into `target` (or a new canvas). */
export function renderShot(shot: Shot, longEdge: number, target?: HTMLCanvasElement): HTMLCanvasElement {
  const { width: w, height: h } = shotSize(shot, longEdge)
  const out = target ?? document.createElement('canvas')
  out.width = w
  out.height = h
  const ctx = out.getContext('2d')
  if (!ctx) return out
  ctx.imageSmoothingQuality = 'high'
  const rng = createRng(shot.seed)
  const backdrop = BACKDROPS[shot.kind]
  const light = backdrop.light(createRng(shot.backdrop))

  ctx.drawImage(renderBackdrop(shot, w, h), 0, 0)

  if (shot.detail) drawDetail(ctx, shot.detail, w, h, createRng(shot.backdrop ^ 0x27d4eb2f))

  if (shot.people.length) {
    const maxSize = Math.max(...shot.people.map((p) => p.size))
    const far = shot.people.filter((p) => p.size < maxSize * 0.72)
    if (far.length) {
      const f = 2.4
      const farLayer = surface('people-far', Math.max(4, Math.round(w / f)), Math.max(4, Math.round(h / f)))
      far.forEach((p) => drawPerson(farLayer.ctx, p, farLayer.canvas.width, farLayer.canvas.height, light, shot.kind))
      ctx.drawImage(farLayer.canvas, 0, 0, w, h)
    }
    const layer = surface('people', w, h)
    shot.people.filter((p) => !far.includes(p)).forEach((p) => drawPerson(layer.ctx, p, w, h, light, shot.kind))
    if (shot.subjectDefocus > 0.05) {
      const f = 1 + shot.subjectDefocus * 9
      const small = surface('people-blur', Math.max(4, Math.round(w / f)), Math.max(4, Math.round(h / f)))
      small.ctx.drawImage(layer.canvas, 0, 0, small.canvas.width, small.canvas.height)
      ctx.drawImage(small.canvas, 0, 0, w, h)
    } else {
      ctx.drawImage(layer.canvas, 0, 0)
    }
  }

  // Focus miss: downscale / upscale the whole frame.
  if (shot.defocus > 0.05) {
    const f = 1 + shot.defocus * 11
    const small = surface('defocus', Math.max(4, Math.round(w / f)), Math.max(4, Math.round(h / f)))
    small.ctx.drawImage(out, 0, 0, small.canvas.width, small.canvas.height)
    ctx.clearRect(0, 0, w, h)
    ctx.drawImage(small.canvas, 0, 0, w, h)
  }

  // Motion blur: running average of shifted copies.
  if (shot.motion > 0.05) {
    const copy = surface('motion', w, h)
    copy.ctx.drawImage(out, 0, 0)
    const passes = 7
    const len = shot.motion * w * 0.035
    const ax = Math.cos(shot.motionAngle)
    const ay = Math.sin(shot.motionAngle)
    for (let i = 1; i < passes; i++) {
      const t = i / (passes - 1) - 0.5
      ctx.globalAlpha = 1 / (i + 1)
      ctx.drawImage(copy.canvas, ax * len * t * 2, ay * len * t * 2)
    }
    ctx.globalAlpha = 1
  }

  // Grade
  ctx.globalCompositeOperation = 'soft-light'
  ctx.fillStyle = rgba(backdrop.grade, backdrop.gradeAlpha * 3)
  ctx.fillRect(0, 0, w, h)
  ctx.globalCompositeOperation = 'source-over'

  if (shot.exposure > 0.05) {
    ctx.globalCompositeOperation = 'screen'
    ctx.fillStyle = `rgba(255,250,242,${clamp(shot.exposure * 0.78, 0, 0.85)})`
    ctx.fillRect(0, 0, w, h)
    ctx.globalCompositeOperation = 'source-over'
  } else if (shot.exposure < -0.05) {
    ctx.globalCompositeOperation = 'multiply'
    ctx.fillStyle = `rgba(22,20,28,${clamp(-shot.exposure * 0.85, 0, 0.9)})`
    ctx.fillRect(0, 0, w, h)
    ctx.globalCompositeOperation = 'source-over'
  }

  const max = Math.max(w, h)
  const v = ctx.createRadialGradient(w / 2, h / 2, max * 0.35, w / 2, h / 2, max * 0.78)
  v.addColorStop(0, 'rgba(0,0,0,0)')
  v.addColorStop(1, `rgba(0,0,0,${shot.kind === 'party' ? 0.5 : 0.32})`)
  ctx.fillStyle = v
  ctx.fillRect(0, 0, w, h)

  const pattern = ctx.createPattern(grain(), 'repeat')
  if (pattern) {
    ctx.globalCompositeOperation = 'overlay'
    ctx.globalAlpha = 0.07 + rng.range(0, 0.02)
    ctx.fillStyle = pattern
    ctx.fillRect(0, 0, w, h)
    ctx.globalAlpha = 1
    ctx.globalCompositeOperation = 'source-over'
  }
  return out
}

/** Renders and encodes to a JPEG object URL. */
export function renderShotUrl(shot: Shot, longEdge: number, quality = 0.84): Promise<string> {
  const canvas = renderShot(shot, longEdge)
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (!blob) reject(new Error('Encoding failed'))
        else resolve(URL.createObjectURL(blob))
      },
      'image/jpeg',
      quality,
    )
  })
}

export function canRender(): boolean {
  return typeof document !== 'undefined' && typeof document.createElement === 'function'
}
