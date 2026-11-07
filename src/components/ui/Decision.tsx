import { CircleCheck, CircleDashed, CircleX, type LucideIcon } from 'lucide-react'
import type { FinalDecision, PhotoView } from '@/lib/contract'
import { cn } from '@/lib/cn'

export const DECISION_TEXT: Record<FinalDecision, string> = {
  selected: 'text-success',
  toReview: 'text-warning',
  rejected: 'text-danger',
}

export const DECISION_BG: Record<FinalDecision, string> = {
  selected: 'bg-success',
  toReview: 'bg-warning',
  rejected: 'bg-danger',
}

export const DECISION_ICON: Record<FinalDecision, LucideIcon> = {
  selected: CircleCheck,
  toReview: CircleDashed,
  rejected: CircleX,
}

export function isUserDecision(photo: PhotoView): boolean {
  return photo.mark.flag !== 'none'
}

/**
 * Traffic-light dot. A proposal of the AI is a soft dot with a halo; the
 * photographer's own flag is a solid dot with a crisp ring.
 */
export function DecisionDot({
  decision,
  user = false,
  pending = false,
  size = 8,
  className,
}: {
  decision: FinalDecision
  user?: boolean
  pending?: boolean
  size?: number
  className?: string
}) {
  if (pending) {
    return <span aria-hidden className={cn('inline-block shrink-0 rounded-full bg-default-300 animate-pulse-soft', className)} style={{ width: size, height: size }} />
  }
  return (
    <span
      aria-hidden
      className={cn(
        'inline-block shrink-0 rounded-full',
        DECISION_BG[decision],
        user ? 'ring-2 ring-offset-1 ring-offset-transparent ring-current' : 'opacity-90 shadow-[0_0_0_3px_color-mix(in_srgb,currentColor_22%,transparent)]',
        DECISION_TEXT[decision],
        className,
      )}
      style={{ width: size, height: size }}
    />
  )
}
