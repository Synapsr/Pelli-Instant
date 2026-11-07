import type { Plan } from '@/lib/contract'
import { cn } from '@/lib/cn'

/** Discreet plan badge ("Pro", "Ultra", "Gratuit"), label localised by the server. */
export function PlanBadge({ plan, label, className }: { plan: Plan; label: string; className?: string }) {
  return (
    <span
      className={cn(
        'inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[10px] font-semibold uppercase tracking-wider',
        plan === 'free'
          ? 'bg-default-100 text-default-500'
          : 'bg-gradient-to-r from-[#5840f0]/15 to-[#7c6af0]/15 text-[#5840f0] ring-1 ring-[#5840f0]/20 dark:from-[#5840f0]/30 dark:to-[#7c6af0]/25 dark:text-[#c3b9fa] dark:ring-[#5840f0]/35',
        className,
      )}
    >
      {label}
    </span>
  )
}
