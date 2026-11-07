import { useEffect } from 'react'
import type { Theme } from '@/lib/contract'

function apply(dark: boolean): void {
  const root = document.documentElement
  root.classList.toggle('dark', dark)
  root.classList.toggle('light', !dark)
  root.dataset.theme = dark ? 'dark' : 'light'
  root.style.colorScheme = dark ? 'dark' : 'light'
}

/** Applies the app theme ("system" follows the OS live). Dark by default. */
export function useApplyTheme(theme: Theme | undefined): void {
  useEffect(() => {
    const mode = theme ?? 'dark'
    if (mode !== 'system') {
      apply(mode === 'dark')
      return
    }
    const query = window.matchMedia('(prefers-color-scheme: dark)')
    apply(query.matches)
    const onChange = (e: MediaQueryListEvent) => apply(e.matches)
    query.addEventListener('change', onChange)
    return () => query.removeEventListener('change', onChange)
  }, [theme])
}
