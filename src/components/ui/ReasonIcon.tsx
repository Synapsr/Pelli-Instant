import { Copy, EyeOff, Focus, Layers, Moon, Palette, ScanFace, Sun, Wind, type LucideIcon } from 'lucide-react'
import type { Reason } from '@/lib/contract'

export const REASON_ICON: Record<Reason, LucideIcon> = {
  blurry: Focus,
  motionBlur: Wind,
  subjectOutOfFocus: ScanFace,
  eyesClosed: EyeOff,
  underexposed: Moon,
  overexposed: Sun,
  duplicate: Layers,
  exactDuplicate: Copy,
  lowAesthetic: Palette,
}

export function ReasonIcon({ reason, className }: { reason: Reason; className?: string }) {
  const Icon = REASON_ICON[reason]
  return <Icon className={className ?? 'size-3.5'} strokeWidth={1.9} aria-hidden />
}
