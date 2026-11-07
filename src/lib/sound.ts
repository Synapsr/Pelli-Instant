/**
 * Small interface sounds, synthesised with Web Audio (no audio file, no
 * licence): a bright bell arpeggio when an analysis ends, a softer two-note
 * chime when an export ends, and a tiny wooden tap when a photo is kept
 * (high, bright) or rejected (low, muted), a soft breath that blooms
 * into a glassy chord when the home screen arrives, and a faint crystal
 * "tock" per letter for text typed by the welcome tour. Quiet by design, off
 * with the "Sounds" setting.
 */
import { usePrefsStore } from '@/store/prefs-store'

export type SoundName = 'analysisDone' | 'exportDone' | 'keep' | 'reject' | 'startup' | 'key'
type Chime = 'analysisDone' | 'exportDone'

let ctx: AudioContext | null = null
let master: GainNode | null = null

function context(): AudioContext | null {
  if (typeof window === 'undefined') return null
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext })
      .webkitAudioContext
  if (!Ctor) return null
  if (!ctx) {
    try {
      ctx = new Ctor()
    } catch {
      return null
    }
    // Soft room: a short feedback echo, low-passed, under the dry signal.
    master = ctx.createGain()
    master.gain.value = 0.9
    const delay = ctx.createDelay(1)
    delay.delayTime.value = 0.19
    const feedback = ctx.createGain()
    feedback.gain.value = 0.28
    const tone = ctx.createBiquadFilter()
    tone.type = 'lowpass'
    tone.frequency.value = 2600
    const wet = ctx.createGain()
    wet.gain.value = 0.32
    master.connect(ctx.destination)
    master.connect(delay)
    delay.connect(tone)
    tone.connect(feedback)
    feedback.connect(delay)
    tone.connect(wet)
    wet.connect(ctx.destination)
  }
  return ctx
}

/**
 * WebKit starts audio suspended until a user gesture: resume on the first
 * pointer or key press so a sound at the end of a long analysis can play.
 */
export function unlockAudioOnFirstGesture(): void {
  if (typeof window === 'undefined') return
  const unlock = () => {
    const c = context()
    if (c && c.state === 'suspended') void c.resume()
    window.removeEventListener('pointerdown', unlock)
    window.removeEventListener('keydown', unlock)
  }
  window.addEventListener('pointerdown', unlock)
  window.addEventListener('keydown', unlock)
}

/** One bell note: sine fundamental + soft overtones, quick attack, long decay. */
function bell(
  c: AudioContext,
  out: AudioNode,
  freq: number,
  at: number,
  length: number,
  volume: number,
): void {
  const partials: [number, number][] = [
    [1, 1],
    [2, 0.22],
    [3, 0.06],
    [4.2, 0.025],
  ]
  for (const [ratio, level] of partials) {
    const osc = c.createOscillator()
    const gain = c.createGain()
    osc.type = 'sine'
    osc.frequency.value = freq * ratio
    const peak = volume * level
    gain.gain.setValueAtTime(0, at)
    gain.gain.linearRampToValueAtTime(peak, at + 0.008)
    // Higher partials fade faster, like a real bell.
    gain.gain.exponentialRampToValueAtTime(
      0.0001,
      at + length / Math.sqrt(ratio),
    )
    osc.connect(gain)
    gain.connect(out)
    osc.start(at)
    osc.stop(at + length + 0.05)
  }
}

const NOTES: Record<
  Chime,
  { freqs: number[]; step: number; length: number; volume: number }
> = {
  // E major arpeggio climbing to the octave: "done, and it went well".
  analysisDone: {
    freqs: [659.25, 830.61, 987.77, 1318.51],
    step: 0.085,
    length: 1.6,
    volume: 0.16,
  },
  // Two calm notes (fifth): "sent".
  exportDone: {
    freqs: [783.99, 1174.66],
    step: 0.11,
    length: 1.2,
    volume: 0.13,
  },
}

/**
 * A tuned wooden tap: inharmonic partials that die within a few tens of
 * milliseconds, a faint click of noise on the strike, an optional pitch
 * drop. Dry (no echo): it plays at every keep or reject, in quick bursts.
 */
interface Tap {
  freq: number
  /** pitch at the strike, gliding to `freq` */
  from: number
  decay: number
  volume: number
  lowpass: number
  click: number
}

const TAPS: Record<'keep' | 'reject', Tap> = {
  // E6, bright: "yes".
  keep: { freq: 1318.51, from: 1318.51, decay: 0.035, volume: 0.2, lowpass: 9000, click: 4000 },
  // B4, muted and falling slightly: "set aside".
  reject: { freq: 493.88, from: 560, decay: 0.045, volume: 0.32, lowpass: 1800, click: 1200 },
}

/** Wooden bar partials (ratio, level). */
const WOOD: [number, number][] = [
  [1, 1],
  [2.76, 0.18],
  [5.4, 0.05],
]

let noiseBuffer: AudioBuffer | null = null

/** 20 ms of white noise, made once. */
function whiteNoise(c: AudioContext): AudioBuffer {
  if (!noiseBuffer) {
    noiseBuffer = c.createBuffer(1, Math.round(c.sampleRate * 0.02), c.sampleRate)
    const data = noiseBuffer.getChannelData(0)
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1
  }
  return noiseBuffer
}

/**
 * One typed letter, motion-design style: a clean, very short crystal
 * "tock" (a damped sine near 1.9 kHz over a soft low pop and a 1 ms grain
 * of noise), its pitch varying a little from one letter to the next. Very
 * quiet: it accompanies the voice-over, it doesn't compete with it.
 */
