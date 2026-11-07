import { animate, useMotionValue } from 'framer-motion'
import { useEffect, useRef } from 'react'
import { formatNumber } from '@/lib/format'

/**
 * A number that glides to its new value (no re-render per frame: the text
 * node is updated directly). `from` makes it count up on mount.
 */
export function AnimatedNumber({ value, from, duration = 0.7, className }: { value: number; from?: number; duration?: number; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null)
  const motion = useMotionValue(from ?? value)

  useEffect(
    () =>
      motion.on('change', (v) => {
        if (ref.current) ref.current.textContent = formatNumber(Math.round(v))
      }),
    [motion],
  )

  useEffect(() => {
    const controls = animate(motion, value, { duration, ease: [0.22, 1, 0.36, 1] })
    return () => controls.stop()
  }, [motion, value, duration])

  return (
    <span ref={ref} className={className ? `tabular ${className}` : 'tabular'}>
      {formatNumber(Math.round(from ?? value))}
    </span>
  )
}
