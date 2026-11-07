import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/cn'

const SIZES = {
  xs: { box: 'size-6 rounded-md', icon: 'size-3.5' },
  sm: { box: 'size-8 rounded-lg', icon: 'size-4' },
  md: { box: 'size-10 rounded-xl', icon: 'size-5' },
  lg: { box: 'size-14 rounded-2xl', icon: 'size-7' },
} as const

const TONES = {
  accent: 'bg-gradient-to-br from-[#5840f0]/15 to-[#5840f0]/5 dark:from-[#5840f0]/25 dark:to-[#5840f0]/10 text-[#5840f0] dark:text-[#a599f5]',
  success: 'bg-gradient-to-br from-success/20 to-success/5 text-success',
  warning: 'bg-gradient-to-br from-warning/20 to-warning/5 text-warning',
  danger: 'bg-gradient-to-br from-danger/20 to-danger/5 text-danger',
  neutral: 'bg-default-100 text-default-500',
} as const

/** Icon in a soft gradient container (Pelli design system). */
export function IconBox({
  icon: Icon,
  size = 'md',
  tone = 'accent',
  className,
}: {
  icon: LucideIcon
  size?: keyof typeof SIZES
  tone?: keyof typeof TONES
  className?: string
}) {
  const s = SIZES[size]
  return (
    <span className={cn('inline-flex shrink-0 items-center justify-center', s.box, TONES[tone], className)} aria-hidden>
      <Icon className={s.icon} strokeWidth={1.8} />
    </span>
  )
}