function key(c: AudioContext): void {
  const at = c.currentTime + 0.002
  const vary = 0.95 + Math.random() * 0.1
  const level = 0.05 + Math.random() * 0.015
  const out = c.createGain()
  out.gain.value = level
  out.connect(c.destination)
  const partials: [number, number, number][] = [
    // frequency, level, decay (s)
    [1900 * vary, 1, 0.006],
    [420 * vary, 0.35, 0.01],
  ]
  for (const [freq, amp, decay] of partials) {
    const osc = c.createOscillator()
    const gain = c.createGain()
    osc.type = 'sine'
    osc.frequency.value = freq
    gain.gain.setValueAtTime(0.0001, at)
    gain.gain.linearRampToValueAtTime(amp, at + 0.0005)
    gain.gain.exponentialRampToValueAtTime(0.0001, at + decay * 7)
    osc.connect(gain)
    gain.connect(out)
    osc.start(at)
    osc.stop(at + decay * 7 + 0.01)
  }
  const grain = c.createBufferSource()
  grain.buffer = whiteNoise(c)
  const grainGain = c.createGain()
  grainGain.gain.setValueAtTime(0.25, at)
  grainGain.gain.exponentialRampToValueAtTime(0.0001, at + 0.0015)
  grain.connect(grainGain)
  grainGain.connect(out)
  grain.start(at)
  grain.stop(at + 0.005)
}

function tap(c: AudioContext, { freq, from, decay, volume, lowpass, click }: Tap): void {
  const at = c.currentTime + 0.005
  const tone = c.createBiquadFilter()
  tone.type = 'lowpass'
  tone.frequency.value = lowpass
  tone.connect(c.destination)
  for (const [ratio, level] of WOOD) {
    const osc = c.createOscillator()
    const gain = c.createGain()
    osc.type = 'sine'
    osc.frequency.setValueAtTime(from * ratio, at)
    osc.frequency.exponentialRampToValueAtTime(freq * ratio, at + 0.012)
    gain.gain.setValueAtTime(0, at)
    gain.gain.linearRampToValueAtTime(volume * level, at + 0.001)
    gain.gain.exponentialRampToValueAtTime(0.0001, at + decay / ratio)
    osc.connect(gain)
    gain.connect(tone)
    osc.start(at)
    osc.stop(at + decay + 0.02)
  }
  // The strike: a 2 ms grain of band-passed noise.
  const noise = c.createBufferSource()
  noise.buffer = whiteNoise(c)
  const band = c.createBiquadFilter()
  band.type = 'bandpass'
  band.frequency.value = click
  band.Q.value = 1.2
  const grain = c.createGain()
  grain.gain.setValueAtTime(volume * 0.35, at)
  grain.gain.exponentialRampToValueAtTime(0.0001, at + 0.012)
  noise.connect(band)
  band.connect(grain)
  grain.connect(tone)
  noise.start(at)
  noise.stop(at + 0.02)
}

/**
 * Arrival of the home screen, timed with the orb coming from afar: a breath
 * of air that rises for ARRIVAL_S, then a soft A major (add 9) chord of
 * glass bells blooming where the orb lands.
 */
export const ARRIVAL_S = 0.85

function startup(c: AudioContext, out: AudioNode): void {
  const at = c.currentTime + 0.03
  // Breath: band-passed noise sweeping up, swelling, then gone at landing.
  const length = ARRIVAL_S + 0.25
  const buffer = c.createBuffer(1, Math.round(c.sampleRate * length), c.sampleRate)
  const data = buffer.getChannelData(0)
  for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1
  const air = c.createBufferSource()
  air.buffer = buffer
  const band = c.createBiquadFilter()
  band.type = 'bandpass'
  band.Q.value = 0.9
  band.frequency.setValueAtTime(280, at)
  band.frequency.exponentialRampToValueAtTime(2400, at + ARRIVAL_S)
  const swell = c.createGain()
  swell.gain.setValueAtTime(0.0001, at)
  swell.gain.exponentialRampToValueAtTime(0.05, at + ARRIVAL_S * 0.85)
  swell.gain.exponentialRampToValueAtTime(0.0001, at + length)
  air.connect(band)
  band.connect(swell)
  swell.connect(out)
  air.start(at)
  air.stop(at + length)
  // Bloom: the chord, notes a breath apart.
  const land = at + ARRIVAL_S
  ;[440, 554.37, 659.25, 987.77].forEach((f, i) => bell(c, out, f, land + i * 0.035, 2.2, 0.085 - i * 0.012))
}

let pendingStartup = false

export function playSound(name: SoundName): void {
  if (!usePrefsStore.getState().sounds) return
  const c = context()
  if (!c || !master) return
  if (c.state === 'suspended') void c.resume()
  if (name === 'keep' || name === 'reject') {
    tap(c, TAPS[name])
    return
  }
  if (name === 'key') {
    key(c)
    return
  }
  if (name === 'startup') {
    // At launch the context may start suspended: only play if it wakes up
    // now, never late at the first click.
    if (c.state === 'running') return startup(c, master)
    if (pendingStartup) return
    pendingStartup = true
    const asked = performance.now()
    void c.resume().then(() => {
      pendingStartup = false
      if (c.state === 'running' && master && performance.now() - asked < 400) startup(c, master)
    })
    return
  }
  const { freqs, step, length, volume } = NOTES[name]
  const start = c.currentTime + 0.02
  freqs.forEach((f, i) =>
    bell(c, master as GainNode, f, start + i * step, length, volume),
  )
}
