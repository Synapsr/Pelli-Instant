import { type ReactNode, useEffect, useRef } from 'react'

/**
 * Background of the home, analysis and sign-in screens: a deep night, two
 * slow violet glows leaning a little towards the pointer, a fine grain.
 * Pure CSS animations on transforms (GPU), paused by "reduce motion".
 */
export function Backdrop({ children }: { children?: ReactNode } = {}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = ref.current
    if (!el || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    let frame = 0
    let tx = 0
    let ty = 0
    let x = 0
    let y = 0
    const onMove = (e: PointerEvent) => {
      tx = e.clientX / window.innerWidth - 0.5
      ty = e.clientY / window.innerHeight - 0.5
      if (!frame) frame = requestAnimationFrame(tick)
    }
    // Eased follow: the glows drift after the pointer instead of snapping.
    const tick = () => {
      x += (tx - x) * 0.05
      y += (ty - y) * 0.05
      el.style.setProperty('--px', x.toFixed(4))
      el.style.setProperty('--py', y.toFixed(4))
      frame = Math.abs(tx - x) + Math.abs(ty - y) > 0.0005 ? requestAnimationFrame(tick) : 0
    }
    window.addEventListener('pointermove', onMove)
    return () => {
      window.removeEventListener('pointermove', onMove)
      cancelAnimationFrame(frame)
    }
  }, [])

  return (
    <div ref={ref} className="ambient" aria-hidden>
      <div className="ambient-layer">
        {/* A screen can bring its own light (the welcome tour moves it at each step). */}
        {children ?? (
          <>
            <span className="ambient-glow ambient-glow-1" />
            <span className="ambient-glow ambient-glow-2" />
          </>
        )}
      </div>
      <span className="ambient-grain" />
      <span className="ambient-vignette" />
    </div>
  )
}
