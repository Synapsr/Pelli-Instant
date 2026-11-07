import { motion, useReducedMotion } from 'framer-motion'

/** One soft light: where it sits (viewport units), how big and how bright. */
interface Light {
  x: number
  y: number
  size: number
  opacity: number
}

/**
 * Three lights per step, Pelli violets only: the brand violet, its lighter
 * tint and a deep indigo. Each step moves them where its visual is, so the
 * light travels with the story (the orb, the scanned photos, the keys, the
 * search field, the Lightroom window, the two precision cards, the plan).
 */
const MOODS: Light[][] = [
  // welcome: a halo around the orb, a faint lilac rising from below
  [
    { x: 50, y: 34, size: 70, opacity: 0.55 },
    { x: 80, y: 90, size: 50, opacity: 0.22 },
    { x: 12, y: 75, size: 55, opacity: 0.35 },
  ],
  // eye: the light sweeps across the row of photos
  [
    { x: 28, y: 38, size: 60, opacity: 0.42 },
    { x: 78, y: 32, size: 52, opacity: 0.3 },
    { x: 50, y: 95, size: 70, opacity: 0.3 },
  ],
  // keys: under the photo being sorted
  [
    { x: 50, y: 52, size: 58, opacity: 0.5 },
    { x: 18, y: 18, size: 46, opacity: 0.2 },
    { x: 86, y: 70, size: 60, opacity: 0.32 },
  ],
  // search: a wide light along the search field
  [
    { x: 50, y: 18, size: 80, opacity: 0.48 },
    { x: 14, y: 70, size: 50, opacity: 0.2 },
    { x: 88, y: 82, size: 56, opacity: 0.32 },
  ],
  // export: towards the Lightroom window
  [
    { x: 62, y: 40, size: 66, opacity: 0.46 },
    { x: 20, y: 88, size: 50, opacity: 0.24 },
    { x: 92, y: 10, size: 50, opacity: 0.3 },
  ],
  // precision: one light behind each card
  [
    { x: 36, y: 40, size: 50, opacity: 0.44 },
    { x: 66, y: 42, size: 50, opacity: 0.3 },
    { x: 50, y: 100, size: 80, opacity: 0.3 },
  ],
  // plan: gathered and brighter, behind the orb or the ring
  [
    { x: 50, y: 28, size: 62, opacity: 0.6 },
    { x: 30, y: 80, size: 46, opacity: 0.22 },
    { x: 72, y: 80, size: 46, opacity: 0.3 },
  ],
]

const COLOURS = ['rgba(88, 64, 240, 0.9)', 'rgba(165, 153, 245, 0.55)', 'rgba(42, 31, 168, 0.9)']

/**
 * Background light of the welcome tour: three blurred glows that glide to
 * new places at every step, slowly enough to feel like light, not motion.
 * Rendered inside <Backdrop>, so they also lean towards the pointer.
 */
export function Aura({ step }: { step: number }) {
  const reduce = useReducedMotion()
  const mood = MOODS[Math.min(step, MOODS.length - 1)]
  return (
    <>
      {mood.map((light, i) => (
        <motion.span
          key={i}
          aria-hidden
          className="absolute left-0 top-0 rounded-full will-change-transform"
          style={{
            width: '100vmax',
            height: '100vmax',
            marginLeft: '-50vmax',
            marginTop: '-50vmax',
            background: `radial-gradient(circle, ${COLOURS[i]}, transparent 62%)`,
            filter: 'blur(60px)',
          }}
          initial={false}
          animate={{
            x: `${light.x}vw`,
            y: `${light.y}vh`,
            scale: light.size / 100,
            opacity: light.opacity,
          }}
          transition={reduce ? { duration: 0 } : { type: 'spring', duration: 2.2, bounce: 0 }}
        />
      ))}
    </>
  )
}
