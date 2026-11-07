import type { ReactNode } from 'react'
import { windowButtonsInset } from '@/hooks/use-window-inset'
import { cn } from '@/lib/cn'

/**
 * Top bar drawn in the window's title bar area: drags the window, leaves room
 * for the macOS window buttons. Interactive children keep working (only the
 * bar's own background starts a drag).
 */
export function WindowBar({ children, className }: { children?: ReactNode; className?: string }) {
  return (
    <header
      data-tauri-drag-region
      className={cn('relative z-20 flex h-[52px] shrink-0 items-center gap-2 pr-3', className)}
      style={{ paddingLeft: Math.max(12, windowButtonsInset()) }}
    >
      {children}
    </header>
  )
}
