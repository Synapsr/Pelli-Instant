import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '@/lib/cn'
import { IconBox } from './IconBox'

/** Empty / error state (Pelli design system "Empty State"). */
export function EmptyState({
  icon,
  title,
  body,
  action,
  tone = 'accent',
  className,
}: {
  icon: LucideIcon
  title: string
  body?: ReactNode
  action?: ReactNode
  tone?: 'accent' | 'warning' | 'danger'
  className?: string
}) {
  return (
    <div
      className={cn(
        'mx-auto flex max-w-md flex-col items-center rounded-2xl border border-[#5840f0]/10 bg-gradient-to-br from-[#5840f0]/[0.04] via-transparent to-[#5840f0]/[0.04] px-8 py-10 text-center dark:border-[#5840f0]/20 dark:from-[#5840f0]/[0.08] dark:to-[#5840f0]/[0.06]',
        'animate-rise-in',
        className,
      )}
    >
      <IconBox icon={icon} size="lg" tone={tone} />
      <p className="mt-4 text-[15px] font-semibold text-foreground">{title}</p>
      {body && <div className="mt-1.5 text-sm text-default-500">{body}</div>}
      {action && <div className="mt-5">{action}</div>}
    </div>
  )
}
