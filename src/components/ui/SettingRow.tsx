import type { ReactNode } from 'react'
import { cn } from '@/lib/cn'

/** A settings line: label and one-line hint on the left, the control on the right. */
export function SettingRow({ label, hint, icon, disabled = false, children }: { label: string; hint?: ReactNode; icon?: ReactNode; disabled?: boolean; children: ReactNode }) {
  return (
    <div className={cn('flex items-start justify-between gap-8 border-b border-default-200/60 py-4 last:border-b-0 dark:border-white/[0.06]', disabled && 'opacity-50')}>
      <div className="flex min-w-0 gap-3">
        {icon && <span className="mt-px shrink-0 text-default-400">{icon}</span>}
        <div className="min-w-0">
          <p className="text-[13px] font-medium text-foreground">{label}</p>
          {hint && <div className="mt-0.5 text-xs leading-snug text-default-500">{hint}</div>}
        </div>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}
