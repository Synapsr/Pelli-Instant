import { useAppStore } from '@/store/app-store'

/** macOS (Tauri `appInfo.platform`, or the browser's guess). */
export function isMac(): boolean {
  const platform = useAppStore.getState().appInfo?.platform
  if (platform && platform !== 'web') return platform === 'macos'
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
}

/** Label of the command modifier: ⌘ on macOS, Ctrl elsewhere. */
export function modLabel(): string {
  return isMac() ? '⌘' : 'Ctrl'
}
