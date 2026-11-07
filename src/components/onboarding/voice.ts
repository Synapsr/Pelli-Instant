import { useEffect, useRef, useState } from 'react'
import type { Language } from '@/lib/contract'

/*
 * Voice-over of the welcome tour: one line per step and language, made with
 * ElevenLabs (scripts/generate-voices.mjs) and shipped with the app. Only
 * the clips of the language in use are loaded.
 */
const CLIPS = import.meta.glob('../../assets/onboarding/voice/*/*.mp3', { query: '?url', import: 'default' }) as Record<string, () => Promise<string>>

const clip = (language: Language, step: string) => CLIPS[`../../assets/onboarding/voice/${language}/${step}.mp3`] ?? CLIPS[`../../assets/onboarding/voice/en/${step}.mp3`]

/** The visual of a step comes in first, then the voice. */
const LEAD_MS = 450
const FADE_MS = 160

function fadeOut(el: HTMLAudioElement) {
  if (el.paused) return
  const from = el.volume
  const start = performance.now()
  const tick = () => {
    const k = Math.min(1, (performance.now() - start) / FADE_MS)
    el.volume = from * (1 - k)
    if (k < 1) requestAnimationFrame(tick)
    else el.pause()
  }
  requestAnimationFrame(tick)
}

/**
 * Reads each step's line when the step appears, in the app's language. A
 * new step, a new language or muting cuts the current line with a short
 * fade. Where autoplay is blocked (a browser preview; the app allows it),
 * the line starts at the first click or key press.
 */
export function useNarration(step: string, language: Language, enabled: boolean): boolean {
  const audio = useRef<HTMLAudioElement | null>(null)
  const [speaking, setSpeaking] = useState(false)

  useEffect(() => {
    const el = (audio.current ??= new Audio())
    const on = () => setSpeaking(true)
    const off = () => setSpeaking(false)
    el.addEventListener('playing', on)
    el.addEventListener('pause', off)
    el.addEventListener('ended', off)
    return () => {
      el.removeEventListener('playing', on)
      el.removeEventListener('pause', off)
      el.removeEventListener('ended', off)
    }
  }, [])

  useEffect(() => {
    if (!enabled) return
    const el = (audio.current ??= new Audio())
    let cancelled = false
    const resume = () => {
      detach()
      if (!cancelled) void el.play().catch(() => {})
    }
    const detach = () => {
      window.removeEventListener('pointerdown', resume)
      window.removeEventListener('keydown', resume)
    }
    const timer = window.setTimeout(async () => {
      const load = clip(language, step)
      if (!load) return
      const url = await load()
      if (cancelled) return
      el.src = url
      el.volume = 1
      el.play().catch(() => {
        window.addEventListener('pointerdown', resume)
        window.addEventListener('keydown', resume)
      })
    }, LEAD_MS)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
      detach()
      fadeOut(el)
    }
  }, [step, language, enabled])

  return enabled && speaking
}

/** Fetches the clips of a language ahead of time (they are small and few). */
export function preloadNarration(language: Language) {
  for (const [path, load] of Object.entries(CLIPS)) {
    if (!path.includes(`/voice/${language}/`)) continue
    void load().then((url) => {
      const a = new Audio()
      a.preload = 'auto'
      a.src = url
    })
  }
}
